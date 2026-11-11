//! Admin review: queue inspection and the approve/reject decision. Approve
//! publishes immediately (Storage copy + packs upsert); this is the only path
//! that writes into the catalog.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
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
    pub(crate) kind: String,
    pub(crate) suite_meta: Option<Value>,
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
        kind: String,
        suite_meta: Option<Value>,
        created_at: DateTime<Utc>,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "select id, pack_name, version, report, category, author, license, homepage, kind, \
         suite_meta, created_at from submissions where status = 'pending' order by created_at asc",
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
            "kind": r.kind,
            "suiteMeta": r.suite_meta,
            "createdAt": r.created_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        })).collect::<Vec<_>>(),
    })))
}

/// GET /api/admin/reviews — the review HISTORY: decided submissions
/// (approved/rejected), newest first, with the reviewer stamp. `status` query
/// param filters ("approved" | "rejected", default: both). The pending queue
/// lives at /api/admin/queue — this is the audit view of what was decided.
pub async fn reviews(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    require_admin(&state, &headers).await?;
    let status_filter = params.get("status").map(String::as_str);
    if !matches!(status_filter, None | Some("approved") | Some("rejected")) {
        return Err(AppError::bad_request(
            "status must be \"approved\" or \"rejected\" (omit it for both)",
        ));
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        pack_name: String,
        version: String,
        kind: String,
        status: String,
        review_note: Option<String>,
        reviewed_at: Option<DateTime<Utc>>,
        reviewer: Option<String>,
        uploader: Option<String>,
        created_at: DateTime<Utc>,
    }
    let where_clause = match status_filter {
        Some(s) => format!("and s.status = '{s}'"),
        None => String::new(),
    };
    // status is whitelisted above, so interpolating it is safe.
    // NOTE: no join to auth.users here — the dev market's role has no auth
    // schema grant (prod's superuser would hide that). Uploader and reviewer
    // both come from profiles via the role's search_path (market_dev | public).
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "select s.id, s.pack_name, s.version, s.kind, s.status, s.review_note, \
         s.reviewed_at, reviewer.display_name as reviewer, uploader.display_name as uploader, s.created_at \
         from submissions s \
         left join profiles uploader on uploader.id = s.uploader \
         left join profiles reviewer on reviewer.id = s.reviewed_by \
         where s.status in ('approved', 'rejected') {where_clause} \
         order by s.reviewed_at desc nulls last, s.created_at desc limit 200"
    ))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "ok": true,
        "reviews": rows.into_iter().map(|r| json!({
            "id": r.id,
            "packName": r.pack_name,
            "version": r.version,
            "kind": r.kind,
            "status": r.status,
            "reviewNote": r.review_note,
            "reviewedAt": r.reviewed_at.map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
            "reviewer": r.reviewer,
            "uploader": r.uploader,
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
        "select id, pack_name, version, uploader, archive_path, report, category, author, license, \
         homepage, kind, suite_meta from submissions where id = $1",
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
        "approve" if sub.kind == "suite" => {
            publish_suite(&state.db, &sub).await?;
            mark_reviewed(&state.db, sub.id, "approved", body.note.as_deref(), admin.id).await?;
            Ok(Json(json!({ "ok": true, "status": "approved", "suite": sub.pack_name })))
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
    let bytes = storage::download(state, storage::bucket_uploads(), &archive_path).await?;
    let sha = storage::sha256_hex(&bytes);
    let object_path = format!("{}-{}.tgz", sub.pack_name, sub.version);
    storage::upload(state, storage::bucket_tarballs(), &object_path, bytes).await?;
    // packs.tarball_path carries the bucket context so the download route can
    // match it: "tarballs/<name>-<version>.tgz".
    let record_path = format!("{}/{}", storage::bucket_tarballs(), object_path);

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

/// Suite publish: form-only submissions carry no archive, so the catalog
/// write is pure SQL. `action: "delete"` removes the row; anything else is an
/// upsert whose form was validated at submission time (member packs existing
/// then; ones deleted since stay on the list and read back as `missing`).
/// Split from `review` so tests can drive it against a scratch database.
pub(crate) async fn publish_suite(db: &sqlx::PgPool, sub: &SubmissionRow) -> Result<(), AppError> {
    let meta = sub
        .suite_meta
        .clone()
        .ok_or_else(|| AppError::internal("suite submission lost its form content"))?;
    if meta.get("action").and_then(Value::as_str) == Some("delete") {
        sqlx::query("delete from suites where name = $1")
            .bind(&sub.pack_name)
            .execute(db)
            .await?;
        return Ok(());
    }
    // The display author comes from the submitter's profile (the suite form
    // has no author field); the suite name is the fallback.
    let author: Option<(Option<String>,)> =
        sqlx::query_as("select display_name from profiles where id = $1")
            .bind(sub.uploader)
            .fetch_optional(db)
            .await?;
    let author = author
        .and_then(|(a,)| a)
        .unwrap_or_else(|| sub.pack_name.clone());
    sqlx::query(
        "insert into suites (name, version, description, category, author, author_id, members) \
         values ($1, $2, $3, $4, $5, $6, $7) \
         on conflict (name) do update set \
           version = excluded.version, \
           description = excluded.description, \
           category = excluded.category, \
           author = excluded.author, \
           author_id = excluded.author_id, \
           members = excluded.members, \
           updated = now()",
    )
    .bind(&sub.pack_name)
    .bind(&sub.version)
    .bind(meta.get("description").cloned().unwrap_or(json!({ "en": "" })))
    .bind(meta.get("category").and_then(Value::as_str).unwrap_or("other"))
    .bind(&author)
    .bind(sub.uploader)
    .bind(meta.get("members").cloned().unwrap_or(json!([])))
    .execute(db)
    .await?;
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
