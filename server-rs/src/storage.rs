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
