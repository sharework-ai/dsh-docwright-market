//! Shared application state handed to every handler.

use crate::config::Config;
use jsonwebtoken::jwk::JwkSet;
use sqlx::PgPool;
use std::time::Instant;
use tokio::sync::Mutex;

pub struct AppState {
    pub cfg: Config,
    pub http: reqwest::Client,
    pub db: PgPool,
    /// GoTrue JWKS cache: None = never fetched; Some((jwks, fetched_at)).
    pub jwks: Mutex<Option<(JwkSet, Instant)>>,
}
