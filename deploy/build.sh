#!/usr/bin/env bash
# Build the market server image for the local k3s.
#
# server: multi-stage podman build (rust-dev compiles, base ships the binary
#         + web pages); the host cargo registry + target dirs are mounted so
#         locked deps and release artifacts come from local caches.
# verifier: NO image at all. The cluster deployment hostPath-mounts the
#         host's docwright + deepseek-harness + verifier checkouts into a
#         plain node22-dev container (see deploy/k8s.yaml) — module
#         resolution is 1:1 with the machine where pack:check is proven.
#         (A former market-verifier image copied ~2G of harness into the
#         build context and materialized pnpm's circular symlinks with
#         cp -rL, which quietly ate hundreds of GB before dying — do not
#         bring that path back.)
# The image is imported into the k3s containerd store (podman's and k3s's
# stores are separate).
#
# Usage: deploy/build.sh [--tag <tag>]     (default tag: HHMM)
set -euo pipefail

M="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEVCONTAINER="${DEVCONTAINER_DIR:-$HOME/projects/devcontainer}"
TAG="$(date +%H%M)"
if [ "${1:-}" = "--tag" ]; then TAG="${2:?tag required}"; fi

need_base() {
  podman image exists "localhost/$1:$2" || {
    (cd "$DEVCONTAINER" && "./build.sh" "$1")
    podman tag "crpi-nt9q5pom2saqwub9.cn-zhangjiakou.personal.cr.aliyuncs.com/sharework/$1:$2" "localhost/$1:$2"
  }
}
need_base rust-dev 1.3
need_base base 1.1

CONTEXT="$(mktemp -d "$HOME/devcache/image-exports/market-context-XXXXXX")"
trap 'rm -rf "$CONTEXT"' EXIT

echo "== assembling context =="
mkdir -p "$CONTEXT/server-rs"
cp -r "$M/server-rs/src" "$M/server-rs/Cargo.toml" "$M/server-rs/Cargo.lock" "$CONTEXT/server-rs/"
cp -r "$M/web" "$CONTEXT/web"

echo "== building localhost/market-server:$TAG =="
podman build \
  --volume "$HOME/devcache/cargo/registry":/home/app/.cargo/registry \
  --volume "$HOME/devcache/cargo/target-market":/build/target \
  --build-arg BASE_IMAGE=localhost/rust-dev:1.3 \
  --build-arg RUNTIME_IMAGE=localhost/base:1.1 \
  -t "localhost/market-server:$TAG" -f "$M/deploy/server.Dockerfile" "$CONTEXT"

echo "== importing into k3s containerd =="
tarfile="$CONTEXT/import-market-server.tar"
podman save --format oci-archive -o "$tarfile" "localhost/market-server:$TAG"
echo justdid1t | sudo -S k3s ctr images import "$tarfile" >/dev/null
rm -f "$tarfile"
echo "imported localhost/market-server:$TAG"

echo "done. Deploy with: deploy/deploy.sh --tag $TAG"
