//! Auth: verify a Supabase (GoTrue) access token and resolve the caller.
//!
//! Two paths, in order:
//!   1. JWKS verification (asymmetric keys — current self-hosted releases).
//!   2. Fallback: GoTrue `/auth/v1/user` with the bearer token (works for any
//!      GoTrue build regardless of signing key type).
//! On success we look up `profiles.is_admin` for admin-gated routes.

use axum::http::HeaderMap;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub email: Option<String>,
}

const JWKS_TTL: Duration = Duration::from_secs(10 * 60);

fn bearer_token(headers: &HeaderMap) -> Result<String, AppError> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::unauthorized("missing Authorization header"))?;
    let token = value
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::unauthorized("Authorization must be `Bearer <jwt>`"))?;
    Ok(token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_extraction() {
        let mut headers = HeaderMap::new();
        assert!(bearer_token(&headers).is_err(), "missing header must fail");

        headers.insert(axum::http::header::AUTHORIZATION, "Basic dXNlcjpwYXNz".parse().unwrap());
        assert!(bearer_token(&headers).is_err(), "non-Bearer scheme must fail");

        headers.insert(axum::http::header::AUTHORIZATION, "Bearer abc.def.ghi".parse().unwrap());
        assert_eq!(bearer_token(&headers).unwrap(), "abc.def.ghi");
    }
}

pub async fn require_user(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, AppError> {
    let token = bearer_token(headers)?;
    verify_token(state, &token)
        .await
        .map_err(|e| AppError::unauthorized(format!("invalid token: {e}")))
}

pub async fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<AuthUser, AppError> {
    let user = require_user(state, headers).await?;
    let is_admin: Option<(bool,)> =
        sqlx::query_as("select is_admin from profiles where id = $1")
            .bind(user.id)
            .fetch_optional(&state.db)
            .await?;
    match is_admin {
        Some((true,)) => Ok(user),
        _ => Err(AppError::forbidden("admin role required")),
    }
}

async fn verify_token(state: &AppState, token: &str) -> Result<AuthUser, String> {
    if let Some(user) = verify_via_jwks(state, token).await {
        return Ok(user);
    }
    verify_via_user_endpoint(state, token).await
}

/// Path 1: asymmetric-key verification against GoTrue's JWKS endpoint.
async fn verify_via_jwks(state: &AppState, token: &str) -> Option<AuthUser> {
    let header = decode_header(token).ok()?;
    if header.alg == Algorithm::HS256 {
        // Symmetric tokens never appear in a JWKS; go straight to the fallback.
        return None;
    }
    let jwks = jwks_cached(state).await?;
    let kid = header.kid.clone()?;
    let jwk = jwks.find(&kid)?.clone();
    let key = DecodingKey::from_jwk(&jwk).ok()?;
    let mut validation = Validation::new(header.alg);
    validation.validate_exp = true;
    validation.required_spec_claims.clear(); // exp is validated; aud varies by host
    let data = decode::<GoTrueClaims>(token, &key, &validation).ok()?;
    Some(AuthUser { id: Uuid::parse_str(&data.claims.sub).ok()?, email: data.claims.email })
}

async fn jwks_cached(state: &AppState) -> Option<JwkSet> {
    let mut cache = state.jwks.lock().await;
    if let Some((jwks, at)) = cache.as_ref() {
        if at.elapsed() < JWKS_TTL {
            return Some(jwks.clone());
        }
    }
    let url = format!("{}/auth/v1/.well-known/jwks.json", state.cfg.supabase_api_url);
    let jwks: JwkSet = state.http.get(url).send().await.ok()?.error_for_status().ok()?.json().await.ok()?;
    *cache = Some((jwks.clone(), Instant::now()));
    Some(jwks)
}

/// Path 2: ask GoTrue directly. Works for HS256-only self-hosted builds.
async fn verify_via_user_endpoint(state: &AppState, token: &str) -> Result<AuthUser, String> {
    #[derive(Deserialize)]
    struct GoTrueUser {
        id: String,
        email: Option<String>,
    }
    let url = format!("{}/auth/v1/user", state.cfg.supabase_api_url);
    let user: GoTrueUser = state
        .http
        .get(url)
        .header("authorization", format!("Bearer {token}"))
        .header("apikey", &state.cfg.supabase_secret_key)
        .send()
        .await
        .map_err(|e| format!("GoTrue unreachable: {e}"))?
        .error_for_status()
        .map_err(|e| format!("GoTrue rejected the token: {e}"))?
        .json()
        .await
        .map_err(|e| format!("GoTrue response unreadable: {e}"))?;
    let id = Uuid::parse_str(&user.id).map_err(|_| "GoTrue returned a malformed user id")?;
    Ok(AuthUser { id, email: user.email })
}

#[derive(Deserialize)]
struct GoTrueClaims {
    sub: String,
    email: Option<String>,
}
