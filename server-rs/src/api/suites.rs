//! Suites: curated groups of packs. Publishing is a form (no tarball, no
//! verifier) that enters the same human review queue as packs; the catalog
//! write happens only when an admin approves. The read surface (list, detail,
//! membership map for pack cards) hides disabled suites entirely.

use std::collections::BTreeMap;

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::auth::{require_org, require_user};
use crate::config::CATEGORY_WHITELIST;
use crate::error::AppError;
use crate::semver;
use crate::state::AppState;

/// A valid suite name: same slug discipline as pack names — lowercase
/// kebab-case, ≤ 64 chars.
fn valid_suite_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let head = name.as_bytes()[0];
    if !(head.is_ascii_lowercase() || head.is_ascii_digit()) {
        return false;
    }
    name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.contains("--")
        && !name.ends_with('-')
}

/// One suite submission's form content, parsed from the raw JSON body.
#[derive(Debug, Clone)]
pub(crate) struct SuiteMeta {
    pub name: String,
    pub version: String,
    pub description: Value,
    pub category: String,
    pub members: Vec<String>,
}

impl SuiteMeta {
    /// Parse + shape-validate the form: name slug, bilingual description,
    /// whitelisted category, non-empty version, deduplicated member list.
    /// Member existence (against `packs`) is DB work — see check_suite_rules.
    pub(crate) fn parse(body: &Value) -> Result<SuiteMeta, AppError> {
        let name = body
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .ok_or_else(|| AppError::bad_request("missing required field: name"))?;
        if !valid_suite_name(name) {
            return Err(AppError::bad_request(
                "name must be lowercase kebab-case (letters, digits, dashes), up to 64 chars",
            ));
        }
        let version = body
            .get("version")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| AppError::bad_request("missing required field: version"))?;
        let description = body.get("description").cloned().unwrap_or(Value::Null);
        let en = description.get("en").and_then(Value::as_str).unwrap_or_default().trim();
        if en.is_empty() {
            return Err(AppError::bad_request("description.en must be a non-empty string"));
        }
        let category = body
            .get("category")
            .and_then(Value::as_str)
            .map(str::trim)
            .ok_or_else(|| AppError::bad_request("missing required field: category"))?;
        if !CATEGORY_WHITELIST.contains(&category) {
            return Err(AppError::bad_request(format!(
                "category must be one of: {}",
                CATEGORY_WHITELIST.join(", ")
            )));
        }
        let members = body
            .get("members")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::bad_request("members must be an array of pack names"))?;
        let mut names: Vec<String> = Vec::with_capacity(members.len());
        for m in members {
            let name = m
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::bad_request("members must be an array of pack names"))?;
            if names.iter().any(|n| n == name) {
                return Err(AppError::bad_request(format!("duplicate member: {name}")));
            }
            names.push(name.to_string());
        }
        if names.is_empty() {
            return Err(AppError::bad_request("a suite needs at least one member pack"));
        }
        Ok(SuiteMeta {
            name: name.to_string(),
            version: version.to_string(),
            description: json!({
                "en": en,
                "zh": description.get("zh").and_then(Value::as_str).unwrap_or(""),
            }),
            category: category.to_string(),
            members: names,
        })
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "version": self.version,
            "description": self.description,
            "category": self.category,
            "members": self.members,
        })
    }
}

