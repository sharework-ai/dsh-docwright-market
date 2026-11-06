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

use super::tests::{init_log, seed_submission, seed_user};

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
            database_url: String::new(),
            verifier_url: verifier.uri(),
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
    seed_user(&pool, author).await;
    seed_user(&pool, admin).await;
    seed_user(&pool, stranger).await;
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

    let (version, status, note): (String, String, Option<String>) = sqlx::query_as(
        "select p.version, s.status, s.review_note from packs p \
         join submissions s on s.pack_name = p.name where s.id = $1",
    )
    .bind(sub_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(version, "1.0.0");
    assert_eq!(status, "approved");
    assert_eq!(note.as_deref(), Some("looks good"));

    // The published tarball is served by the download route (catalog match).
    let redirect = tower::ServiceExt::oneshot(
        app,
        Request::builder()
            .uri("/tarballs/mocked-pack-1.0.0.tgz")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert!(redirect.status().is_redirection());
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

