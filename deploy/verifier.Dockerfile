# docwright pack market — verifier sidecar image.
#
# No in-image build: the context carries the host's proven docwright checkout
# (node_modules, lib, packs, scripts — the exact tree pack:check runs against
# on the dev machine) plus only the harness packages its node_modules symlinks
# into, at the same relative layout (/app/deepseek-harness + /app/docwright).
# pnpm is installed via npm because corepack's own downloader cannot reach the
# network directly (mirrors are baked into npm only).
#
# Layer order = stability order: the pnpm install (needs the network) comes
# first and caches until the base image changes; then the least-volatile
# trees — harness, docwright — and finally verifier.
#
# node22-dev provides node + npm; the app user (uid 1000) matches the host
# uid, so context files rsync'd from the host are already app-owned and no
# chown pass is needed.
ARG BASE_IMAGE=crpi-nt9q5pom2saqwub9.cn-zhangjiakou.personal.cr.aliyuncs.com/sharework/node22-dev:1.1
FROM ${BASE_IMAGE}

USER root
RUN npm install -g pnpm@12.11.2
COPY deepseek-harness /app/deepseek-harness
COPY docwright /app/docwright
COPY verifier /app/verifier

USER app
ENV DOCWRIGHT_ROOT=/app/docwright \
    VERIFIER_PORT=8399 \
    VERIFIER_TMP=/tmp/verifier
EXPOSE 8399
WORKDIR /app/docwright
CMD ["node", "/app/verifier/index.mjs"]
