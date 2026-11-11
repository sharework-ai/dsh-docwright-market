//! Full-handler integration tests with wiremock standing in for the external
//! services (Supabase Auth + Storage on one server, the verifier sidecar on
//! another). Real router, real scratch database, mocked network — the layer
//! between unit tests and the live e2e.
//!
//! Auth note: `verify_token` tries JWKS first, then falls back to GoTrue's
//! `/auth/v1/user`. The mock 404s the JWKS endpoint, so every happy-path test
//! exercises the fallback; the token itself can be any non-empty string
//! because the mock keys the identity off the Authorization header value.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt; // oneshot
use uuid::Uuid;
use wiremock::matchers::{header, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::state::AppState;

use super::tests::{init_log, seed_profile, seed_submission, seed_suite, seed_user, seed_pack};

const TOK_AUTHOR: &str = "tok-author";
const TOK_ADMIN: &str = "tok-admin";
const TOK_STRANGER: &str = "tok-stranger";

/// Auth (JWKS-404 + /user identity) and Storage REST on one mock server;
/// the verifier sidecar on a second.
async fn start_mocks(author: Uuid, admin: Uuid, stranger: Uuid, verifier_ok: bool) -> (MockServer, MockServer) {
    let supabase = MockServer::start().await;
    let verifier = MockServer::start().await;

    // --- auth: JWKS 404 pushes verification onto the /user fallback -------
    Mock::given(method("GET"))
        .and(path("/auth/v1/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&supabase)
        .await;
    for (token, id) in [(TOK_AUTHOR, author), (TOK_ADMIN, admin), (TOK_STRANGER, stranger)] {
        Mock::given(method("GET"))
            .and(path("/auth/v1/user"))
            .and(header("authorization", format!("Bearer {token}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": id, "email": "x@example.invalid" })))
            .mount(&supabase)
            .await;
    }

    // --- storage: bucket ensure, upload, authenticated download -----------
    Mock::given(method("POST"))
        .and(path("/storage/v1/bucket"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&supabase)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/storage/v1/object/uploads/.+"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&supabase)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/storage/v1/object/tarballs/.+"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&supabase)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"/storage/v1/object/authenticated/uploads/.+"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"fake archive bytes".to_vec()))
        .mount(&supabase)
        .await;
    // The proxied public download (tarball route serves bytes, not a 302).
    Mock::given(method("GET"))
        .and(path_regex(r"/storage/v1/object/public/tarballs/.+"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"published tarball bytes".to_vec()))
        .mount(&supabase)
        .await;
    // Best-effort deletes (pack removal, submission withdrawal) — the DB is
    // the source of truth, so only these tests assert the calls happened.
    Mock::given(method("DELETE"))
        .and(path_regex(r"/storage/v1/object/(uploads|tarballs)/.+"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&supabase)
        .await;

    // --- verifier sidecar ---------------------------------------------------
    let report = if verifier_ok {
        json!({
            "ok": true,
            "report": {
                "packName": "mocked-pack", "version": "1.0.0",
                "manifest": {
                    "description": { "en": "mocked", "zh": "模拟" },
                    "consumes": [], "languages": ["zh-CN"],
                    "requiresTopic": false, "artifactPrefix": null,
                },
                "shapeOk": true, "checkGreen": true, "hasFunctionRules": false,
                "contentHash": "sha256:mock",
                "files": ["mocked-pack/manifest.json"], "problems": [],
                "checkOutput": "GREEN",
            }
        })
    } else {
        json!({
            "ok": false,
            "report": { "problems": ["no pack found"], "checkOutput": "" },
        })
    };
    Mock::given(method("POST"))
        .and(path("/verify"))
        .respond_with(ResponseTemplate::new(200).set_body_json(report))
        .mount(&verifier)
        .await;

    (supabase, verifier)
}

fn state_for(pool: &PgPool, supabase: &MockServer, verifier: &MockServer) -> Arc<AppState> {
    Arc::new(AppState {
        cfg: crate::config::Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            supabase_api_url: supabase.uri(),
            supabase_secret_key: "test-secret".into(),
            supabase_publishable_key: "test-publishable".into(),
            supabase_public_url: supabase.uri(),
            database_url: String::new(),
            verifier_url: verifier.uri(),
            web_dir: String::new(),
            public_origin: "http://127.0.0.1:8398".into(),
            source_url: "https://example.invalid/repo".into(),
        },
        http: reqwest::Client::new(),
        db: pool.clone(),
        jwks: tokio::sync::Mutex::new(None),
    })
}