/// Market rules for suite submissions, independent of HTTP: member packs must
/// exist in `packs` (any status — disabled members stay flagged on the read
/// side, not erased), and the version story mirrors packs (never reuse a
/// submitted pair; edits must strictly increase). `current_version` is the
/// live suite's version when editing, None when creating.
pub(crate) async fn check_suite_rules(
    db: &sqlx::PgPool,
    meta: &SuiteMeta,
    current_version: Option<&str>,
) -> Result<(), AppError> {
    for member in &meta.members {
        let hit: Option<(String,)> =
            sqlx::query_as("select name from packs where name = $1")
                .bind(member)
                .fetch_optional(db)
                .await?;
        if hit.is_none() {
            return Err(AppError::bad_request(format!(
                "member pack {member} does not exist in the market"
            )));
        }
    }
    if let Some(current) = current_version {
        if semver::compare(&meta.version, current) != std::cmp::Ordering::Greater {
            return Err(AppError::conflict(format!(
                "version must strictly increase: {} is already published",
                current
            )));
        }
    }
    let taken: Option<(Uuid,)> = sqlx::query_as(
        "select id from submissions where pack_name = $1 and version = $2",
    )
    .bind(&meta.name)
    .bind(&meta.version)
    .fetch_optional(db)
    .await?;
    if taken.is_some() {
        return Err(AppError::conflict(format!(
            "version {} of this suite was already submitted — bump the version",
            meta.version
        )));
    }
    Ok(())
}

/// The pending-duplicate guard, suite flavor of the pack queue rule.
async fn pending_suite_submission(db: &sqlx::PgPool, name: &str) -> Result<Option<Uuid>, AppError> {
    let pending: Option<(Uuid,)> = sqlx::query_as(
        "select id from submissions where pack_name = $1 and kind = 'suite' and status = 'pending'",
    )
    .bind(name)
    .fetch_optional(db)
    .await?;
    Ok(pending.map(|(id,)| id))
}

async fn enqueue_suite_submission(
    state: &AppState,
    uploader: Uuid,
    meta: &SuiteMeta,
) -> Result<Uuid, AppError> {
    let id = Uuid::new_v4();
    // Suite submissions carry no archive: archive_path stays NULL and the
    // whole form lives in suite_meta for the reviewer to read.
    sqlx::query(
        "insert into submissions (id, pack_name, version, uploader, kind, report, suite_meta) \
         values ($1, $2, $3, $4, 'suite', '{}'::jsonb, $5)",
    )
    .bind(&id)
    .bind(&meta.name)
    .bind(&meta.version)
    .bind(uploader)
    .bind(meta.to_json())
    .execute(&state.db)
    .await?;
    Ok(id)
}

#[derive(FromRow)]
struct SuiteRow {
    name: String,
    version: String,
    description: Value,
    category: String,
    author: String,
    author_id: Uuid,
    members: Value,
    enabled: bool,
    added: DateTime<Utc>,
    updated: DateTime<Utc>,
}

const SUITE_COLUMNS: &str = "name, version, description, category, author, author_id, \
     members, enabled, added, updated";

fn suite_json(r: &SuiteRow) -> Value {
    json!({
        "name": r.name,
        "version": r.version,
        "description": r.description,
        "category": r.category,
        "author": r.author,
        "members": r.members,
        "added": r.added.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "updated": r.updated.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    })
}

// ---------------------------------------------------------------------------
// Publisher surface: /api/my/suites

/// GET /api/my/suites — every suite the caller owns, disabled ones included.
pub async fn my_suites(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    let rows: Vec<SuiteRow> = sqlx::query_as::<_, SuiteRow>(&format!(
        "select {SUITE_COLUMNS} from suites where author_id = $1 order by name"
    ))
    .bind(user.id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(json!({
        "ok": true,
        "suites": rows.iter().map(suite_json).collect::<Vec<_>>(),
    })))
}

/// POST /api/my/suites — submit a new suite (form only). Nothing touches the
/// catalog until an admin approves; only full validation passes enter the
/// queue.
pub async fn create_suite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let (user, _org) = require_org(&state, &headers).await?;
    let meta = SuiteMeta::parse(&body)?;

    let exists: Option<(String,)> =
        sqlx::query_as("select name from suites where name = $1")
            .bind(&meta.name)
            .fetch_optional(&state.db)
            .await?;
    if exists.is_some() {
        return Err(AppError::conflict(format!(
            "a suite named {} already exists",
            meta.name
        )));
    }
    if let Some(id) = pending_suite_submission(&state.db, &meta.name).await? {
        return Err(AppError::conflict(format!(
            "a submission for this suite is already awaiting review (id {id}) — try again later"
        )));
    }
    check_suite_rules(&state.db, &meta, None).await?;

    let id = enqueue_suite_submission(&state, user.id, &meta).await?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

