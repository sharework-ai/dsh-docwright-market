//! docwright pack market — Rust main service (product flow only; pack
//! semantics live in the verifier sidecar, docwright's own loader is the sole
//! validation surface).

mod api;
mod auth;
mod config;
mod error;
mod semver;
mod state;
mod storage;

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde_json::json;
use tower_http::services::ServeDir;

use crate::config::Config;
use crate::state::AppState;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Misconfiguration fails loud: every required env var panics here if absent.
    let cfg = Config::from_env();
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect(&cfg.database_url)
        .await
        .unwrap_or_else(|e| panic!("DATABASE_URL unusable: {e}"));
    std::fs::create_dir_all(&cfg.data_dir).expect("DATA_DIR must be creatable");

    let state = Arc::new(AppState {
        cfg: cfg.clone(),
        http: reqwest::Client::new(),
        db,
        jwks: tokio::sync::Mutex::new(None),
    });

    // Fresh deployments need the market buckets; idempotent, fails loud.
    storage::ensure_buckets(&state)
        .await
        .unwrap_or_else(|e| panic!("storage buckets unusable: {}", e.message));

    let app = build_router(state, &cfg.web_dir);

    let listener = tokio::net::TcpListener::bind(cfg.listen)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {}: {e}", cfg.listen));
    tracing::info!("market service listening on {}", cfg.listen);
    axum::serve(listener, app).await.expect("server error");
}

/// Bootstrap for the browser pages: Supabase endpoint + publishable key so the
/// static pages never hardcode deployment values.
async fn frontend_config(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(json!({
        "supabaseUrl": state.cfg.supabase_public_url,
        "supabasePublishableKey": state.cfg.supabase_publishable_key,
    }))
}

/// The whole HTTP surface, split out of `main` so the integration tests can
/// drive the real routes against a scratch database.
pub(crate) fn build_router(state: Arc<AppState>, web_dir: &str) -> Router {
    Router::new()
        .route("/config.json", get(frontend_config))
        .route("/api/account", get(api::account::show))
        .route("/api/account/org", put(api::account::set_org))
        .route("/api/my/packs", get(api::owner::my_packs))
        .route("/api/my/packs/{name}", get(api::owner::my_packs_one).patch(api::owner::patch_pack).delete(api::owner::delete_pack))
        .route("/api/my/packs/{name}/disable", post(api::owner::disable_pack))
        .route("/api/my/packs/{name}/enable", post(api::owner::enable_pack))
        .route("/api/my/submissions", get(api::owner::my_submissions))
        .route("/api/my/submissions/{id}", delete(api::owner::withdraw_submission))
        .route("/api/submissions", post(api::submissions::submit))
        .route("/api/submissions/{id}", get(api::submissions::get_submission))
        .route("/api/packs", get(api::packs::list))
        .route("/api/packs/{name}", get(api::packs::detail))
        .route("/api/index", get(api::packs::index))
        .route("/tarballs/{file}", get(api::packs::tarball))
        .route("/api/admin/queue", get(api::admin::queue))
        .route("/api/admin/review/{id}", post(api::admin::review))
        .fallback_service(
            ServeDir::new(web_dir).append_index_html_on_directories(true),
        )
        .with_state(state)
}
