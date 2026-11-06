#!/usr/bin/env bash
# Local dev runner: pg port-forward + verifier sidecar + the Rust market service.
#
# Prereqs (see README):
#   - commons/supabase stack running on the local k3s (../..? ~/projects/commons/supabase)
#   - docwright checkout with ../deepseek-harness built
# Usage: scripts/dev-run.sh          (Ctrl-C stops everything)
set -euo pipefail

M="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DW="${DOCWRIGHT_ROOT:-$HOME/projects/lab/docwright}"
SUPABASE_DIR="${SUPABASE_DIR:-$HOME/projects/commons/supabase}"
NS=supabase

command -v kubectl >/dev/null || { echo "kubectl required" >&2; exit 1; }
[ -d "$DW/packs" ] || { echo "DOCWRIGHT_ROOT ($DW) missing packs/" >&2; exit 1; }

# --- credentials straight from the cluster secrets ---------------------------
get_secret() {
  kubectl -n "$NS" get secret "$1" -o "jsonpath={.data.$2}" | base64 -d
}
DB_PASSWORD="$(get_secret supabase-postgres-auth password)"
SECRET_KEY="$(get_secret supabase-jwt service-key)"
PUBLISHABLE_KEY="$(get_secret supabase-jwt publishable-key)"

# --- pg port-forward (the DB is ClusterIP-only) ------------------------------
PF_PORT="${PF_PORT:-15432}"
kubectl -n "$NS" port-forward svc/supabase-postgres "${PF_PORT}:5432" >/dev/null 2>&1 &
PF_PID=$!
trap 'kill "$PF_PID" "$VERIFIER_PID" "$SERVER_PID" 2>/dev/null || true' EXIT
for _ in $(seq 1 20); do
  (exec 3<>"/dev/tcp/127.0.0.1/$PF_PORT") 2>/dev/null && { exec 3>&- 3<&-; break; }
  sleep 0.5
done
echo "pg forwarded on 127.0.0.1:${PF_PORT}"

# --- verifier sidecar --------------------------------------------------------
VERIFIER_PORT=8399 DOCWRIGHT_ROOT="$DW" node "$M/verifier/index.mjs" &
VERIFIER_PID=$!

# --- build + run the market service ------------------------------------------
echo "building server-rs (rust-dev container)..."
IMG=crpi-nt9q5pom2saqwub9.cn-zhangjiakou.personal.cr.aliyuncs.com/sharework/rust-dev:1.2
podman run --rm --userns=keep-id -v "$M":/workspace -w /workspace/server-rs \
  -v "$HOME/devcache/cargo/registry":/home/app/.cargo/registry \
  "$IMG" cargo build

PORT=8398 \
SUPABASE_API_URL="http://supabase.lan" \
SUPABASE_SECRET_KEY="$SECRET_KEY" \
SUPABASE_PUBLISHABLE_KEY="$PUBLISHABLE_KEY" \
DATABASE_URL="postgres://postgres:${DB_PASSWORD}@127.0.0.1:${PF_PORT}/postgres" \
VERIFIER_URL="http://127.0.0.1:8399" \
WEB_DIR="$M/web" \
DATA_DIR="$M/data" \
"$M/server-rs/target/debug/docwright-market" &
SERVER_PID=$!

echo
echo "market:  http://127.0.0.1:8398   (browse / submit.html / admin.html)"
echo "stop with Ctrl-C"
wait
