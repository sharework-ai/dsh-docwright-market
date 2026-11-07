//! DB-backed integration tests. `#[sqlx::test]` provisions a scratch
//! Postgres database per test and applies the real migrations
//! (`../../supabase/migrations` — 0000 stubs the GoTrue `auth` schema, a
//! no-op on a live Supabase). These cover the market's own rules: queue
//! dedup, version uniqueness/monotonicity, ownership, content-change
//! requirement, catalog upsert, and the review transition.

use sqlx::PgPool;
use uuid::Uuid;

use super::admin::{mark_reviewed, upsert_published_pack, SubmissionRow};
use super::submissions::check_submission_rules;
use crate::auth::AuthUser;
use crate::error::AppError;
use serde_json::{json, Value};

// Surface the swallowed sqlx errors behind AppError::internal during tests.
static INIT: std::sync::Once = std::sync::Once::new();
pub(crate) fn init_log() {
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .try_init();
    });
}

fn user(id: Uuid) -> AuthUser {
    AuthUser { id, email: Some("author@example.invalid".into()) }
}

pub(crate) async fn seed_user(pool: &PgPool, id: Uuid) {
    sqlx::query("insert into auth.users (id, email) values ($1, $2)")
        .bind(id)
        .bind("author@example.invalid")
        .execute(pool)
        .await
        .expect("seed auth user");
}

/// Profiles row with (optionally) a publisher org — submitting requires one.
pub(crate) async fn seed_profile(pool: &PgPool, id: Uuid, org: Option<&str>) {
    sqlx::query("insert into profiles (id, org) values ($1, $2)")
        .bind(id)
        .bind(org)
        .execute(pool)
        .await
        .expect("seed profile");
}

pub(crate) async fn seed_pack(pool: &PgPool, name: &str, version: &str, author_id: Uuid, hash: &str) {
    sqlx::query(
        "insert into packs (name, version, description, category, author, author_id, license, \
         content_hash, tarball_path, tarball_sha256) \
         values ($1, $2, $3, 'dev', 'author', $4, 'MIT', $5, 'tarballs/x.tgz', 'sha256:x')",
    )
    .bind(name)
    .bind(version)
    .bind(json!({ "en": "seed", "zh": "种子" }))
    .bind(author_id)
    .bind(hash)
    .execute(pool)
    .await
    .expect("seed pack");
}

