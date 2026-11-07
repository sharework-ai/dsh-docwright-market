//! Admin review: queue inspection and the approve/reject decision. Approve
//! publishes immediately (Storage copy + packs upsert); this is the only path
//! that writes into the catalog.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::{require_admin, AuthUser};
use crate::error::AppError;
use crate::state::AppState;
use crate::storage;

#[derive(sqlx::FromRow)]
pub(crate) struct SubmissionRow {
    pub(crate) id: Uuid,
    pub(crate) pack_name: String,
    pub(crate) version: String,
    pub(crate) uploader: Uuid,
    pub(crate) archive_path: Option<String>,
    pub(crate) report: Value,
    pub(crate) category: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) license: Option<String>,
    pub(crate) homepage: Option<String>,
}

/// GET /api/admin/queue — pending submissions, oldest first, full reports.
pub async fn queue(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &headers).await?;
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        pack_name: String,
        version: String,
        report: Value,
        category: Option<String>,
        author: Option<String>,
        license: Option<String>,
        homepage: Option<String>,
        created_at: DateTime<Utc>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "select id, pack_name, version, report, category, author, license, homepage, created_at \
         from submissions where status = 'pending' order by created_at asc",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "ok": true,
        "queue": rows.into_iter().map(|r| json!({
            "id": r.id,
            "packName": r.pack_name,
            "version": r.version,
            "report": r.report,
            "category": r.category,
            "author": r.author,
            "license": r.license,
            "homepage": r.homepage,
            "createdAt": r.created_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
pub struct ReviewBody {
    pub action: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// POST /api/admin/review/{id} — `{action: "approve"|"reject", note?}`.
/// Approve = publish: copy the verified archive into the public tarballs
/// bucket and upsert the catalog row. Reject = archive the decision.
pub async fn review(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Result<Json<Value>, AppError> {
    let admin: AuthUser = require_admin(&state, &headers).await?;
    let row: Option<SubmissionRow> = sqlx::query_as(
        "select id, pack_name, version, uploader, archive_path, report, category, author, license, homepage \
         from submissions where id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let sub = row.ok_or_else(|| AppError::not_found("no such submission"))?;

    match body.action.as_str() {
        "reject" => {
            mark_reviewed(&state.db, id, "rejected", body.note.as_deref(), admin.id).await?;
            Ok(Json(json!({ "ok": true, "status": "rejected" })))
        }
        "approve" => {
            publish(&state, &admin, &sub, body.note.as_deref()).await?;
            Ok(Json(json!({ "ok": true, "status": "approved", "pack": sub.pack_name })))
        }
        other => Err(AppError::bad_request(format!(
            "action must be approve or reject, got: {other}"
        ))),
    }
}

/// The publish chain. The archived upload is the single source of truth: the
/// reviewer approved exactly what the verifier validated, byte for byte.
async fn publish(
    state: &AppState,
    admin: &AuthUser,
    sub: &SubmissionRow,
    note: Option<&str>,
) -> Result<(), AppError> {
    let archive_path = sub
        .archive_path
        .clone()
        .ok_or_else(|| AppError::internal("approved submission lost its archive path"))?;
    let bytes = storage::download(state, storage::BUCKET_UPLOADS, &archive_path).await?;
    let sha = storage::sha256_hex(&bytes);
    let object_path = format!("{}-{}.tgz", sub.pack_name, sub.version);
    storage::upload(state, storage::BUCKET_TARBALLS, &object_path, bytes).await?;
    // packs.tarball_path carries the bucket context so the download route can
    // match it: "tarballs/<name>-<version>.tgz".
    let record_path = format!("{}/{}", storage::BUCKET_TARBALLS, object_path);

    let manifest = sub
        .report
        .get("manifest")
        .cloned()
        .ok_or_else(|| AppError::internal("verification report is missing the manifest"))?;
    let content_hash = sub
        .report
        .get("contentHash")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let has_function_rules = sub
        .report
        .get("hasFunctionRules")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // The publisher org comes from the uploader's current profile. The submit
    // gate makes a missing org unreachable through the UI; this defends the
    // admin API and any historical queue entries.
    let org: Option<(Option<String>,)> =
        sqlx::query_as("select org from profiles where id = $1")
            .bind(sub.uploader)
            .fetch_optional(&state.db)
            .await?;
    let org = org
        .and_then(|(o,)| o)
        .ok_or_else(|| AppError::conflict("uploader has no publisher org — cannot publish"))?;

    upsert_published_pack(
        &state.db,
        sub,
        &manifest,
        &content_hash,
        has_function_rules,
        &record_path,
        &sha,
        &org,
    )
    .await?;
    mark_reviewed(&state.db, sub.id, "approved", note, admin.id).await?;
    Ok(())
}

/// Catalog write: insert the first publish or overwrite in place (updates
/// keep `added`, bump `updated`, keep `status` — a disabled pack stays
/// disabled until its owner re-enables), and append the version history row.
/// Split from `publish` so tests can drive it against a scratch database
/// without Storage.
pub(crate) async fn upsert_published_pack(
    db: &sqlx::PgPool,
    sub: &SubmissionRow,
    manifest: &Value,
    content_hash: &str,
    has_function_rules: bool,
    tarball_path: &str,
    tarball_sha256: &str,
    org: &str,
) -> Result<(), AppError> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "insert into packs (name, version, description, category, author, author_id, license, \
         homepage, consumes, languages, requires_topic, artifact_prefix, has_function_rules, \
         content_hash, tarball_path, tarball_sha256, org) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17) \
         on conflict (name) do update set \
           version = excluded.version, \
           description = excluded.description, \
           category = excluded.category, \
           author = excluded.author, \
           author_id = excluded.author_id, \
           license = excluded.license, \
           homepage = excluded.homepage, \
           consumes = excluded.consumes, \
           languages = excluded.languages, \
           requires_topic = excluded.requires_topic, \
           artifact_prefix = excluded.artifact_prefix, \
           has_function_rules = excluded.has_function_rules, \
           content_hash = excluded.content_hash, \
           tarball_path = excluded.tarball_path, \
           tarball_sha256 = excluded.tarball_sha256, \
           org = excluded.org, \
           updated = now()",
    )
    .bind(&sub.pack_name)
    .bind(&sub.version)
    .bind(manifest.get("description").cloned().unwrap_or(Value::Null))
    .bind(sub.category.clone().unwrap_or_else(|| "other".into()))
    .bind(sub.author.clone().unwrap_or_else(|| sub.pack_name.clone()))
    .bind(sub.uploader) // pack ownership belongs to the submitting account
    .bind(sub.license.clone().unwrap_or_else(|| "MIT".into()))
    .bind(sub.homepage.clone())
    .bind(manifest.get("consumes").cloned().unwrap_or(json!([])))
    .bind(manifest.get("languages").cloned().unwrap_or(json!([])))
    .bind(manifest.get("requiresTopic").and_then(Value::as_bool).unwrap_or(false))
    .bind(manifest.get("artifactPrefix").and_then(Value::as_str))
    .bind(has_function_rules)
    .bind(content_hash)
    .bind(tarball_path)
    .bind(tarball_sha256)
    .bind(org)
    .execute(&mut *tx)
    .await?;
    // History row for this exact version. `on conflict do nothing` keeps an
    // idempotent re-approve from duplicating.
    sqlx::query(
        "insert into pack_versions (name, version, description, content_hash, tarball_path, \
         tarball_sha256, has_function_rules, consumes, languages, requires_topic, \
         artifact_prefix, published_at, submission_id) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, now(), $12) \
         on conflict (name, version) do nothing",
    )
    .bind(&sub.pack_name)
    .bind(&sub.version)
    .bind(manifest.get("description").cloned().unwrap_or(Value::Null))
    .bind(content_hash)
    .bind(tarball_path)
    .bind(tarball_sha256)
    .bind(has_function_rules)
    .bind(manifest.get("consumes").cloned().unwrap_or(json!([])))
    .bind(manifest.get("languages").cloned().unwrap_or(json!([])))
    .bind(manifest.get("requiresTopic").and_then(Value::as_bool).unwrap_or(false))
    .bind(manifest.get("artifactPrefix").and_then(Value::as_str))
    .bind(sub.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Shared review-state transition (approve/reject) with reviewer stamp.
pub(crate) async fn mark_reviewed(
    db: &sqlx::PgPool,
    submission_id: Uuid,
    status: &str,
    note: Option<&str>,
    reviewer: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        "update submissions set status = $2, review_note = $3, \
         reviewed_by = $4, reviewed_at = now() where id = $1",
    )
    .bind(submission_id)
    .bind(status)
    .bind(note)
    .bind(reviewer)
    .execute(db)
    .await?;
    Ok(())
}
