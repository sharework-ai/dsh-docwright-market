//! Owner pack management: the author's own view over the catalog. Ownership
//! is `packs.author_id` — the first-publishing account. Admins get nothing
//! here on purpose; their only lever remains the review queue.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::{require_user, AuthUser};
use crate::config::{CATEGORY_WHITELIST, LICENSE_WHITELIST};
use crate::error::AppError;
use crate::state::AppState;
use crate::storage;

#[derive(FromRow)]
struct PackRow {
    name: String,
    version: String,
    org: Option<String>,
    status: String,
    category: String,
    author: String,
    author_id: Uuid,
    license: String,
    homepage: Option<String>,
    added: chrono::DateTime<chrono::Utc>,
    updated: chrono::DateTime<chrono::Utc>,
}

const OWNER_PACK_COLUMNS: &str = "name, version, org, status, category, author, author_id, \
     license, homepage, added, updated";

fn pack_json(r: &PackRow) -> Value {
    json!({
        "name": r.name,
        "version": r.version,
        "org": r.org,
        "status": r.status,
        "category": r.category,
        "author": r.author,
        "license": r.license,
        "homepage": r.homepage,
        "added": r.added.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "updated": r.updated.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    })
}

async fn owned_pack(
    state: &AppState,
    user: &AuthUser,
    name: &str,
) -> Result<PackRow, AppError> {
    let row: Option<PackRow> = sqlx::query_as::<_, PackRow>(&format!(
        "select {OWNER_PACK_COLUMNS} from packs where name = $1"
    ))
    .bind(name)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found(format!("no such pack: {name}")))?;
    // Non-owners learn nothing beyond existence.
    if row.author_id != user.id {
        return Err(AppError::forbidden("only the pack owner can do that"));
    }
    Ok(row)
}

/// GET /api/my/packs — every pack the caller owns, disabled ones included.
pub async fn my_packs(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    let rows: Vec<PackRow> = sqlx::query_as::<_, PackRow>(&format!(
        "select {OWNER_PACK_COLUMNS} from packs where author_id = $1 order by name"
    ))
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "ok": true,
        "packs": rows.iter().map(pack_json).collect::<Vec<_>>(),
    })))
}

/// GET /api/my/packs/{name} — one owned pack, for the edit flow.
pub async fn my_packs_one(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    let row = owned_pack(&state, &user, &name).await?;
    Ok(Json(json!({ "ok": true, "pack": pack_json(&row) })))
}

/// A JSON string field: absent (or null) → None, string → trimmed Some.
/// Typed on the raw body so absent and explicit-null stay distinguishable —
/// serde's Option<T> folds JSON null into None and would erase that signal.
fn opt_str(body: &Value, key: &str) -> Result<Option<String>, AppError> {
    match body.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.trim().to_string())),
        Some(_) => Err(AppError::bad_request(format!("{key} must be a string"))),
    }
}

/// PATCH /api/my/packs/{name} — metadata-only edits. `description` is
/// deliberately not patchable: it comes from the manifest, so it changes with
/// a new version through the normal submission pipeline. The body stays raw
/// JSON for the tri-state homepage: string sets, null clears, absent keeps.
pub async fn patch_pack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    owned_pack(&state, &user, &name).await?;

    let category = opt_str(&body, "category")?;
    if let Some(c) = &category {
        if !CATEGORY_WHITELIST.contains(&c.as_str()) {
            return Err(AppError::bad_request(format!(
                "category must be one of: {}",
                CATEGORY_WHITELIST.join(", ")
            )));
        }
    }
    let license = opt_str(&body, "license")?;
    if let Some(l) = &license {
        if !LICENSE_WHITELIST.contains(&l.as_str()) {
            return Err(AppError::bad_request(format!(
                "license must be an SPDX id from: {}",
                LICENSE_WHITELIST.join(", ")
            )));
        }
    }
    let homepage: Option<Option<String>> = match body.get("homepage") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(s.clone())),
        Some(_) => return Err(AppError::bad_request("homepage must be a string or null")),
    };
    let author = opt_str(&body, "author")?;
    if let Some(a) = &author {
        if a.is_empty() {
            return Err(AppError::bad_request("author must not be empty"));
        }
    }
    if category.is_none() && license.is_none() && homepage.is_none() && author.is_none() {
        return Err(AppError::bad_request(
            "nothing to update: send at least one of category/license/homepage/author",
        ));
    }
    if let Some(Some(h)) = &homepage {
        if !h.starts_with("https://") && !h.starts_with("http://") {
            return Err(AppError::bad_request("homepage must be an http(s) URL"));
        }
    }

    sqlx::query(
        "update packs set \
           category = coalesce($2, category), \
           license = coalesce($3, license), \
           homepage = case when $4 then $5 else homepage end, \
           author = coalesce($6, author), \
           updated = now() \
         where name = $1",
    )
    .bind(&name)
    .bind(&category)
    .bind(&license)
    .bind(homepage.is_some())
    .bind(homepage.clone().flatten())
    .bind(&author)
    .execute(&state.db)
    .await?;

    let row: Option<PackRow> = sqlx::query_as::<_, PackRow>(&format!(
        "select {OWNER_PACK_COLUMNS} from packs where name = $1"
    ))
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true, "pack": row.map(|r| pack_json(&r)) })))
}