pub(crate) async fn seed_submission(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    version: &str,
    uploader: Uuid,
    status: &str,
) {
    sqlx::query(
        "insert into submissions (id, pack_name, version, uploader, status, report) \
         values ($1, $2, $3, $4, $5, $6::jsonb)",
    )
    .bind(id)
    .bind(name)
    .bind(version)
    .bind(uploader)
    .bind(status)
    .bind(r#"{}"#)
    .execute(pool)
    .await
    .expect("seed submission");
}

fn err_text(error: AppError) -> String {
    error.message
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn first_submission_passes_all_rules(pool: PgPool) {
    init_log();
    let author = user(Uuid::new_v4());
    check_submission_rules(&pool, &author, "fresh-pack", "0.1.0", "sha256:a")
        .await
        .expect("an empty catalog accepts any first submission");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn pending_submission_blocks_the_queue(pool: PgPool) {
    init_log();
    let uploader = Uuid::new_v4();
    seed_user(&pool, uploader).await;
    seed_submission(&pool, Uuid::new_v4(), "busy-pack", "0.1.0", uploader, "pending").await;

    let error = check_submission_rules(&pool, &user(uploader), "busy-pack", "0.2.0", "sha256:b")
        .await
        .expect_err("a pending same-name submission must block");
    assert!(err_text(error).contains("awaiting review"));
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn reused_version_is_rejected_even_after_review(pool: PgPool) {
    init_log();
    let uploader = Uuid::new_v4();
    seed_user(&pool, uploader).await;
    seed_submission(&pool, Uuid::new_v4(), "old-pack", "0.3.0", uploader, "approved").await;

    let error = check_submission_rules(&pool, &user(uploader), "old-pack", "0.3.0", "sha256:c")
        .await
        .expect_err("a (name, version) pair must be unique forever");
    assert!(err_text(error).contains("bump the version"));
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn updates_require_the_original_author(pool: PgPool) {
    init_log();
    let owner = Uuid::new_v4();
    let other = Uuid::new_v4();
    seed_user(&pool, owner).await;
    seed_user(&pool, other).await;
    seed_pack(&pool, "owned-pack", "0.1.0", owner, "sha256:a").await;

    let error = check_submission_rules(&pool, &user(other), "owned-pack", "0.2.0", "sha256:b")
        .await
        .expect_err("another account must not update someone else's pack");
    assert!(err_text(error).contains("another account"));
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn version_must_strictly_increase(pool: PgPool) {
    init_log();
    let owner = Uuid::new_v4();
    seed_user(&pool, owner).await;
    seed_pack(&pool, "stable-pack", "0.2.0", owner, "sha256:a").await;

    let same = check_submission_rules(&pool, &user(owner), "stable-pack", "0.2.0", "sha256:b")
        .await
        .expect_err("same version must not pass");
    assert!(err_text(same).contains("strictly increase"));

    let lower = check_submission_rules(&pool, &user(owner), "stable-pack", "0.1.9", "sha256:b")
        .await
        .expect_err("lower version must not pass");
    assert!(err_text(lower).contains("strictly increase"));

    check_submission_rules(&pool, &user(owner), "stable-pack", "0.3.0", "sha256:b")
        .await
        .expect("higher version passes");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn identical_content_is_rejected(pool: PgPool) {
    init_log();
    let owner = Uuid::new_v4();
    seed_user(&pool, owner).await;
    seed_pack(&pool, "frozen-pack", "0.1.0", owner, "sha256:same").await;

    let error = check_submission_rules(&pool, &user(owner), "frozen-pack", "0.2.0", "sha256:same")
        .await
        .expect_err("same content under a bumped version is a no-op upload");
    assert!(err_text(error).contains("identical"));
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn first_publish_lands_the_catalog_row_and_the_history(pool: PgPool) {
    init_log();
    let uploader = Uuid::new_v4();
    seed_user(&pool, uploader).await;
    seed_profile(&pool, uploader, Some("sharework-ai")).await;
    let sub = SubmissionRow {
        id: Uuid::new_v4(),
        pack_name: "published-pack".into(),
        version: "0.1.0".into(),
        uploader,
        archive_path: Some("archive.tgz".into()),
        report: json!({}),
        category: Some("dev".into()),
        author: Some("sharework-ai".into()),
        license: Some("MIT".into()),
        homepage: None,
    };
    let manifest = json!({
        "description": { "en": "eng", "zh": "中文" },
        "consumes": ["cosmic-func"],
        "languages": ["zh-CN"],
        "requiresTopic": true,
        "artifactPrefix": "CARD",
    });
    upsert_published_pack(&pool, &sub, &manifest, "sha256:h1", true, "tarballs/p-0.1.0.tgz", "sha256:t1", "sharework-ai")
        .await
        .expect("first publish writes the row");

    let row: (String, Value, bool, bool, String, String, Option<String>, String) = sqlx::query_as(
        "select version, description, has_function_rules, requires_topic, category, author, \
         org, status from packs where name = 'published-pack'",
    )
    .fetch_one(&pool)
    .await
    .expect("row must exist");
    assert_eq!(row.0, "0.1.0");
    assert_eq!(row.1, json!({ "en": "eng", "zh": "中文" }));
    assert!(row.2, "has_function_rules from the report");
    assert!(row.3, "requiresTopic from the manifest");
    assert_eq!(row.4, "dev");
    assert_eq!(row.5, "sharework-ai");
    assert_eq!(row.6.as_deref(), Some("sharework-ai"), "org stamped from the uploader profile");
    assert_eq!(row.7, "active", "new packs start active");

    // The publish also opened the version history, carrying provenance.
    let (versions, submission_id): (i64, Option<Uuid>) = sqlx::query_as(
        "select count(*), (select submission_id from pack_versions where name = 'published-pack' limit 1) \
         from pack_versions where name = 'published-pack'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(versions, 1);
    assert_eq!(submission_id, Some(sub.id));
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn update_publish_overwrites_but_keeps_added(pool: PgPool) {
    init_log();
    let uploader = Uuid::new_v4();
    seed_user(&pool, uploader).await;
    let mut sub = SubmissionRow {
        id: Uuid::new_v4(),
        pack_name: "aging-pack".into(),
        version: "0.1.0".into(),
        uploader,
        archive_path: None,
        report: json!({}),
        category: Some("dev".into()),
        author: Some("a".into()),
        license: Some("MIT".into()),
        homepage: None,
    };
    let manifest = json!({ "description": { "en": "v1" }, "consumes": [], "languages": [] });
    upsert_published_pack(&pool, &sub, &manifest, "sha256:v1", false, "tarballs/a-0.1.0.tgz", "sha256:t1", "org-a")
        .await
        .expect("v1 publish");

    let (added, ):(chrono::DateTime<chrono::Utc>, ) =
        sqlx::query_as("select added from packs where name = 'aging-pack'").fetch_one(&pool).await.unwrap();

    sub.version = "0.2.0".into();
    let manifest2 = json!({ "description": { "en": "v2" }, "consumes": [], "languages": [] });
    upsert_published_pack(&pool, &sub, &manifest2, "sha256:v2", true, "tarballs/a-0.2.0.tgz", "sha256:t2", "org-a")
        .await
        .expect("v2 publish overwrites");

    let (version, description, fn_rules, added2, updated): (
        String,
        Value,
        bool,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
    ) = sqlx::query_as(
        "select version, description, has_function_rules, added, updated \
         from packs where name = 'aging-pack'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(version, "0.2.0");
    assert_eq!(description["en"], "v2");
    assert!(fn_rules);
    assert_eq!(added, added2, "added is preserved across updates");
    assert!(updated >= added);

    let (history, old_paths): (i64, i64) = sqlx::query_as(
        "select count(*), count(*) filter (where version = '0.1.0' and tarball_path = 'tarballs/a-0.1.0.tgz') \
         from pack_versions where name = 'aging-pack'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(history, 2, "each publish appends a history row");
    assert_eq!(old_paths, 1, "the old version's tarball path stays intact");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn republishing_keeps_a_disabled_pack_disabled(pool: PgPool) {
    init_log();
    let uploader = Uuid::new_v4();
    seed_user(&pool, uploader).await;
    let mut sub = SubmissionRow {
        id: Uuid::new_v4(),
        pack_name: "benched-pack".into(),
        version: "0.1.0".into(),
        uploader,
        archive_path: None,
        report: json!({}),
        category: Some("dev".into()),
        author: Some("a".into()),
        license: Some("MIT".into()),
        homepage: None,
    };
    let manifest = json!({ "description": { "en": "v1" }, "consumes": [], "languages": [] });
    upsert_published_pack(&pool, &sub, &manifest, "sha256:v1", false, "tarballs/b-0.1.0.tgz", "sha256:t1", "org-a")
        .await
        .expect("v1 publish");
    sqlx::query("update packs set status = 'disabled' where name = 'benched-pack'")
        .execute(&pool)
        .await
        .unwrap();

    sub.version = "0.2.0".into();
    let manifest2 = json!({ "description": { "en": "v2" }, "consumes": [], "languages": [] });
    upsert_published_pack(&pool, &sub, &manifest2, "sha256:v2", true, "tarballs/b-0.2.0.tgz", "sha256:t2", "org-a")
        .await
        .expect("v2 publish lands even while disabled");

    let (status, versions): (String, i64) = sqlx::query_as(
        "select p.status, (select count(*) from pack_versions v where v.name = p.name) \
         from packs p where p.name = 'benched-pack'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "disabled", "publishing must not silently re-enable");
    assert_eq!(versions, 2, "history grows even while disabled");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn review_transition_stamps_reviewer_and_time(pool: PgPool) {
    init_log();
    let uploader = Uuid::new_v4();
    let reviewer = Uuid::new_v4();
    seed_user(&pool, uploader).await;
    seed_user(&pool, reviewer).await;
    let id = Uuid::new_v4();
    seed_submission(&pool, id, "stamped-pack", "0.1.0", uploader, "pending").await;

    mark_reviewed(&pool, id, "approved", Some("fine"), reviewer)
        .await
        .expect("approve transition");

    let (status, note, reviewed_by, reviewed_at): (
        String,
        Option<String>,
        Option<Uuid>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "select status, review_note, reviewed_by, reviewed_at from submissions where id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "approved");
    assert_eq!(note.as_deref(), Some("fine"));
    assert_eq!(reviewed_by, Some(reviewer));
    assert!(reviewed_at.is_some(), "reviewed_at is stamped");
}

// ---- read-side handlers, driven directly (no HTTP) --------------------------
// The handlers take State/Arc<AppState> extractors, so tests can call them
// like plain async functions with a hand-built state over the scratch DB.
// Auth-gated writes (submit/review) go through real HTTP in scripts/e2e.sh.

use std::sync::Arc;

use axum::extract::{Path, State};

fn test_state(pool: PgPool) -> Arc<crate::state::AppState> {
    test_state_with(&pool, "http://supabase.invalid")
}

fn test_state_with(pool: &PgPool, supabase_api_url: &str) -> Arc<crate::state::AppState> {
    Arc::new(crate::state::AppState {
        cfg: crate::config::Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            supabase_api_url: supabase_api_url.into(),
            supabase_secret_key: "test".into(),
            supabase_publishable_key: "test".into(),
            supabase_public_url: "http://supabase.invalid".into(),
            database_url: String::new(),
            verifier_url: "http://127.0.0.1:1".into(),
            web_dir: String::new(),
            data_dir: String::new(),
            public_origin: "http://127.0.0.1:8398".into(),
            source_url: "https://example.invalid/repo".into(),
        },
        http: reqwest::Client::new(),
        db: pool.clone(),
        jwks: tokio::sync::Mutex::new(None),
    })
}

async fn seed_full_pack(pool: &PgPool, name: &str) -> Uuid {
    let uploader = Uuid::new_v4();
    seed_user(pool, uploader).await;
    let sub = SubmissionRow {
        id: Uuid::new_v4(),
        pack_name: name.into(),
        version: "0.1.0".into(),
        uploader,
        archive_path: None,
        report: json!({
            "checkOutput": "task-card@0.1.0 ... GREEN",
            "checkedAt": "2026-11-06T00:00:00Z",
            "manifest": {},
        }),
        category: Some("dev".into()),
        author: Some("sharework-ai".into()),
        license: Some("MIT".into()),
        homepage: None,
    };
    let manifest = json!({
        "description": { "en": "eng desc", "zh": "中文描述" },
        "consumes": ["cosmic-func"],
        "languages": ["zh-CN"],
        "requiresTopic": true,
        "artifactPrefix": "CARD",
    });
    upsert_published_pack(pool, &sub, &manifest, "sha256:h", false, &format!("tarballs/{name}-0.1.0.tgz"), "sha256:t", "sharework-ai")
        .await
        .expect("seed publish");
    // The detail page's health sheet comes from the approved submission.
    sqlx::query(
        "insert into submissions (id, pack_name, version, uploader, status, report) \
         values ($1, $2, '0.1.0', $3, 'approved', $4::jsonb)",
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .bind(uploader)
    .bind(
        json!({
            "checkOutput": "task-card@0.1.0 ... GREEN",
            "checkedAt": "2026-11-06T00:00:00Z",
        })
        .to_string(),
    )
    .execute(pool)
    .await
    .expect("seed approved submission");
    uploader
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn pack_list_and_detail_render_the_catalog(pool: PgPool) {
    init_log();
    let state = test_state(pool);
    seed_full_pack(&state.db, "shown-pack").await;

    let list = super::packs::list(State(state.clone())).await.unwrap().0;
    let packs = list["packs"].as_array().unwrap();
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0]["name"], "shown-pack");
    assert_eq!(packs[0]["tarball"], "/tarballs/shown-pack-0.1.0.tgz");

    let detail = super::packs::detail(
        State(state.clone()),
        Path("shown-pack".into()),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(detail["pack"]["name"], "shown-pack");
    assert_eq!(detail["health"], "task-card@0.1.0 ... GREEN");
    assert_eq!(detail["verifiedAt"], "2026-11-06T00:00:00Z");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn pack_detail_404s_on_unknown_names(pool: PgPool) {
    init_log();
    let state = test_state(pool);
    let error = super::packs::detail(State(state), Path("ghost".into()))
        .await
        .expect_err("unknown pack must 404");
    assert_eq!(error.status, axum::http::StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn index_is_format_two_with_org_and_version_history(pool: PgPool) {
    init_log();
    let state = test_state(pool);
    seed_full_pack(&state.db, "idx-pack").await;

    let index = super::packs::index(State(state)).await.unwrap().0;
    assert_eq!(index["format"], 2, "install.mjs keys off the format number");
    assert_eq!(index["origin"], "http://127.0.0.1:8398");
    let pack = &index["packs"][0];
    assert_eq!(pack["name"], "idx-pack");
    assert_eq!(pack["tarball"], "/tarballs/idx-pack-0.1.0.tgz");
    assert_eq!(pack["contentHash"], "sha256:h");
    assert_eq!(pack["tarballSha256"], "sha256:t");
    assert_eq!(pack["org"], "sharework-ai");
    assert_eq!(pack["page"], "/p/idx-pack");
    let versions = pack["versions"].as_array().expect("versions array");
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0]["version"], "0.1.0");
    assert_eq!(versions[0]["tarball"], "/tarballs/idx-pack-0.1.0.tgz");
    assert_eq!(versions[0]["tarballSha256"], "sha256:t");
    assert_eq!(versions[0]["contentHash"], "sha256:h");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn tarball_route_maps_disabled_to_410_and_unknown_to_404(pool: PgPool) {
    init_log();
    let state = test_state(pool);
    seed_full_pack(&state.db, "dl-pack").await;
    sqlx::query("update packs set status = 'disabled' where name = 'dl-pack'")
        .execute(&state.db)
        .await
        .unwrap();

    // Disabled packs answer 410 before any Storage traffic (this test's
        // Supabase URL is a dead end on purpose — reaching it would 500).
    let gone = super::packs::tarball(
        State(state.clone()),
        Path("dl-pack-0.1.0.tgz".into()),
    )
    .await
    .expect_err("disabled pack must not serve bytes");
    assert_eq!(gone.status, axum::http::StatusCode::GONE);

    let miss = super::packs::tarball(
        State(state),
        Path("ghost-9.9.9.tgz".into()),
    )
    .await
    .expect_err("unknown tarball must 404");
    assert_eq!(miss.status, axum::http::StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn disabled_packs_vanish_from_the_public_surface(pool: PgPool) {
    init_log();
    let state = test_state(pool);
    seed_full_pack(&state.db, "quiet-pack").await;
    sqlx::query("update packs set status = 'disabled' where name = 'quiet-pack'")
        .execute(&state.db)
        .await
        .unwrap();

    let list = super::packs::list(State(state.clone())).await.unwrap().0;
    assert!(list["packs"].as_array().unwrap().is_empty(), "list hides disabled packs");

    let index = super::packs::index(State(state.clone())).await.unwrap().0;
    assert!(index["packs"].as_array().unwrap().is_empty(), "index hides disabled packs");

    let error = super::packs::detail(State(state), Path("quiet-pack".into()))
        .await
        .expect_err("detail hides disabled packs");
    assert_eq!(error.status, axum::http::StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn detail_lists_versions_newest_first(pool: PgPool) {
    init_log();
    let state = test_state(pool);
    let uploader = seed_full_pack(&state.db, "hist-pack").await;
    let sub = SubmissionRow {
        id: Uuid::new_v4(),
        pack_name: "hist-pack".into(),
        version: "0.2.0".into(),
        uploader,
        archive_path: None,
        report: json!({ "manifest": {} }),
        category: Some("dev".into()),
        author: Some("a".into()),
        license: Some("MIT".into()),
        homepage: None,
    };
    let manifest = json!({ "description": { "en": "v2" }, "consumes": [], "languages": [] });
    upsert_published_pack(&state.db, &sub, &manifest, "sha256:h2", false, "tarballs/hist-pack-0.2.0.tgz", "sha256:t2", "sharework-ai")
        .await
        .expect("v2 publish");

    let detail = super::packs::detail(State(state), Path("hist-pack".into()))
        .await
        .unwrap()
        .0;
    let versions = detail["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0]["version"], "0.2.0", "newest publish first");
    assert_eq!(versions[1]["version"], "0.1.0");
    assert_eq!(versions[1]["tarball"], "/tarballs/hist-pack-0.1.0.tgz");
    assert_eq!(detail["pack"]["version"], "0.2.0", "top level stays latest");
}

// ---- org --------------------------------------------------------------------

#[sqlx::test(migrations = "../supabase/migrations")]
async fn org_change_relabels_only_the_owners_packs(pool: PgPool) {
    init_log();
    let owner = Uuid::new_v4();
    let other = Uuid::new_v4();
    for id in [owner, other] {
        seed_user(&pool, id).await;
    }
    seed_profile(&pool, owner, Some("old-org")).await;
    seed_profile(&pool, other, Some("other-org")).await;
    seed_pack(&pool, "mine-1", "0.1.0", owner, "sha256:a").await;
    seed_pack(&pool, "mine-2", "0.1.0", owner, "sha256:b").await;
    seed_pack(&pool, "theirs", "0.1.0", other, "sha256:c").await;
    // seed_pack leaves org null; give the other author's pack its real org so
    // the "untouched" assertion below means something.
    sqlx::query("update packs set org = 'other-org' where name = 'theirs'")
        .execute(&pool)
        .await
        .unwrap();

    let affected = super::account::apply_org_change(&pool, owner, "new-org")
        .await
        .expect("org change succeeds");
    assert_eq!(affected, 2, "only the owner's packs were relabeled");

    let mine: (Option<String>, Option<String>) = sqlx::query_as(
        "select (select org from packs where name = 'mine-1'), \
                (select org from packs where name = 'theirs')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(mine.0.as_deref(), Some("new-org"));
    assert_eq!(mine.1.as_deref(), Some("other-org"), "another author's packs untouched");

    let profile: (Option<String>,) =
        sqlx::query_as("select org from profiles where id = $1").bind(owner).fetch_one(&pool).await.unwrap();
    assert_eq!(profile.0.as_deref(), Some("new-org"));
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn org_slugs_are_unique_reserved_and_shape_checked(pool: PgPool) {
    init_log();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    seed_user(&pool, a).await;
    seed_user(&pool, b).await;
    seed_profile(&pool, a, Some("taken-org")).await;

    let taken = super::account::apply_org_change(&pool, b, "taken-org")
        .await
        .expect_err("claiming another account's org must 409");
    assert_eq!(taken.status, axum::http::StatusCode::CONFLICT);

    for bad in ["-leading", "Upper", "with space", "admin", "docwright", "", "way-too-long-this-slug-keeps-going-beyond-the-thirty-nine-char-mark"] {
        let err = super::account::apply_org_change(&pool, b, bad)
            .await
            .expect_err(format!("invalid org {bad:?} must 400").as_str());
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST, "{bad:?}");
    }

    super::account::apply_org_change(&pool, b, "b-org").await.expect("valid org passes");
    // Same account re-setting its own org is a no-op, not a conflict.
    super::account::apply_org_change(&pool, b, "b-org").await.expect("re-set is idempotent");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn tarball_route_rejects_malformed_names(pool: PgPool) {
    init_log();
    let state = test_state(pool);
    let error = super::packs::tarball(
        State(state),
        Path("../evil.tgz".into()),
    )
    .await
    .expect_err("traversal names must be rejected before the DB lookup");
    assert_eq!(error.status, axum::http::StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn auth_gated_routes_reject_missing_and_bogus_tokens(pool: PgPool) {
    init_log();
    // supabase_api_url points at a dead port: both verification paths
    // (JWKS fetch, GoTrue /user fallback) must fail -> 401, never a 5xx.
    let app = crate::build_router(test_state_with(&pool, "http://127.0.0.1:9"), "");

    for path in [
        "/api/submissions/00000000-0000-0000-0000-000000000000",
        "/api/admin/queue",
    ] {
        let response = tower::ServiceExt::oneshot(
            app.clone(),
            axum::http::Request::builder()
                .uri(path)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(
            response.status(),
            axum::http::StatusCode::UNAUTHORIZED,
            "{path} without a token must 401"
        );
    }

    let response = tower::ServiceExt::oneshot(
        app,
        axum::http::Request::builder()
            .uri("/api/admin/queue")
            .header("authorization", "Bearer not-a-real-jwt")
            .body(axum::body::Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}
