//! Submission intake: authenticated authors POST a raw .tgz plus form fields;
//! the verifier sidecar judges the pack; only fully-green packs enter the
//! human review queue (reviewers never see machine-rejectable junk).

use std::sync::Arc;

use axum::extract::{Multipart, Path, State};
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::{require_admin, require_user, AuthUser};
use crate::config::{CATEGORY_WHITELIST, LICENSE_WHITELIST, MAX_ARCHIVE_BYTES};
use crate::error::AppError;
use crate::semver;
use crate::state::AppState;
use crate::storage;

/// RFC3339 now, embedded into the stored verification report.
fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[derive(sqlx::FromRow)]
struct SubmissionRow {
    id: Uuid,
    pack_name: String,
    version: String,
    uploader: Uuid,
    status: String,
    report: Value,
    review_note: Option<String>,
}

/// POST /api/submissions — multipart/form-data:
///   archive=<file.tgz>, author=<display name>, category, license, [homepage]
pub async fn submit(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    multipart: Result<Multipart, axum::extract::multipart::MultipartRejection>,
) -> Result<Json<Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    let mut multipart = match multipart {
        Ok(m) => m,
        Err(_) => return Err(AppError::bad_request("expected multipart/form-data")),
    };

    let mut form = FormFields::default();
    let mut archive: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("bad multipart body: {e}")))?
    {
        match field.name().unwrap_or_default() {
            "archive" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::bad_request(format!("unreadable archive: {e}")))?;
                if bytes.len() > MAX_ARCHIVE_BYTES {
                    return Err(AppError::bad_request(format!(
                        "archive exceeds {MAX_ARCHIVE_BYTES} bytes"
                    )));
                }
                archive = Some(bytes.to_vec());
            }
            "author" => form.author = Some(read_text(field).await?),
            "category" => form.category = Some(read_text(field).await?),
            "license" => form.license = Some(read_text(field).await?),
            "homepage" => form.homepage = Some(read_text(field).await?),
            _ => {}
        }
    }
    let archive = archive.ok_or_else(|| AppError::bad_request("missing archive field"))?;
    let (author, category, license, homepage) = form.into_validated()?;

    // 1. Machine verification via the sidecar — the only pack-semantics judge.
    let verification: Value = state
        .http
        .post(format!("{}/verify", state.cfg.verifier_url))
        .header("content-type", "application/octet-stream")
        .body(archive.clone())
        .send()
        .await
        .map_err(|e| AppError::internal(format!("verifier unreachable: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::internal(format!("verifier response unreadable: {e}")))?;
    let report = verification
        .get("report")
        .cloned()
        .ok_or_else(|| AppError::internal("verifier response missing report"))?;
    if verification.get("ok").and_then(Value::as_bool) != Some(true) {
        // Not accepted: surfaced to the author immediately, nothing stored.
        return Ok(Json(json!({
            "ok": false,
            "error": "verification failed — fix the report and resubmit",
            "report": report,
        })));
    }

    let pack_name = report
        .get("packName")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::internal("verifier report missing packName"))?
        .to_string();
    let version = report
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::internal("verifier report missing version"))?
        .to_string();
    let content_hash = report
        .get("contentHash")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    // Timestamp the report with when verification actually ran.
    let mut report = report;
    report["checkedAt"] = Value::String(now_rfc3339());

    // 2. Market rules (ownership, version monotonicity, queue dedup).
    check_submission_rules(&state.db, &user, &pack_name, &version, &content_hash).await?;

    // 3. Archive the raw upload, then enqueue for human review.
    let submission_id = Uuid::new_v4();
    let archive_path = format!("{}.tgz", submission_id);
    storage::upload(&state, storage::BUCKET_UPLOADS, &archive_path, archive).await?;

    sqlx::query(
        "insert into submissions (id, pack_name, version, uploader, report, archive_path, \
         category, author, license, homepage) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(&submission_id)
    .bind(&pack_name)
    .bind(&version)
    .bind(user.id)
    .bind(&report)
    .bind(&archive_path)
    .bind(&category)
    .bind(&author)
    .bind(&license)
    .bind(&homepage)
    .execute(&state.db)
    .await?;

    Ok(Json(json!({ "ok": true, "id": submission_id, "report": report })))
}

#[derive(Default)]
struct FormFields {
    author: Option<String>,
    category: Option<String>,
    license: Option<String>,
    homepage: Option<String>,
}

async fn read_text(field: axum::extract::multipart::Field<'_>) -> Result<String, AppError> {
    let bytes = field
        .bytes()
        .await
        .map_err(|e| AppError::bad_request(format!("unreadable form field: {e}")))?;
    String::from_utf8(bytes.to_vec())
        .map_err(|_| AppError::bad_request("form fields must be UTF-8 text"))
}

impl FormFields {
    fn into_validated(self) -> Result<(String, String, String, Option<String>), AppError> {
        let author = non_empty(self.author, "author")?;
        let category = non_empty(self.category, "category")?;
        let license = non_empty(self.license, "license")?;
        if !CATEGORY_WHITELIST.contains(&category.as_str()) {
            return Err(AppError::bad_request(format!(
                "category must be one of: {}",
                CATEGORY_WHITELIST.join(", ")
            )));
        }
        if !LICENSE_WHITELIST.contains(&license.as_str()) {
            return Err(AppError::bad_request(format!(
                "license must be an SPDX id from: {}",
                LICENSE_WHITELIST.join(", ")
            )));
        }
        let homepage = match self.homepage {
            Some(h) if !h.trim().is_empty() => {
                if !h.starts_with("https://") && !h.starts_with("http://") {
                    return Err(AppError::bad_request("homepage must be an http(s) URL"));
                }
                Some(h)
            }
            _ => None,
        };
        Ok((author, category, license, homepage))
    }
}

fn non_empty(value: Option<String>, name: &str) -> Result<String, AppError> {
    match value {
        Some(v) if !v.trim().is_empty() => Ok(v.trim().to_string()),
        _ => Err(AppError::bad_request(format!("missing required field: {name}"))),
    }
}

/// Market rules enforced purely in SQL + code, independent of HTTP and the
/// verifier: queue dedup, version uniqueness, ownership, strict version
/// monotonicity, content change. `pub(crate)` so the integration tests can
/// drive it against a scratch database.
pub(crate) async fn check_submission_rules(
    db: &sqlx::PgPool,
    user: &AuthUser,
    pack_name: &str,
    version: &str,
    content_hash: &str,
) -> Result<(), AppError> {
    let pending: Option<(Uuid,)> =
        sqlx::query_as("select id from submissions where pack_name = $1 and status = 'pending'")
            .bind(pack_name)
            .fetch_optional(db)
            .await?;
    if pending.is_some() {
        return Err(AppError::conflict(
            "a submission for this pack is already awaiting review — try again later",
        ));
    }

    let taken: Option<(Uuid,)> = sqlx::query_as(
        "select id from submissions where pack_name = $1 and version = $2",
    )
    .bind(pack_name)
    .bind(version)
    .fetch_optional(db)
    .await?;
    if taken.is_some() {
        return Err(AppError::conflict(format!(
            "version {version} of this pack was already submitted — bump the version"
        )));
    }

    #[derive(sqlx::FromRow)]
    struct ExistingPack {
        author_id: Uuid,
        version: String,
        content_hash: String,
    }
    let existing: Option<ExistingPack> = sqlx::query_as(
        "select author_id, version, content_hash from packs where name = $1",
    )
    .bind(pack_name)
    .fetch_optional(db)
    .await?;
    if let Some(existing) = existing {
        if existing.author_id != user.id {
            return Err(AppError::forbidden(format!(
                "pack {pack_name} is owned by another account — updates must come from the original author"
            )));
        }
        if semver::compare(version, &existing.version) != std::cmp::Ordering::Greater {
            return Err(AppError::conflict(format!(
                "version must strictly increase: {} is already published",
                existing.version
            )));
        }
        if content_hash == existing.content_hash {
            return Err(AppError::conflict(
                "content is identical to the published version — nothing to update",
            ));
        }
    }
    Ok(())
}

/// GET /api/submissions/{id} — the author (or an admin) polls this for the
/// verification report and review status.
pub async fn get_submission(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    let row: Option<SubmissionRow> = sqlx::query_as(
        "select id, pack_name, version, uploader, status, report, review_note \
         from submissions where id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("no such submission"))?;
    if row.uploader != user.id {
        // Non-owners learn nothing beyond existence; admins get full detail.
        require_admin(&state, &headers).await?;
    }
    Ok(Json(json!({
        "ok": true,
        "id": row.id,
        "packName": row.pack_name,
        "version": row.version,
        "status": row.status,
        "report": row.report,
        "reviewNote": row.review_note,
    })))
}