async fn owned_suite(state: &AppState, user_id: Uuid, name: &str) -> Result<SuiteRow, AppError> {
    let row: Option<SuiteRow> = sqlx::query_as::<_, SuiteRow>(&format!(
        "select {SUITE_COLUMNS} from suites where name = $1"
    ))
    .bind(name)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found(format!("no such suite: {name}")))?;
    if row.author_id != user_id {
        return Err(AppError::forbidden("only the suite owner can do that"));
    }
    Ok(row)
}

/// PATCH /api/my/suites/{name} — propose an edit (description, members,
/// version bump). Like pack updates it goes through the queue: the live
/// suite stays untouched until an admin approves.
pub async fn patch_suite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let (user, _org) = require_org(&state, &headers).await?;
    let live = owned_suite(&state, user.id, &name).await?;

    let mut form = body.clone();
    form["name"] = json!(name); // the path decides which suite is edited
    let meta = SuiteMeta::parse(&form)?;
    if let Some(id) = pending_suite_submission(&state.db, &name).await? {
        return Err(AppError::conflict(format!(
            "a submission for this suite is already awaiting review (id {id}) — try again later"
        )));
    }
    check_suite_rules(&state.db, &meta, Some(&live.version)).await?;

    let id = enqueue_suite_submission(&state, user.id, &meta).await?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

/// DELETE /api/my/suites/{name} — two flavors:
///   * never published (no suites row): withdraw the pending submission
///     directly, no review needed;
///   * published: enqueue a deletion submission — the catalog row survives
///     until an admin approves.
pub async fn delete_suite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    let user = require_user(&state, &headers).await?;
    let live: Option<SuiteRow> = sqlx::query_as::<_, SuiteRow>(&format!(
        "select {SUITE_COLUMNS} from suites where name = $1"
    ))
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;

    match live {
        None => {
            // Never published: remove the pending submission, if any.
            let pending = pending_suite_submission(&state.db, &name).await?
                .ok_or_else(|| AppError::not_found(format!("no such suite: {name}")))?;
            let uploader: Option<(Uuid,)> =
                sqlx::query_as("select uploader from submissions where id = $1")
                    .bind(pending)
                    .fetch_optional(&state.db)
                    .await?;
            if uploader.map(|(u,)| u) != Some(user.id) {
                return Err(AppError::forbidden("only the submitter can withdraw"));
            }
            sqlx::query("delete from submissions where id = $1")
                .bind(pending)
                .execute(&state.db)
                .await?;
            Ok(Json(json!({ "ok": true, "mode": "withdrawn" })))
        }
        Some(live) => {
            if live.author_id != user.id {
                return Err(AppError::forbidden("only the suite owner can do that"));
            }
            if pending_suite_submission(&state.db, &name).await?.is_some() {
                return Err(AppError::conflict(
                    "a submission for this suite is already awaiting review — try again later",
                ));
            }
            // A deletion carries a suffixed version: the live pair belongs to
            // whichever submission last published it (history stays intact).
            // Prior rejected deletion attempts on this suite are dropped so
            // the (name, version) unique constraint stays satisfiable.
            sqlx::query(
                "delete from submissions where pack_name = $1 and kind = 'suite' \
                 and status = 'rejected' and suite_meta->>'action' = 'delete'",
            )
            .bind(&name)
            .execute(&state.db)
            .await?;
            let id = Uuid::new_v4();
            sqlx::query(
                "insert into submissions (id, pack_name, version, uploader, kind, report, suite_meta) \
                 values ($1, $2, $3, $4, 'suite', '{}'::jsonb, $5)",
            )
            .bind(&id)
            .bind(&name)
            .bind(format!("{}-delete", live.version))
            .bind(user.id)
            .bind(json!({ "name": name, "action": "delete" }))
            .execute(&state.db)
            .await?;
            Ok(Json(json!({ "ok": true, "mode": "queued", "id": id })))
        }
    }
}

