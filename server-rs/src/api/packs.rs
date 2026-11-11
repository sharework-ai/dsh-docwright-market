//! Public read surface: pack list, per-pack detail (with the health sheet),
//! the machine-consumable format-2 index, and tarball downloads. Disabled
//! packs are absent from this whole surface (downloads answer 410, not 404 —
//! "existed, withheld"), and version history makes every past release
//! downloadable again.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
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
    pub org: Option<String>,
    pub status: String,
    pub added: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

const PACK_COLUMNS: &str = "name, version, description, category, author, author_id, license, \
     homepage, consumes, languages, requires_topic, artifact_prefix, has_function_rules, \
     content_hash, tarball_path, tarball_sha256, org, status, added, updated";

/// GET /api/packs — the browser list page; `?q=word word` turns it into the
/// keyword search the doc_market_search tool drives (traditional matching —
/// every term must hit (AND), any field counts (OR): name, category, and
/// both description languages via ILIKE; parameter binding keeps the terms
/// literal). The catalog is small; vectors and FTS are deliberately not
/// involved — the CALLER (the model) decomposes intent into keywords.
#[derive(serde::Deserialize)]
pub struct ListQuery {
    q: Option<String>,
}

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(list_query): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let terms: Vec<String> = list_query
        .q
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let rows: Vec<PackRow> = if terms.is_empty() {
        sqlx::query_as::<_, PackRow>(
            &format!("select {PACK_COLUMNS} from packs where status = 'active' order by name"),
        )
        .fetch_all(&state.db)
        .await?
    } else {
        // The OR of fields per term, the AND across terms. Patterns are bound
        // parameters built from the raw terms (%term%) — no user text enters
        // the SQL string itself.
        let mut where_parts: Vec<String> = Vec::new();
        let mut params: Vec<String> = Vec::new();
        for term in &terms {
            params.push(format!("%{}%", term.to_lowercase()));
            let idx = params.len();
            where_parts.push(format!(
                "(name ilike ${idx} or category ilike ${idx} or description->>'en' ilike ${idx} or description->>'zh' ilike ${idx})"
            ));
        }
        let sql = format!(
            "select {PACK_COLUMNS} from packs where status = 'active' and {} order by name limit 50",
            where_parts.join(" and ")
        );
        let mut query = sqlx::query_as::<_, PackRow>(&sql);
        for param in &params {
            query = query.bind(param);
        }
        query.fetch_all(&state.db).await?
    };
    Ok(Json(json!({
        "ok": true,
        "packs": rows.iter().map(|r| pack_json(r)).collect::<Vec<_>>(),
    })))
}

/// GET /api/packs/{name} — detail incl. the latest health sheet (from the
/// approved submission's verification report) and the full version history.
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
        "versions": versions_json(&state, &name).await?,
        "health": health_sheet,
        "verifiedAt": verified_at,
    })))
}

/// GET /api/index — format-2 machine catalog (install.mjs and future hosts).
/// Top-level per-pack fields keep pointing at the latest version so an
/// unpinned install behaves exactly as on format 1.
pub async fn index(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let rows: Vec<PackRow> = sqlx::query_as::<_, PackRow>(
        &format!("select {PACK_COLUMNS} from packs where status = 'active' order by name"),
    )
    .fetch_all(&state.db)
    .await?;
    let mut packs = Vec::with_capacity(rows.len());
    for r in &rows {
        packs.push(json!({
            "name": r.name,
            "version": r.version,
            "description": r.description,
            "category": r.category,
            "author": r.author,
            "org": r.org,
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
            "versions": versions_json(&state, &r.name).await?,
        }));
    }
    Ok(Json(json!({
        "format": 2,
        "generatedAt": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "origin": state.cfg.public_origin,
        "source": state.cfg.source_url,
        "packs": packs,
    })))
}

/// GET /tarballs/{name}-{version}.tgz — proxy to Storage (its endpoint is
/// cluster-internal; a 302 could never be followed by external clients).
/// Lookup matches `pack_versions.tarball_path` exactly — never parse the
/// filename, pack names and semver prereleases both contain dashes. Disabled
/// packs answer 410; deleted packs (and stray guesses) 404.
pub async fn tarball(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
) -> Result<Response, AppError> {
    if !file.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
        || file.contains("..")
    {
        return Err(AppError::bad_request("malformed tarball name"));
    }
    // The stored path carries the bucket context ("<bucket>/<file>"); derive
    // the prefix from the SAME env-backed bucket name the publish path wrote,
    // so a deployment with renamed buckets (dev) matches its own rows.
    let object_path = format!("{}/{}", storage::bucket_tarballs(), file);
    let hit: Option<(String,)> = sqlx::query_as(
        "select p.status from pack_versions v join packs p on p.name = v.name \
         where v.tarball_path = $1",
    )
    .bind(&object_path)
    .fetch_optional(&state.db)
    .await?;
    match hit {
        Some((status,)) if status == "disabled" => {
            Err(AppError::gone("this pack has been disabled by its owner"))
        }
        Some(_) => {
            let bytes = storage::download_public(&state, storage::bucket_tarballs(), &file).await?;
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
    let row: Option<PackRow> = sqlx::query_as::<_, PackRow>(&format!(
        "select {PACK_COLUMNS} from packs where name = $1"
    ))
    .bind(name)
    .fetch_optional(&state.db)
    .await?
    .filter(|r| r.status == "active");
    row.ok_or_else(|| AppError::not_found(format!("no such pack: {name}")))
}

/// The version history of one pack, newest publish first. `tarball` uses the
/// stored object path verbatim — no filename reconstruction.
async fn versions_json(state: &AppState, name: &str) -> Result<Vec<Value>, AppError> {
    #[derive(FromRow)]
    struct VersionRow {
        version: String,
        content_hash: String,
        tarball_path: String,
        tarball_sha256: String,
        has_function_rules: bool,
        published_at: DateTime<Utc>,
    }
    let rows: Vec<VersionRow> = sqlx::query_as(
        "select version, content_hash, tarball_path, tarball_sha256, has_function_rules, \
         published_at from pack_versions where name = $1 order by published_at desc",
    )
    .bind(name)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|v| {
            json!({
                "version": v.version,
                "tarball": format!("/tarballs/{}", v.tarball_path.rsplit('/').next().unwrap_or(&v.tarball_path)),
                "tarballSha256": v.tarball_sha256,
                "contentHash": v.content_hash,
                "hasFunctionRules": v.has_function_rules,
                "publishedAt": v.published_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            })
        })
        .collect())
}

fn pack_json(r: &PackRow) -> Value {
    json!({
        "name": r.name,
        "version": r.version,
        "description": r.description,
        "category": r.category,
        "author": r.author,
        "org": r.org,
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