async fn set_status(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    name: &str,
    status: &str,
) -> Result<Json<Value>, AppError> {
    let user = require_user(state, headers).await?;
    owned_pack(state, &user, name).await?;
    sqlx::query("update packs set status = $2 where name = $1")
        .bind(name)
        .bind(status)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true, "name": name, "status": status })))
}

/// POST /api/my/packs/{name}/disable — hide from the public catalog, block
/// downloads (410). Reversible.
pub async fn disable_pack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    set_status(&state, &headers, &name, "disabled").await
}

/// POST /api/my/packs/{name}/enable — bring a disabled pack back.
pub async fn enable_pack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    set_status(&state, &headers, &name, "active").await
}

/// DELETE /api/my/packs/{name} — hard delete: catalog row + version history
/// (cascade) + Storage tarballs. Submissions history is kept on purpose, so
/// the (name, version) pairs stay permanently reserved. Storage deletes are
/// best-effort after the DB commit: the DB is the source of truth, orphaned
/// objects are merely unreachable bytes.
pub async fn delete_pack(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    owned_pack(&state, &user, &name).await?;

    let mut tx = state.db.begin().await?;
    // Every publish writes a history row, so the version table covers the
    // full set of stored objects for this pack.
    let paths: Vec<(String,)> =
        sqlx::query_as("select tarball_path from pack_versions where name = $1")
            .bind(&name)
            .fetch_all(&mut *tx)
            .await?;
    sqlx::query("delete from pack_versions where name = $1")
        .bind(&name)
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from packs where name = $1")
        .bind(&name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let mut storage_warnings: Vec<String> = Vec::new();
    for (path,) in &paths {
        // "tarballs/<file>.tgz" — strip the bucket prefix for the REST call.
        let Some(object) = path.strip_prefix("tarballs/") else { continue };
        if let Err(e) = storage::delete(&state, storage::BUCKET_TARBALLS, object).await {
            storage_warnings.push(format!("{path}: {e}"));
        }
    }
    Ok(Json(json!({
        "ok": true,
        "name": name,
        "deletedVersions": paths.len(),
        "storageWarnings": storage_warnings,
    })))
}

/// GET /api/my/submissions — the caller's submission history, newest first.
/// Full reports stay on GET /api/submissions/{id}.
pub async fn my_submissions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        pack_name: String,
        version: String,
        status: String,
        created_at: chrono::DateTime<chrono::Utc>,
        review_note: Option<String>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "select id, pack_name, version, status, created_at, review_note \
         from submissions where uploader = $1 order by created_at desc limit 100",
    )
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "ok": true,
        "submissions": rows.into_iter().map(|r| json!({
            "id": r.id,
            "packName": r.pack_name,
            "version": r.version,
            "status": r.status,
            "createdAt": r.created_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "reviewNote": r.review_note,
        })).collect::<Vec<_>>(),
    })))
}

/// DELETE /api/my/submissions/{id} — withdraw a pending submission. Approved
/// or rejected ones are historical record and cannot be withdrawn; that also
/// keeps their (name, version) reservation intact.
pub async fn withdraw_submission(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    let row: Option<(Uuid, String, Option<String>)> = sqlx::query_as(
        "select uploader, status, archive_path from submissions where id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let (uploader, status, archive_path) =
        row.ok_or_else(|| AppError::not_found("no such submission"))?;
    if uploader != user.id {
        return Err(AppError::forbidden("only the submitter can withdraw"));
    }
    if status != "pending" {
        return Err(AppError::conflict(format!(
            "submission is already {status} and can no longer be withdrawn"
        )));
    }
    sqlx::query("delete from submissions where id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;
    let mut storage_warnings: Vec<String> = Vec::new();
    if let Some(path) = archive_path {
        if let Err(e) = storage::delete(&state, storage::BUCKET_UPLOADS, &path).await {
            storage_warnings.push(format!("{path}: {e}"));
        }
    }
    Ok(Json(json!({ "ok": true, "storageWarnings": storage_warnings })))
}
