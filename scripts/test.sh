#!/usr/bin/env bash
# Test + coverage for both layers:
#   verifier (node):   vitest with v8 coverage gates (85/85/85/75) over
#                      verifier/index.mjs — the validation surface.
#   server-rs (rust):  cargo test inside the rust-dev container (no local
#                      toolchain, per the devcontainer discipline).
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
  podman run --rm --userns=keep-id -v "$M":/workspace -w /workspace/server-rs \
    -v "$HOME/devcache/cargo/registry":/home/app/.cargo/registry \
    "$IMG" sh -c "$1"
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
  run_rust 'cargo test'
  if $RUST_COV; then
    echo "== server-rs (cargo-llvm-cov) =="
    run_rust 'cargo llvm-cov --summary-only'
  fi
fi

echo "all tests passed"
