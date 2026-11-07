//! Runtime configuration from environment variables. Misconfiguration fails
//! loud at startup — no defaults that mask a wrong deployment.

use std::net::SocketAddr;

#[derive(Clone, Debug)]
pub struct Config {
    pub listen: SocketAddr,
    /// Supabase API gateway (envoy), e.g. http://supabase.lan — Auth/Storage REST.
    pub supabase_api_url: String,
    /// Server-side credential, never sent to browsers. Use the legacy
    /// service_role JWT (`supabase-jwt/service-key`): Storage still authenticates
    /// with the old JWT shape, while the sb_secret_ key is gateway-only.
    pub supabase_secret_key: String,
    /// Publishable key exposed to the web UI via /config.json.
    pub supabase_publishable_key: String,
    /// Browser-facing Supabase URL (public domain, e.g. https://sb.shrwk.com).
    /// The server itself talks to SUPABASE_API_URL over the cluster-internal
    /// svc — browsers can never resolve those names.
    pub supabase_public_url: String,
    /// Direct Postgres connection for sqlx (cluster-internal or port-forwarded).
    pub database_url: String,
    /// Verification sidecar base URL (POST /verify).
    pub verifier_url: String,
    /// Directory containing the static web pages.
    pub web_dir: String,
    /// Local work dir for staging archives during review.
    pub data_dir: String,
    /// Public origin of this market service (index.json `origin`, page URLs).
    pub public_origin: String,
    /// Source repository URL (index.json `source`).
    pub source_url: String,
}

fn env(name: &str) -> String {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => v,
        _ => panic!("missing required env var {name}"),
    }
}

fn env_or(name: &str, default: &str) -> String {
    match std::env::var(name) {
        Ok(v) if !v.trim().is_empty() => v,
        _ => default.to_string(),
    }
}

impl Config {
    pub fn from_env() -> Self {
        let port: u16 = env_or("PORT", "8398").parse().expect("PORT must be a port number");
        // Loopback by default; LAN-facing deployments set LISTEN_HOST=192.168.2.1
        // (k3s traefik routes dm.shrwk.com here) or 0.0.0.0.
        let host = env_or("LISTEN_HOST", "127.0.0.1");
        let addr = if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            SocketAddr::from((ip, port))
        } else {
            SocketAddr::from(([127, 0, 0, 1], port))
        };
        Self {
            listen: addr,
            supabase_api_url: env("SUPABASE_API_URL").trim_end_matches('/').to_string(),
            supabase_secret_key: env("SUPABASE_SECRET_KEY"),
            supabase_publishable_key: env("SUPABASE_PUBLISHABLE_KEY"),
            supabase_public_url: env("PUBLIC_SUPABASE_URL").trim_end_matches('/').to_string(),
            database_url: env("DATABASE_URL"),
            verifier_url: env_or("VERIFIER_URL", "http://127.0.0.1:8399")
                .trim_end_matches('/')
                .to_string(),
            web_dir: env_or("WEB_DIR", "web"),
            data_dir: env_or("DATA_DIR", "data"),
            public_origin: env_or("PUBLIC_ORIGIN", "http://localhost:8398")
                .trim_end_matches('/')
                .to_string(),
            source_url: env_or(
                "SOURCE_URL",
                "https://github.com/sharework-ai/dsh-docwright-market",
            ),
        }
    }
}

pub const CATEGORY_WHITELIST: [&str; 6] = ["estimation", "dev", "pm", "test", "docs", "other"];
pub const LICENSE_WHITELIST: [&str; 5] = ["MIT", "Apache-2.0", "BSD-3-Clause", "BSD-2-Clause", "ISC"];
/// Same cap as the verifier's VERIFIER_MAX_BYTES default.
pub const MAX_ARCHIVE_BYTES: usize = 50 * 1024 * 1024;
