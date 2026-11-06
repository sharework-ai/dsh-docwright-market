//! Supabase Storage REST access with the server-side secret key.
//! Buckets: `uploads` (private, raw submission archive) and `tarballs`
//! (public read, install-time downloads) — created by the market migration.

use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::state::AppState;

pub const BUCKET_UPLOADS: &str = "uploads";
pub const BUCKET_TARBALLS: &str = "tarballs";

pub async fn upload(
    state: &AppState,
    bucket: &str,
    object_path: &str,
    bytes: Vec<u8>,
) -> Result<(), AppError> {
    let url = format!(
        "{}/storage/v1/object/{}/{}",
        state.cfg.supabase_api_url, bucket, object_path
    );
    let response = state
        .http
        .post(url)
        .header("authorization", format!("Bearer {}", state.cfg.supabase_secret_key))
        .header("x-upsert", "true")
        .header("content-type", "application/octet-stream")
        .body(bytes)
        .send()
        .await?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        let body = response.text().await.unwrap_or_default();
        Err(AppError::internal(format!(
            "storage upload {bucket}/{object_path} -> {status}: {body}"
        )))
    }
}

/// Public-object fetch (no auth header) for proxying downloads.
pub async fn download_public(
    state: &AppState,
    bucket: &str,
    object_path: &str,
) -> Result<Vec<u8>, AppError> {
    let url = format!(
        "{}/storage/v1/object/public/{}/{}",
        state.cfg.supabase_api_url, bucket, object_path
    );
    let response = state.http.get(url).send().await?;
    let status = response.status();
    if status.is_success() {
        Ok(response.bytes().await?.to_vec())
    } else {
        Err(AppError::not_found(format!(
            "storage object {bucket}/{object_path} -> {status}"
        )))
    }
}

pub async fn download(
    state: &AppState,
    bucket: &str,
    object_path: &str,
) -> Result<Vec<u8>, AppError> {
    let url = format!(
        "{}/storage/v1/object/authenticated/{}/{}",
        state.cfg.supabase_api_url, bucket, object_path
    );
    let response = state
        .http
        .get(url)
        .header("authorization", format!("Bearer {}", state.cfg.supabase_secret_key))
        .send()
        .await?;
    let status = response.status();
    if status.is_success() {
        Ok(response.bytes().await?.to_vec())
    } else {
        Err(AppError::not_found(format!(
            "storage object {bucket}/{object_path} -> {status}"
        )))
    }
}

/// Public read URL of a tarball (the bucket is public; install.mjs follows it).
pub fn public_url(state: &AppState, object_path: &str) -> String {
    format!(
        "{}/storage/v1/object/public/{}/{}",
        state.cfg.supabase_api_url, BUCKET_TARBALLS, object_path
    )
}

/// Prefixed ("sha256:…") so it matches the verifier's contentHash format.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Idempotently make sure the market buckets exist. Called at boot so a fresh
/// deployment works from `0001_init.sql` alone (bucket creation needs the
/// Storage API, not plain SQL — plain-Postgres test databases have no
/// storage schema). "Already exists" is success.
pub async fn ensure_buckets(state: &AppState) -> Result<(), AppError> {
    for (bucket, public) in [(BUCKET_UPLOADS, false), (BUCKET_TARBALLS, true)] {
        let url = format!("{}/storage/v1/bucket", state.cfg.supabase_api_url);
        let response = state
            .http
            .post(url)
            .header("authorization", format!("Bearer {}", state.cfg.supabase_secret_key))
            .json(&serde_json::json!({ "name": bucket, "public": public }))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        // "Already exists" surfaces as HTTP 400 with an inner 409 code, or
        // 409/200 depending on the Storage build — accept any of those.
        let ok = status.is_success()
            || body.contains("BucketAlreadyExists")
            || body.contains("Duplicate")
            || status == reqwest::StatusCode::CONFLICT;
        if !ok {
            return Err(AppError::internal(format!(
                "cannot ensure storage bucket {bucket} -> {status}: {body}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