/// Hand-built multipart body: form fields + one binary `archive` part.
fn multipart_body(fields: &[(&str, &str)], archive: &[u8]) -> (String, Vec<u8>) {
    let boundary = "XMARKETBOUNDARYX";
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!("--{boundary}\r\ncontent-disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes(),
        );
    }
    body.extend_from_slice(
        format!("--{boundary}\r\ncontent-disposition: form-data; name=\"archive\"; filename=\"pack.tgz\"\r\ncontent-type: application/octet-stream\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(archive);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary.to_string(), body)
}

async fn post_multipart(app: &axum::Router, token: &str, body: Vec<u8>, boundary: &str) -> axum::response::Response {
    tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/api/submissions")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", format!("multipart/form-data; boundary={boundary}"))
            .body(Body::from(body))
            .unwrap(),
    )
    .await
    .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    serde_json::from_slice(&bytes).expect("response body must be JSON")
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn submit_happy_path_verifies_uploads_and_enqueues(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    let admin = Uuid::new_v4();
    let stranger = Uuid::new_v4();
    for id in [author, admin, stranger] {
        seed_user(&pool, id).await;
    }
    seed_profile(&pool, author, Some("author-org")).await;
    let (supabase, verifier) = start_mocks(author, admin, stranger, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let (boundary, body) = multipart_body(
        &[("author", "author"), ("category", "dev"), ("license", "MIT")],
        b"fake archive bytes",
    );
    let response = post_multipart(&app, TOK_AUTHOR, body, &boundary).await;
    assert_eq!(response.status(), StatusCode::OK, "accepted submission returns 200");
    let payload = json_body(response).await;
    assert_eq!(payload["ok"], true);
    let sub_id = payload["id"].as_str().unwrap().to_string();

    // Enqueued as pending with the verifier's report attached.
    let (status, report): (String, Value) = sqlx::query_as(
        "select status, report from submissions where id = $1",
    )
    .bind(Uuid::parse_str(&sub_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "pending");
    assert_eq!(report["packName"], "mocked-pack");

    // The raw archive reached the uploads bucket exactly once.
    supabase.verify().await;
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn machine_rejected_submission_stores_nothing(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_profile(&pool, author, Some("author-org")).await;
    let (supabase, verifier) = start_mocks(author, author, author, false).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let (boundary, body) = multipart_body(
        &[("author", "author"), ("category", "dev"), ("license", "MIT")],
        b"garbage",
    );
    let response = post_multipart(&app, TOK_AUTHOR, body, &boundary).await;
    assert_eq!(response.status(), StatusCode::OK, "rejections are surfaced as 200 + ok:false");
    let payload = json_body(response).await;
    assert_eq!(payload["ok"], false);

    let (count,): (i64,) = sqlx::query_as("select count(*) from submissions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "machine-rejected submissions must not enter the queue");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn approve_publishes_from_the_mocked_chain(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    let admin = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_user(&pool, admin).await;
    seed_profile(&pool, author, Some("author-org")).await;
    sqlx::query("insert into profiles (id, is_admin) values ($1, true)")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();

    let (supabase, verifier) = start_mocks(author, admin, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    // Submit through the real handler first (uploads the archive via mock).
    let (boundary, body) = multipart_body(
        &[("author", "author"), ("category", "dev"), ("license", "MIT")],
        b"fake archive bytes",
    );
    let response = post_multipart(&app, TOK_AUTHOR, body, &boundary).await;
    let payload = json_body(response).await;
    let sub_id = Uuid::parse_str(payload["id"].as_str().unwrap()).unwrap();

    // Admin approves through the real handler: Storage download + upload via
    // mock, catalog upsert + review stamping via the scratch DB.
    let review = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri(format!("/api/admin/review/{sub_id}"))
            .header("authorization", format!("Bearer {TOK_ADMIN}"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"action":"approve","note":"looks good"}"#))
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(review.status(), StatusCode::OK);
    assert_eq!(json_body(review).await["status"], "approved");

    let (version, status, note, org): (String, String, Option<String>, Option<String>) =
        sqlx::query_as(
            "select p.version, s.status, s.review_note, p.org from packs p \
             join submissions s on s.pack_name = p.name where s.id = $1",
        )
        .bind(sub_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version, "1.0.0");
    assert_eq!(status, "approved");
    assert_eq!(note.as_deref(), Some("looks good"));
    assert_eq!(org.as_deref(), Some("author-org"), "publish stamps the uploader's org");

    let (versions, provenance): (i64, Option<Uuid>) = sqlx::query_as(
        "select count(*), (select submission_id from pack_versions where name = 'mocked-pack' limit 1) \
         from pack_versions where name = 'mocked-pack'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(versions, 1, "publish opens the version history");
    assert_eq!(provenance, Some(sub_id));

    // The published tarball is proxied from Storage's public object.
    let download = tower::ServiceExt::oneshot(
        app,
        Request::builder()
            .uri("/tarballs/mocked-pack-1.0.0.tgz")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(download.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(download.into_body(), 1 << 20).await.unwrap();
    assert_eq!(&bytes[..], b"published tarball bytes");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn review_and_owner_views_are_identity_gated(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    let stranger = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_user(&pool, stranger).await;
    let sub_id = Uuid::new_v4();
    seed_submission(&pool, sub_id, "gated-pack", "0.1.0", author, "pending").await;

    let (supabase, verifier) = start_mocks(author, author, stranger, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let get = |token: String| {
        let app = app.clone();
        async move {
            tower::ServiceExt::oneshot(
                app,
                Request::builder()
                    .uri(format!("/api/submissions/{sub_id}"))
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
        }
    };

    let owner_view = get(TOK_AUTHOR.to_string()).await;
    assert_eq!(owner_view.status(), StatusCode::OK);
    assert_eq!(json_body(owner_view).await["status"], "pending");

    let stranger_view = get(TOK_STRANGER.to_string()).await;
    assert_eq!(
        stranger_view.status(),
        StatusCode::FORBIDDEN,
        "non-owner non-admin must not see a submission"
    );
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn queue_lists_pending_for_admin_only(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    let admin = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_user(&pool, admin).await;
    sqlx::query("insert into profiles (id, is_admin) values ($1, true)")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    let sub_id = Uuid::new_v4();
    seed_submission(&pool, sub_id, "queued-pack", "2.0.0", author, "pending").await;

    let (supabase, verifier) = start_mocks(author, admin, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let admin_view = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder()
            .uri("/api/admin/queue")
            .header("authorization", format!("Bearer {TOK_ADMIN}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(admin_view.status(), StatusCode::OK);
    let payload = json_body(admin_view).await;
    assert_eq!(payload["queue"][0]["packName"], "queued-pack");

    let stranger_view = tower::ServiceExt::oneshot(
        app,
        Request::builder()
            .uri("/api/admin/queue")
            .header("authorization", format!("Bearer {TOK_STRANGER}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(stranger_view.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn rejected_submission_is_archived_with_a_note(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    let admin = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_user(&pool, admin).await;
    sqlx::query("insert into profiles (id, is_admin) values ($1, true)")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    let sub_id = Uuid::new_v4();
    seed_submission(&pool, sub_id, "nope-pack", "0.1.0", author, "pending").await;

    let (supabase, verifier) = start_mocks(author, admin, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let review = tower::ServiceExt::oneshot(
        app,
        Request::builder()
            .method("POST")
            .uri(format!("/api/admin/review/{sub_id}"))
            .header("authorization", format!("Bearer {TOK_ADMIN}"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"action":"reject","note":"suspicious functions.js"}"#))
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(review.status(), StatusCode::OK);

    let (status, note): (String, Option<String>) = sqlx::query_as(
        "select status, review_note from submissions where id = $1",
    )
    .bind(sub_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "rejected");
    assert_eq!(note.as_deref(), Some("suspicious functions.js"));
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn submit_rejects_bad_form_fields_before_the_verifier(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_profile(&pool, author, Some("author-org")).await;
    let (supabase, verifier) = start_mocks(author, author, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    for category in ["nope", ""] {
        let (boundary, body) = multipart_body(
            &[("author", "a"), ("category", category), ("license", "MIT")],
            b"bytes",
        );
        let response = post_multipart(&app, TOK_AUTHOR, body, &boundary).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "category {category:?}");
    }
    let (boundary, body) = multipart_body(
        &[("author", "a"), ("category", "dev"), ("license", "Proprietary-9")],
        b"bytes",
    );
    let response = post_multipart(&app, TOK_AUTHOR, body, &boundary).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST, "unknown SPDX license");
}

// ---- org gate + owner management (real router, mocked network) --------------

async fn json_request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: &str,
    body: &str,
) -> axum::response::Response {
    tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn submit_without_an_org_is_refused_before_any_side_effects(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    seed_user(&pool, author).await;
    // No profile row at all — same treatment as a null org.
    let (supabase, verifier) = start_mocks(author, author, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let (boundary, body) = multipart_body(
        &[("author", "a"), ("category", "dev"), ("license", "MIT")],
        b"bytes",
    );
    let response = post_multipart(&app, TOK_AUTHOR, body, &boundary).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let payload = json_body(response).await;
    assert!(payload["error"].as_str().unwrap().contains("org"));

    let (count,): (i64,) = sqlx::query_as("select count(*) from submissions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "the org gate fires before anything is stored");

    // Setting an org through the real handler flips the gate.
    let set = json_request(&app, "PUT", "/api/account/org", TOK_AUTHOR, r#"{"org":"fresh-org"}"#).await;
    assert_eq!(set.status(), StatusCode::OK);
    assert_eq!(json_body(set).await["packsAffected"], 0);

    let (boundary, body) = multipart_body(
        &[("author", "a"), ("category", "dev"), ("license", "MIT")],
        b"bytes",
    );
    let response = post_multipart(&app, TOK_AUTHOR, body, &boundary).await;
    assert_eq!(response.status(), StatusCode::OK, "with an org, the same submit passes");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn my_routes_are_identity_gated(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    let stranger = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_user(&pool, stranger).await;
    seed_pack(&pool, "owned-pack", "0.1.0", author, "sha256:a").await;
    let sub_id = Uuid::new_v4();
    seed_submission(&pool, sub_id, "owned-pack", "0.2.0", author, "pending").await;

    let (supabase, verifier) = start_mocks(author, author, stranger, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    // Pack listing is scoped to the caller.
    let mine = json_request(&app, "GET", "/api/my/packs", TOK_AUTHOR, "").await;
    assert_eq!(mine.status(), StatusCode::OK);
    let payload = json_body(mine).await;
    assert_eq!(payload["packs"].as_array().unwrap().len(), 1);
    assert_eq!(payload["packs"][0]["name"], "owned-pack");

    let theirs = json_request(&app, "GET", "/api/my/packs", TOK_STRANGER, "").await;
    assert_eq!(theirs.status(), StatusCode::OK);
    assert!(json_body(theirs).await["packs"].as_array().unwrap().is_empty());

    // Mutations on someone else's pack are refused, not leaked.
    for (method, uri, body) in [
        ("PATCH", "/api/my/packs/owned-pack", r#"{"category":"dev"}"#),
        ("POST", "/api/my/packs/owned-pack/disable", ""),
        ("POST", "/api/my/packs/owned-pack/enable", ""),
        ("DELETE", "/api/my/packs/owned-pack", ""),
    ] {
        let response = json_request(&app, method, uri, TOK_STRANGER, body).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method} {uri}");
    }
    let response = json_request(&app, "DELETE", &format!("/api/my/submissions/{sub_id}"), TOK_STRANGER, "").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "stranger cannot withdraw");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn disable_hides_the_pack_and_blocks_downloads_then_enable_restores(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_pack(&pool, "toggle-pack", "0.1.0", author, "sha256:a").await;
    // Downloads resolve through the version history, so give the seed one.
    sqlx::query(
        "insert into pack_versions (name, version, description, content_hash, tarball_path, tarball_sha256) \
         values ('toggle-pack', '0.1.0', '{}', 'sha256:a', 'tarballs/toggle-pack-0.1.0.tgz', 'sha256:t')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (supabase, verifier) = start_mocks(author, author, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let disable = json_request(&app, "POST", "/api/my/packs/toggle-pack/disable", TOK_AUTHOR, "").await;
    assert_eq!(disable.status(), StatusCode::OK);
    assert_eq!(json_body(disable).await["status"], "disabled");

    let gone = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder()
            .uri("/tarballs/toggle-pack-0.1.0.tgz")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(gone.status(), StatusCode::GONE, "disabled packs serve 410, not 404");

    let index = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder().uri("/api/index").body(Body::empty()).unwrap(),
    )
    .await
    .unwrap();
    assert!(json_body(index).await["packs"].as_array().unwrap().is_empty());

    let detail = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder().uri("/api/packs/toggle-pack").body(Body::empty()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(detail.status(), StatusCode::NOT_FOUND);

    let enable = json_request(&app, "POST", "/api/my/packs/toggle-pack/enable", TOK_AUTHOR, "").await;
    assert_eq!(enable.status(), StatusCode::OK);

    let restored = tower::ServiceExt::oneshot(
        app,
        Request::builder()
            .uri("/tarballs/toggle-pack-0.1.0.tgz")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(restored.status(), StatusCode::OK, "re-enabled packs download again");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn patch_updates_owner_metadata_with_whitelist_checks(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_pack(&pool, "meta-pack", "0.1.0", author, "sha256:a").await;

    let (supabase, verifier) = start_mocks(author, author, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let bad = json_request(&app, "PATCH", "/api/my/packs/meta-pack", TOK_AUTHOR, r#"{"category":"nope"}"#).await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    let bad = json_request(&app, "PATCH", "/api/my/packs/meta-pack", TOK_AUTHOR, r#"{"license":"Proprietary-9"}"#).await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    let bad = json_request(&app, "PATCH", "/api/my/packs/meta-pack", TOK_AUTHOR, r#"{"homepage":"ftp://x"}"#).await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    let empty = json_request(&app, "PATCH", "/api/my/packs/meta-pack", TOK_AUTHOR, "{}").await;
    assert_eq!(empty.status(), StatusCode::BAD_REQUEST, "empty patch is rejected");

    let set_home = json_request(
        &app,
        "PATCH",
        "/api/my/packs/meta-pack",
        TOK_AUTHOR,
        r#"{"homepage":"https://example.invalid/pack"}"#,
    )
    .await;
    assert_eq!(set_home.status(), StatusCode::OK);
    assert_eq!(json_body(set_home).await["pack"]["homepage"], "https://example.invalid/pack");

    let ok = json_request(
        &app,
        "PATCH",
        "/api/my/packs/meta-pack",
        TOK_AUTHOR,
        r#"{"category":"docs","license":"ISC","author":"New Name","homepage":null}"#,
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);
    let pack = json_body(ok).await["pack"].clone();
    assert_eq!(pack["category"], "docs");
    assert_eq!(pack["license"], "ISC");
    assert_eq!(pack["author"], "New Name");
    assert!(pack["homepage"].is_null(), "explicit null clears the homepage");

    let (fn_rules,): (bool,) = sqlx::query_as(
        "select has_function_rules from packs where name = 'meta-pack'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!fn_rules, "patch must not touch manifest-derived columns");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn delete_pack_removes_rows_and_storage_objects_keeps_submissions(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_pack(&pool, "doomed-pack", "0.1.0", author, "sha256:a").await;
    let sub_id = Uuid::new_v4();
    seed_submission(&pool, sub_id, "doomed-pack", "0.1.0", author, "approved").await;
    // History rows must go with the pack (the migration cascades them).
    sqlx::query(
        "insert into pack_versions (name, version, description, content_hash, tarball_path, tarball_sha256) \
         values ('doomed-pack', '0.1.0', '{}', 'sha256:a', 'tarballs/doomed-pack-0.1.0.tgz', 'sha256:t')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let (supabase, verifier) = start_mocks(author, author, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let response = json_request(&app, "DELETE", "/api/my/packs/doomed-pack", TOK_AUTHOR, "").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["deletedVersions"], 1);

    let (packs, versions, subs): (i64, i64, i64) = sqlx::query_as(
        "select (select count(*) from packs where name = 'doomed-pack'), \
                (select count(*) from pack_versions where name = 'doomed-pack'), \
                (select count(*) from submissions where id = $1)",
    )
    .bind(sub_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((packs, versions), (0, 0), "catalog and history are gone");
    assert_eq!(subs, 1, "submissions history survives so the version stays reserved");

    // The download is a plain 404 now.
    let miss = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder()
            .uri("/tarballs/doomed-pack-0.1.0.tgz")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(miss.status(), StatusCode::NOT_FOUND);

    // The Storage object delete was attempted for the history path.
    supabase.verify().await;

    // Re-registering the freed name must still refuse the old version.
    let rules = super::submissions::check_submission_rules(&pool, &crate::auth::AuthUser { id: author, email: None }, "doomed-pack", "0.1.0", "sha256:fresh")
        .await
        .expect_err("a deleted pack's (name, version) stays reserved forever");
    assert!(rules.message.contains("bump the version"));
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn withdraw_only_removes_own_pending_submissions(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    seed_user(&pool, author).await;
    let pending = Uuid::new_v4();
    seed_submission(&pool, pending, "wip-pack", "0.1.0", author, "pending").await;
    sqlx::query("update submissions set archive_path = 'pending.tgz' where id = $1")
        .bind(pending)
        .execute(&pool)
        .await
        .unwrap();
    let approved = Uuid::new_v4();
    seed_submission(&pool, approved, "done-pack", "0.1.0", author, "approved").await;

    let (supabase, verifier) = start_mocks(author, author, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let not_pending = json_request(&app, "DELETE", &format!("/api/my/submissions/{approved}"), TOK_AUTHOR, "").await;
    assert_eq!(not_pending.status(), StatusCode::CONFLICT);

    let ok = json_request(&app, "DELETE", &format!("/api/my/submissions/{pending}"), TOK_AUTHOR, "").await;
    assert_eq!(ok.status(), StatusCode::OK);

    let (left,): (i64,) = sqlx::query_as("select count(*) from submissions where id = $1")
        .bind(pending)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(left, 0);

    supabase.verify().await; // the uploads archive delete was attempted
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn my_submissions_lists_the_callers_history(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    let stranger = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_user(&pool, stranger).await;
    seed_submission(&pool, Uuid::new_v4(), "a-pack", "0.1.0", author, "pending").await;
    seed_submission(&pool, Uuid::new_v4(), "b-pack", "0.1.0", author, "rejected").await;
    seed_submission(&pool, Uuid::new_v4(), "c-pack", "0.1.0", stranger, "pending").await;

    let (supabase, verifier) = start_mocks(author, author, stranger, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let response = json_request(&app, "GET", "/api/my/submissions", TOK_AUTHOR, "").await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = json_body(response).await;
    let subs = payload["submissions"].as_array().unwrap();
    assert_eq!(subs.len(), 2, "only the caller's submissions");
    assert!(subs.iter().all(|s| s["packName"] != "c-pack"));
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn submission_archive_is_owner_or_admin_only(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    let stranger = Uuid::new_v4();
    let admin = Uuid::new_v4();
    for id in [author, stranger, admin] {
        seed_user(&pool, id).await;
    }
    sqlx::query("insert into profiles (id, is_admin) values ($1, true)")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    let sub_id = Uuid::new_v4();
    seed_submission(&pool, sub_id, "arch-pack", "0.3.0", author, "approved").await;
    sqlx::query("update submissions set archive_path = 'arch.tgz' where id = $1")
        .bind(sub_id)
        .execute(&pool)
        .await
        .unwrap();

    let (supabase, verifier) = start_mocks(author, admin, stranger, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    // The owner gets the raw archive back as an attachment.
    let hit = json_request(&app, "GET", &format!("/api/my/submissions/{sub_id}/archive"), TOK_AUTHOR, "").await;
    assert_eq!(hit.status(), StatusCode::OK);
    assert_eq!(
        hit.headers()["content-disposition"],
        "attachment; filename=\"arch-pack-0.3.0.tgz\""
    );
    let bytes = axum::body::to_bytes(hit.into_body(), 1 << 20).await.unwrap();
    assert_eq!(&bytes[..], b"fake archive bytes");

    // Strangers learn nothing; admins may inspect.
    let miss = json_request(&app, "GET", &format!("/api/my/submissions/{sub_id}/archive"), TOK_STRANGER, "").await;
    assert_eq!(miss.status(), StatusCode::FORBIDDEN);
    let admin_hit = tower::ServiceExt::oneshot(
        app,
        Request::builder()
            .uri(format!("/api/my/submissions/{sub_id}/archive"))
            .header("authorization", format!("Bearer {TOK_ADMIN}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(admin_hit.status(), StatusCode::OK);
}


// ---------------------------------------------------------------------------
// Suites over the real router: form submission → mixed queue → approve →
// public reads → disable filtering.

fn suite_form(name: &str, version: &str, members: &[&str]) -> String {
    json!({
        "name": name,
        "version": version,
        "description": { "en": "the cosmic suite", "zh": "宇宙套件" },
        "category": "estimation",
        "members": members,
    })
    .to_string()
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn suite_submission_queue_approve_and_public_reads(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    let admin = Uuid::new_v4();
    for id in [author, admin] {
        seed_user(&pool, id).await;
    }
    seed_profile(&pool, author, Some("author-org")).await;
    sqlx::query("insert into profiles (id, is_admin) values ($1, true)")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    seed_pack(&pool, "member-a", "0.1.0", author, "sha256:a").await;
    seed_pack(&pool, "member-b", "0.2.0", author, "sha256:b").await;

    let (supabase, verifier) = start_mocks(author, admin, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    // A member outside the packs table is refused at the door, no queue entry.
    let bad = json_request(
        &app,
        "POST",
        "/api/my/suites",
        TOK_AUTHOR,
        &suite_form("cosmic", "1.0.0", &["member-a", "ghost"]),
    )
    .await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    assert!(json_body(bad).await["error"].as_str().unwrap().contains("ghost"));

    // The valid form enters the queue (no archive, form in suite_meta).
    let ok = json_request(
        &app,
        "POST",
        "/api/my/suites",
        TOK_AUTHOR,
        &suite_form("cosmic", "1.0.0", &["member-a", "member-b"]),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK, "missing members fail, existing ones pass");
    let sub_id = json_body(ok).await["id"].as_str().unwrap().to_string();

    // The queue mixes pack and suite submissions, each labeled with its kind.
    let queue = json_request(&app, "GET", "/api/admin/queue", TOK_ADMIN, "").await;
    assert_eq!(queue.status(), StatusCode::OK);
    let queue_body = json_body(queue).await;
    let item = queue_body["queue"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "suite")
        .expect("suite submission shows in the queue with kind=suite")
        .clone();
    assert_eq!(item["suiteMeta"]["name"], "cosmic");
    assert_eq!(item["packName"], "cosmic");

    // Approving writes the catalog row; the pack card gains its suite link.
    let approve = json_request(
        &app,
        "POST",
        &format!("/api/admin/review/{sub_id}"),
        TOK_ADMIN,
        r#"{ "action": "approve" }"#,
    )
    .await;
    assert_eq!(approve.status(), StatusCode::OK);

    let packs = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder().uri("/api/packs").body(Body::empty()).unwrap(),
    )
    .await
    .unwrap();
    let packs_body = json_body(packs).await;
    assert_eq!(packs_body["packs"][0]["suites"], json!(["cosmic"]));

    // Detail: members expand into pack cards; one member vanishes from the
    // catalog after approval and must read back flagged `missing`.
    sqlx::query("delete from pack_versions where name = 'member-b'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from packs where name = 'member-b'")
        .execute(&pool)
        .await
        .unwrap();
    let detail = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder().uri("/api/suites/cosmic").body(Body::empty()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_body = json_body(detail).await;
    let cards = detail_body["members"].as_array().unwrap();
    let found = cards.iter().find(|c| c["name"] == "member-a").unwrap();
    assert_eq!(found["missing"], false);
    assert_eq!(found["version"], "0.1.0");
    let gone = cards.iter().find(|c| c["name"] == "member-b").unwrap();
    assert_eq!(gone["missing"], true);

    // Index: format stays 2; the optional suites array rides along.
    let index = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder().uri("/api/index").body(Body::empty()).unwrap(),
    )
    .await
    .unwrap();
    let index_body = json_body(index).await;
    assert_eq!(index_body["format"], 2);
    assert_eq!(index_body["suites"][0]["name"], "cosmic");
    assert_eq!(index_body["packs"][0]["suites"], json!(["cosmic"]));

    // Disabling hides the suite from every public read surface.
    let disable = json_request(&app, "POST", "/api/my/suites/cosmic/disable", TOK_AUTHOR, "").await;
    assert_eq!(disable.status(), StatusCode::OK);

    let suites = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder().uri("/api/suites").body(Body::empty()).unwrap(),
    )
    .await
    .unwrap();
    assert!(json_body(suites).await["suites"].as_array().unwrap().is_empty());

    let detail = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder().uri("/api/suites/cosmic").body(Body::empty()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(detail.status(), StatusCode::NOT_FOUND);

    let index = tower::ServiceExt::oneshot(
        app.clone(),
        Request::builder().uri("/api/index").body(Body::empty()).unwrap(),
    )
    .await
    .unwrap();
    let index_body = json_body(index).await;
    assert!(index_body["suites"].as_array().unwrap().is_empty());
    assert_eq!(index_body["packs"][0]["suites"], json!([]));

    // Enable restores everything.
    let enable = json_request(&app, "POST", "/api/my/suites/cosmic/enable", TOK_AUTHOR, "").await;
    assert_eq!(enable.status(), StatusCode::OK);
    let suites = tower::ServiceExt::oneshot(
        app,
        Request::builder().uri("/api/suites").body(Body::empty()).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(json_body(suites).await["suites"][0]["name"], "cosmic");
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn suite_edits_and_deletions_queue_instead_of_touching_the_catalog(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    let admin = Uuid::new_v4();
    let stranger = Uuid::new_v4();
    for id in [author, admin, stranger] {
        seed_user(&pool, id).await;
    }
    seed_profile(&pool, author, Some("author-org")).await;
    sqlx::query("insert into profiles (id, is_admin) values ($1, true)")
        .bind(admin)
        .execute(&pool)
        .await
        .unwrap();
    seed_pack(&pool, "member-a", "0.1.0", author, "sha256:a").await;
    seed_suite(&pool, "cosmic", &["member-a"], true, author).await;

    let (supabase, verifier) = start_mocks(author, admin, stranger, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    // Another account can neither edit nor delete someone else's suite.
    let stranger_edit = json_request(
        &app,
        "PATCH",
        "/api/my/suites/cosmic",
        TOK_STRANGER,
        &suite_form("cosmic", "1.1.0", &["member-a"]),
    )
    .await;
    assert_eq!(stranger_edit.status(), StatusCode::FORBIDDEN);

    // A same-version edit is refused (versions must strictly increase).
    let same = json_request(
        &app,
        "PATCH",
        "/api/my/suites/cosmic",
        TOK_AUTHOR,
        &suite_form("cosmic", "1.0.0", &["member-a"]),
    )
    .await;
    assert_eq!(same.status(), StatusCode::CONFLICT);

    // A proper edit enqueues; the live catalog stays untouched.
    let edit = json_request(
        &app,
        "PATCH",
        "/api/my/suites/cosmic",
        TOK_AUTHOR,
        &suite_form("cosmic", "1.1.0", &["member-a"]),
    )
    .await;
    assert_eq!(edit.status(), StatusCode::OK);
    let (version,): (String,) = sqlx::query_as("select version from suites where name = 'cosmic'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version, "1.0.0", "queued edits do not touch the live suite");

    // One pending submission per suite: approve the edit before proceeding.
    let queue = json_request(&app, "GET", "/api/admin/queue", TOK_ADMIN, "").await;
    let edit_id = json_body(queue).await["queue"][0]["id"].as_str().unwrap().to_string();
    let approve_edit = json_request(
        &app,
        "POST",
        &format!("/api/admin/review/{edit_id}"),
        TOK_ADMIN,
        r#"{ "action": "approve" }"#,
    )
    .await;
    assert_eq!(approve_edit.status(), StatusCode::OK);
    let (version,): (String,) = sqlx::query_as("select version from suites where name = 'cosmic'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version, "1.1.0", "the approved edit overwrites the live suite");

    // Deleting the published suite also queues — approval is the trigger.
    let delete = json_request(&app, "DELETE", "/api/my/suites/cosmic", TOK_AUTHOR, "").await;
    assert_eq!(delete.status(), StatusCode::OK);
    assert_eq!(json_body(delete).await["mode"], "queued");
    let (count,): (i64,) = sqlx::query_as("select count(*) from suites where name = 'cosmic'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "the suite survives until an admin approves the deletion");

    // Admin approves the deletion → the row goes away.
    let queue = json_request(&app, "GET", "/api/admin/queue", TOK_ADMIN, "").await;
    let queue_body = json_body(queue).await;
    let items = queue_body["queue"].as_array().unwrap();
    assert_eq!(items.len(), 1, "only the deletion submission is still pending");
    let delete_id = items
        .iter()
        .find(|i| i["suiteMeta"]["action"] == "delete")
        .expect("deletion submission is flagged")
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    let approve = json_request(
        &app,
        "POST",
        &format!("/api/admin/review/{delete_id}"),
        TOK_ADMIN,
        r#"{ "action": "approve", "note": "cleaned up" }"#,
    )
    .await;
    assert_eq!(approve.status(), StatusCode::OK);
    let (count,): (i64,) = sqlx::query_as("select count(*) from suites where name = 'cosmic'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../supabase/migrations")]
async fn never_published_suite_submissions_withdraw_directly(pool: PgPool) {
    init_log();
    let author = Uuid::new_v4();
    seed_user(&pool, author).await;
    seed_profile(&pool, author, Some("author-org")).await;
    seed_pack(&pool, "member-a", "0.1.0", author, "sha256:a").await;

    let (supabase, verifier) = start_mocks(author, author, author, true).await;
    let app = crate::build_router(state_for(&pool, &supabase, &verifier), "");

    let ok = json_request(
        &app,
        "POST",
        "/api/my/suites",
        TOK_AUTHOR,
        &suite_form("draft-suite", "1.0.0", &["member-a"]),
    )
    .await;
    assert_eq!(ok.status(), StatusCode::OK);

    // DELETE with no suites row = withdraw the pending submission directly.
    let delete = json_request(&app, "DELETE", "/api/my/suites/draft-suite", TOK_AUTHOR, "").await;
    assert_eq!(delete.status(), StatusCode::OK);
    assert_eq!(json_body(delete).await["mode"], "withdrawn");
    let (count,): (i64,) = sqlx::query_as("select count(*) from submissions where kind = 'suite'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
