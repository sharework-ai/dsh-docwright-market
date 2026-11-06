//! Public read surface: pack list, per-pack detail (with the health sheet),
//! the machine-consumable format-1 index, and tarball downloads.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::FromRow;

use crate::error::AppError;
use crate::state::AppState;
use crate::storage;

#[derive(FromRow)]
pub struct PackRow {
    pub name: String,
    pub version: String,
    pub description: Value,
    pub category: String,
    pub author: String,
    pub author_id: uuid::Uuid,
    pub license: String,
    pub homepage: Option<String>,
    pub consumes: Value,
    pub languages: Value,
    pub requires_topic: bool,
    pub artifact_prefix: Option<String>,
    pub has_function_rules: bool,
    pub content_hash: String,
    pub tarball_path: String,
    pub tarball_sha256: String,
    pub added: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

const PACK_COLUMNS: &str = "name, version, description, category, author, author_id, license, \
     homepage, consumes, languages, requires_topic, artifact_prefix, has_function_rules, \
     content_hash, tarball_path, tarball_sha256, added, updated";

/// GET /api/packs — the browser list page.
pub async fn list(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let rows: Vec<PackRow> = sqlx::query_as::<_, PackRow>(
        &format!("select {PACK_COLUMNS} from packs order by name"),
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "ok": true,
        "packs": rows.iter().map(|r| pack_json(r)).collect::<Vec<_>>(),
    })))
}

/// GET /api/packs/{name} — detail incl. the latest health sheet (from the
/// approved submission's verification report).
pub async fn detail(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    let row = fetch_pack(&state, &name).await?;
    let health: Option<(Value,)> = sqlx::query_as(
        "select report from submissions \
         where pack_name = $1 and status = 'approved' \
         order by reviewed_at desc nulls last limit 1",
    )
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;
    let (health_sheet, verified_at) = match health {
        Some((report,)) => (
            report.get("checkOutput").cloned().unwrap_or(Value::Null),
            report.get("checkedAt").cloned().unwrap_or(Value::Null),
        ),
        None => (Value::Null, Value::Null),
    };
    Ok(Json(json!({
        "ok": true,
        "pack": pack_json(&row),
        "health": health_sheet,
        "verifiedAt": verified_at,
    })))
}

/// GET /api/index — format-1 machine catalog (install.mjs and future hosts).
pub async fn index(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let rows: Vec<PackRow> = sqlx::query_as::<_, PackRow>(
        &format!("select {PACK_COLUMNS} from packs order by name"),
    )
    .fetch_all(&state.db)
    .await?;
    let packs = rows
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "version": r.version,
                "description": r.description,
                "category": r.category,
                "author": r.author,
                "license": r.license,
                "consumes": r.consumes,
                "languages": r.languages,
                "requiresTopic": r.requires_topic,
                "artifactPrefix": r.artifact_prefix,
                "hasFunctionRules": r.has_function_rules,
                "contentHash": r.content_hash,
                "tarball": format!("/tarballs/{}-{}.tgz", r.name, r.version),
                "tarballSha256": r.tarball_sha256,
                "page": format!("/p/{}", r.name),
                "added": r.added.date().to_string(),
                "updated": r.updated.date().to_string(),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "format": 1,
        "generatedAt": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "origin": state.cfg.public_origin,
        "source": state.cfg.source_url,
        "packs": packs,
    })))
}

/// GET /tarballs/{name}-{version}.tgz — 302 to Storage's public URL. Only
/// paths present in the packs table are served (no guessing, no traversal).
pub async fn tarball(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
) -> Result<Response, AppError> {
    if !file.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
        || file.contains("..")
    {
        return Err(AppError::bad_request("malformed tarball name"));
    }
    let object_path = format!("tarballs/{file}");
    let hit: Option<(String,)> = sqlx::query_as(
        "select tarball_path from packs where tarball_path = $1",
    )
    .bind(&object_path)
    .fetch_optional(&state.db)
    .await?;
    match hit {
        // Proxy instead of 302: the Storage endpoint is cluster-internal
        // (supabase-envoy svc) — external clients could never follow that
        // redirect. Small tarballs, one extra hop through the server is fine.
        Some(_) => {
            let bytes = storage::download_public(&state, storage::BUCKET_TARBALLS, &file).await?;
            Ok((
                [(axum::http::header::CONTENT_TYPE, "application/gzip")],
                axum::body::Body::from(bytes),
            )
                .into_response())
        }
        None => Err(AppError::not_found("no such pack tarball")),
    }
}

async fn fetch_pack(state: &AppState, name: &str) -> Result<PackRow, AppError> {
    sqlx::query_as::<_, PackRow>(&format!(
        "select {PACK_COLUMNS} from packs where name = $1"
    ))
    .bind(name)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found(format!("no such pack: {name}")))
}

fn pack_json(r: &PackRow) -> Value {
    json!({
        "name": r.name,
        "version": r.version,
        "description": r.description,
        "category": r.category,
        "author": r.author,
        "license": r.license,
        "homepage": r.homepage,
        "consumes": r.consumes,
        "languages": r.languages,
        "requiresTopic": r.requires_topic,
        "artifactPrefix": r.artifact_prefix,
        "hasFunctionRules": r.has_function_rules,
        "tarball": format!("/tarballs/{}-{}.tgz", r.name, r.version),
        "added": r.added.date().to_string(),
        "updated": r.updated.date().to_string(),
    })
}
