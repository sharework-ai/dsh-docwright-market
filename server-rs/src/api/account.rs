//! Account surface: who the caller is, and their publisher org. The org is a
//! per-account publisher identity — set it before submitting; changing it
//! relabels every pack the account owns in one transaction.

use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::auth::require_user;
use crate::config::valid_org_slug;
use crate::error::AppError;
use crate::state::AppState;

/// GET /api/account — profile view for the account page (org, display name,
/// admin flag). A profiles row may not exist yet; everything is then null.
pub async fn show(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    let row: Option<(Option<String>, Option<String>, bool)> = sqlx::query_as(
        "select org, display_name, is_admin from profiles where id = $1",
    )
    .bind(user.id)
    .fetch_optional(&state.db)
    .await?;
    let (org, display_name, is_admin) = row.unwrap_or((None, None, false));
    Ok(Json(json!({
        "ok": true,
        "email": user.email,
        "org": org,
        "displayName": display_name,
        "isAdmin": is_admin,
    })))
}

#[derive(Deserialize)]
pub struct OrgBody {
    pub org: String,
}

/// PUT /api/account/org — set or change the publisher org. One transaction:
/// upsert the profile, then relabel every pack the account owns. Returns how
/// many packs were retagged so the UI can confirm the cascade.
pub async fn set_org(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<OrgBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    let packs_affected = apply_org_change(&state.db, user.id, body.org.trim()).await?;
    Ok(Json(json!({
        "ok": true,
        "org": body.org.trim(),
        "packsAffected": packs_affected,
    })))
}

/// Validation + transactional cascade, split from the handler so tests can
/// drive it directly (same pattern as check_submission_rules).
pub(crate) async fn apply_org_change(
    db: &sqlx::PgPool,
    user_id: uuid::Uuid,
    org: &str,
) -> Result<i64, AppError> {
    if !valid_org_slug(org) {
        return Err(AppError::bad_request(
            "org must be lowercase kebab-case (letters, digits, dashes), up to 39 chars, and not reserved",
        ));
    }

    let mut tx = db.begin().await?;
    // Claim the slug. A unique-index violation (23505 on
    // profiles_org_unique_idx) means another account holds it.
    let claim = sqlx::query(
        "insert into profiles (id, org) values ($1, $2) \
         on conflict (id) do update set org = excluded.org",
    )
    .bind(user_id)
    .bind(org)
    .execute(&mut *tx)
    .await;
    if let Err(e) = claim {
        if e.as_database_error().is_some_and(|d| d.is_unique_violation()) {
            return Err(AppError::conflict(format!("org {org:?} is already taken")));
        }
        return Err(e.into());
    }
    let updated = sqlx::query("update packs set org = $2 where author_id = $1")
        .bind(user_id)
        .bind(org)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(updated.rows_affected() as i64)
}
