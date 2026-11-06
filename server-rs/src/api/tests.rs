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
fn init_log() {
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::ERROR)
            .try_init();
    });
}

fn user(id: Uuid) -> AuthUser {
    AuthUser { id, email: Some("author@example.invalid".into()) }
}

async fn seed_user(pool: &PgPool, id: Uuid) {
    sqlx::query("insert into auth.users (id, email) values ($1, $2)")
        .bind(id)
        .bind("author@example.invalid")
        .execute(pool)
        .await
        .expect("seed auth user");
}

async fn seed_pack(pool: &PgPool, name: &str, version: &str, author_id: Uuid, hash: &str) {
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

async fn seed_submission(
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
async fn first_publish_lands_the_catalog_row(pool: PgPool) {
    init_log();
    let uploader = Uuid::new_v4();
    seed_user(&pool, uploader).await;
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
    upsert_published_pack(&pool, &sub, &manifest, "sha256:h1", true, "tarballs/p-0.1.0.tgz", "sha256:t1")
        .await
        .expect("first publish writes the row");

    let row: (String, Value, bool, bool, String, String) = sqlx::query_as(
        "select version, description, has_function_rules, requires_topic, category, author \
         from packs where name = 'published-pack'",
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
    upsert_published_pack(&pool, &sub, &manifest, "sha256:v1", false, "tarballs/a-0.1.0.tgz", "sha256:t1")
        .await
        .expect("v1 publish");

    let (added, ):(chrono::DateTime<chrono::Utc>, ) =
        sqlx::query_as("select added from packs where name = 'aging-pack'").fetch_one(&pool).await.unwrap();

    sub.version = "0.2.0".into();
    let manifest2 = json!({ "description": { "en": "v2" }, "consumes": [], "languages": [] });
    upsert_published_pack(&pool, &sub, &manifest2, "sha256:v2", true, "tarballs/a-0.2.0.tgz", "sha256:t2")
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
    test_state_with(pool, "http://supabase.invalid")
}

fn test_state_with(pool: PgPool, supabase_api_url: &str) -> Arc<crate::state::AppState> {
    Arc::new(crate::state::AppState {
        cfg: crate::config::Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            supabase_api_url: supabase_api_url.into(),
            supabase_secret_key: "test".into(),
            supabase_publishable_key: "test".into(),
            database_url: String::new(),
            verifier_url: "http://127.0.0.1:1".into(),
            web_dir: String::new(),
            data_dir: String::new(),
            public_origin: "http://127.0.0.1:8398".into(),
            source_url: "https://example.invalid/repo".into(),
        },
        http: reqwest::Client::new(),
        db: pool,
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
    upsert_published_pack(pool, &sub, &manifest, "sha256:h", false, &format!("tarballs/{name}-0.1.0.tgz"), "sha256:t")
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
async fn index_is_format_one_with_relative_tarball_paths(pool: PgPool) {
    init_log();
    let state = test_state(pool);
    seed_full_pack(&state.db, "idx-pack").await;

    let index = super::packs::index(State(state)).await.unwrap().0;
    assert_eq!(index["format"], 1);
    assert_eq!(index["origin"], "http://127.0.0.1:8398");
    let pack = &index["packs"][0];
    assert_eq!(pack["name"], "idx-pack");
    assert_eq!(pack["tarball"], "/tarballs/idx-pack-0.1.0.tgz");
    assert_eq!(pack["contentHash"], "sha256:h");
    assert_eq!(pack["tarballSha256"], "sha256:t");
    assert_eq!(pack["page"], "/p/idx-pack");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn tarball_route_redirects_known_paths_and_404s_unknown(pool: PgPool) {
    init_log();
    let state = test_state(pool);
    seed_full_pack(&state.db, "dl-pack").await;

    let hit = super::packs::tarball(
        State(state.clone()),
        Path("dl-pack-0.1.0.tgz".into()),
    )
    .await
    .unwrap();
    assert!(hit.status().is_redirection(), "known tarball must redirect");
    let location = hit
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();
    assert!(location.ends_with("/public/tarballs/dl-pack-0.1.0.tgz"), "{location}");

    let miss = super::packs::tarball(
        State(state),
        Path("ghost-9.9.9.tgz".into()),
    )
    .await
    .expect_err("unknown tarball must 404");
    assert_eq!(miss.status, axum::http::StatusCode::NOT_FOUND);
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
    let app = crate::build_router(test_state_with(pool, "http://127.0.0.1:9"), "");

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
