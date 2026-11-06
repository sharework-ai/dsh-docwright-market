#!/usr/bin/env bash
# Build the market images (Rust server + verifier sidecar) for the local k3s.
#
# server: multi-stage podman build (rust-dev compiles, base ships the binary
#         + web pages), host cargo cache mounted for speed.
# verifier: NO in-image build. The context carries the host's proven
#         docwright checkout (node_modules, lib, packs, scripts — the exact
#         tree pack:check runs against on the dev machine) plus a COMPLETE
#         copy of deepseek-harness, whose packages/vendor trees host the
#         internal imports (@deepseek-ai/cosmokit and friends resolve through
#         per-package node_modules links). Harness symlinks are materialized
#         in the context (podman COPY proved lossy on nested dir symlinks);
#         links escaping the context (host .pnpm devDeps) are dropped.
# Both images are imported into the k3s containerd store (podman's and k3s's
# stores are separate).
#
# Usage: deploy/build.sh [--tag <tag>]     (default tag: HHMM)
set -euo pipefail

M="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DOCWRIGHT="${DOCWRIGHT_DIR:-$HOME/projects/lab/docwright}"
HARNESS="${HARNESS_DIR:-$HOME/projects/lab/deepseek-harness}"
DEVCONTAINER="${DEVCONTAINER_DIR:-$HOME/projects/devcontainer}"
TAG="$(date +%H%M)"
if [ "${1:-}" = "--tag" ]; then TAG="${2:?tag required}"; fi

[ -f "$DOCWRIGHT/package.json" ] || { echo "docwright not found at $DOCWRIGHT" >&2; exit 1; }
[ -d "$HARNESS/packages" ] || { echo "deepseek-harness not found at $HARNESS" >&2; exit 1; }

need_base() {
  podman image exists "localhost/$1:$2" || {
    (cd "$DEVCONTAINER" && "./build.sh" "$1")
    podman tag "crpi-nt9q5pom2saqwub9.cn-zhangjiakou.personal.cr.aliyuncs.com/sharework/$1:$2" "localhost/$1:$2"
  }
}
need_base rust-dev 1.3
need_base node22-dev 1.1
need_base base 1.1

CONTEXT="$(mktemp -d "$HOME/devcache/image-exports/market-context-XXXXXX")"
trap 'rm -rf "$CONTEXT"' EXIT

echo "== assembling context =="
mkdir -p "$CONTEXT/deepseek-harness" "$CONTEXT/docwright" "$CONTEXT/server-rs"

# Harness: complete 1:1 copy of the tree pack:check's imports resolve against
# on the dev machine (~2G with node_modules — deterministic over clever).
rsync -a --exclude .git "$HARNESS/" "$CONTEXT/deepseek-harness/"

# Materialize every harness symlink whose target lives inside the context
# (podman COPY proved lossy on nested directory symlinks); drop links
# escaping to the host (.pnpm devDeps — pack:check imports nothing from them).
LINKS="$(mktemp "$HOME/devcache/image-exports/links-XXXXXX")"
find "$CONTEXT/deepseek-harness" -type l > "$LINKS"
while IFS= read -r link; do
  target="$(readlink -f "$link")"
  case "$target" in
    "$CONTEXT"/*)
      rm -rf "$link"
      # pnpm packages cross-reference each other through circular links;
      # cp refuses to recurse into a cycle, which is fine — drop and move on.
      cp -rL "$target" "$link" 2>/dev/null || rm -rf "$link"
      ;;
    *) rm -f "$link" ;;
  esac
done < "$LINKS"
rm -f "$LINKS"

# docwright as-is (node_modules included — its pnpm layout is what pack:check
# runs against; lib/ packs/ scripts/ src/ all proven on this host).
rsync -a \
  --exclude .git --exclude '/target/' --exclude '/dist/' --exclude '/client/' \
  "$DOCWRIGHT/" "$CONTEXT/docwright/"

cp -r "$M/server-rs/src" "$M/server-rs/Cargo.toml" "$M/server-rs/Cargo.lock" "$CONTEXT/server-rs/"
cp -r "$M/web" "$CONTEXT/web"
cp -r "$M/verifier" "$CONTEXT/verifier"

echo "== building localhost/market-server:$TAG =="
podman build \
  --volume "$HOME/devcache/cargo/registry":/home/app/.cargo/registry \
  --build-arg BASE_IMAGE=localhost/rust-dev:1.3 \
  --build-arg RUNTIME_IMAGE=localhost/base:1.1 \
  -t "localhost/market-server:$TAG" -f "$M/deploy/server.Dockerfile" "$CONTEXT"

echo "== building localhost/market-verifier:$TAG =="
podman build \
  --build-arg BASE_IMAGE=localhost/node22-dev:1.1 \
  -t "localhost/market-verifier:$TAG" -f "$M/deploy/verifier.Dockerfile" "$CONTEXT"

echo "== importing into k3s containerd =="
for img in "localhost/market-server:$TAG" "localhost/market-verifier:$TAG"; do
  safe="$(echo "$img" | tr '/:' '__')"
  tarfile="$CONTEXT/import-$safe.tar"
  podman save --format oci-archive -o "$tarfile" "$img"
  echo justdid1t | sudo -S k3s ctr images import "$tarfile" >/dev/null
  rm -f "$tarfile"
  echo "imported $img"
done

echo "done. Deploy with: deploy/deploy.sh --tag $TAG"