async fn set_enabled(
    state: &Arc<AppState>,
    headers: &HeaderMap,
    name: &str,
    enabled: bool,
) -> Result<Json<Value>, AppError> {
    let user = require_user(state, headers).await?;
    owned_suite(state, user.id, name).await?;
    sqlx::query("update suites set enabled = $2 where name = $1")
        .bind(name)
        .bind(enabled)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true, "name": name, "enabled": enabled })))
}

/// POST /api/my/suites/{name}/disable — hide from every public read surface.
/// Installed member packs are unaffected. Reversible.
pub async fn disable_suite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    set_enabled(&state, &headers, &name, false).await
}

/// POST /api/my/suites/{name}/enable — bring a disabled suite back.
pub async fn enable_suite(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    set_enabled(&state, &headers, &name, true).await
}

// ---------------------------------------------------------------------------
// Public read surface: /api/suites

/// GET /api/suites — the enabled suite list. Disabled suites are absent.
pub async fn list_suites(State(state): State<Arc<AppState>>) -> Result<Json<Value>, AppError> {
    let suites = enabled_suites(&state.db).await?;
    Ok(Json(json!({ "ok": true, "suites": suites })))
}

/// GET /api/suites/{name} — one enabled suite, its members expanded into pack
/// cards. A member missing from the catalog (deleted, or currently disabled)
/// stays on the list, explicitly flagged `missing` — the suite owner decides
/// its fate on the next update, the read surface never rewrites history.
pub async fn suite_detail(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<Value>, AppError> {
    let row: Option<SuiteRow> = sqlx::query_as::<_, SuiteRow>(&format!(
        "select {SUITE_COLUMNS} from suites where name = $1 and enabled"
    ))
    .bind(&name)
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found(format!("no such suite: {name}")))?;

    let members = row
        .members
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| m.as_str().map(str::to_string))
        .collect::<Vec<_>>();
    let mut cards = Vec::with_capacity(members.len());
    for member in &members {
        let pack: Option<(String, String, Value, Value)> = sqlx::query_as(
            "select name, version, description, consumes from packs \
             where name = $1 and status = 'active'",
        )
        .bind(member)
        .fetch_optional(&state.db)
        .await?;
        match pack {
            Some((name, version, description, consumes)) => cards.push(json!({
                "name": name,
                "version": version,
                "description": description,
                "consumes": consumes,
                "missing": false,
            })),
            None => cards.push(json!({
                "name": member,
                "missing": true,
            })),
        }
    }

    Ok(Json(json!({
        "ok": true,
        "suite": suite_json(&row),
        "members": cards,
    })))
}

/// Every enabled suite as list records — shared by /api/suites and the
/// /api/index `suites` array (same field structure per the design).
pub(crate) async fn enabled_suites(db: &sqlx::PgPool) -> Result<Vec<Value>, AppError> {
    let rows: Vec<SuiteRow> = sqlx::query_as::<_, SuiteRow>(&format!(
        "select {SUITE_COLUMNS} from suites where enabled order by name"
    ))
    .fetch_all(db)
    .await?;
    Ok(rows.iter().map(suite_json).collect())
}

/// Suite membership map for pack cards: pack name → names of the enabled
/// suites containing it. The catalog is small; one full scan beats a join on
/// a jsonb containment index.
pub(crate) async fn membership_map(
    db: &sqlx::PgPool,
) -> Result<BTreeMap<String, Vec<String>>, AppError> {
    let rows: Vec<(String, Value)> =
        sqlx::query_as("select name, members from suites where enabled order by name")
            .fetch_all(db)
            .await?;
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (suite, members) in rows {
        for member in members.as_array().cloned().unwrap_or_default() {
            if let Some(pack) = member.as_str() {
                map.entry(pack.to_string()).or_default().push(suite.clone());
            }
        }
    }
    Ok(map)
}
