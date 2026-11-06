#!/usr/bin/env bash
# Test + coverage for both layers:
#   verifier (node):   vitest with v8 coverage gates (85/85/85/75) over
#                      verifier/index.mjs — the validation surface.
#   server-rs (rust):  cargo test inside the rust-dev container (no local
#                      toolchain, per the devcontainer discipline). The
#                      sqlx::test integration cases need Postgres: the script
#                      port-forwards the local supabase pg and hands the
#                      container the host-mapped URL.
# Flags: --node-only | --rust-only to scope; --cov adds a Rust coverage
# summary (cargo-llvm-cov, requires rust-dev:1.3+).
set -euo pipefail

M="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMG="${RUST_DEV_IMAGE:-localhost/rust-dev:1.3}"
NODE_ONLY=false RUST_ONLY=false RUST_COV=false
for a in "$@"; do
  case "$a" in
    --node-only) NODE_ONLY=true ;;
    --rust-only) RUST_ONLY=true ;;
    --cov) RUST_COV=true ;;
    *) echo "unknown flag: $a" >&2; exit 1 ;;
  esac
done

run_rust() {
  local extra="$1" cmd="$2"
  # --network host: DB-backed tests reach the host-loopback port-forward.
  podman run --rm --userns=keep-id --network host \
    -v "$M":/workspace -w /workspace/server-rs \
    -v "$HOME/devcache/cargo/registry":/home/app/.cargo/registry \
    $extra "$IMG" sh -c "$cmd"
}

if ! $RUST_ONLY; then
  echo "== verifier (vitest + coverage gates) =="
  (cd "$M" && npx vitest run --coverage \
    --coverage.all=false --coverage.include='verifier/index.mjs' \
    --coverage.thresholds.lines=85 --coverage.thresholds.functions=85 \
    --coverage.thresholds.statements=85 --coverage.thresholds.branches=75)
fi

if ! $NODE_ONLY; then
  echo "== server-rs (cargo test) =="
  if command -v kubectl >/dev/null 2>&1 \
    && kubectl -n supabase get svc supabase-postgres >/dev/null 2>&1; then
    # Scratch DBs for sqlx::test, via a host port-forward. The test container
    # runs with --network host, so it shares the host loopback where kubectl
    # binds (no podman-gateway dance: host.containers.internal maps to the
    # container netns, not a host address).
    PF_PORT="${TEST_PF_PORT:-15435}"
    kubectl -n supabase port-forward svc/supabase-postgres "${PF_PORT}:5432" >/dev/null 2>&1 &
    PF_PID=$!
    trap 'kill "$PF_PID" 2>/dev/null || true' EXIT
    for _ in $(seq 1 30); do
      (exec 3<>"/dev/tcp/127.0.0.1/$PF_PORT") 2>/dev/null && { exec 3>&- 3<&-; break; }
      sleep 0.5
    done
    DB_PASSWORD="$(kubectl -n supabase get secret supabase-postgres-auth \
      -o jsonpath='{.data.password}' | base64 -d)"
    TEST_DB="postgres://postgres:${DB_PASSWORD}@127.0.0.1:${PF_PORT}/postgres"
    echo "   (scratch DBs on the supabase postgres via 127.0.0.1:${PF_PORT})"
    run_rust "-e DATABASE_URL=$TEST_DB" 'cargo test'
    if $RUST_COV; then
      echo "== server-rs (cargo-llvm-cov) =="
      run_rust "-e DATABASE_URL=$TEST_DB" 'cargo llvm-cov --summary-only'
    fi
  else
    echo "   (supabase pg not found — skipping DB-backed cases)" >&2
    run_rust '' 'cargo test'
  fi
fi

echo "all tests passed"
