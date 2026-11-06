# docwright pack market — verifier sidecar image.
#
# No in-image build: the context carries the host's proven docwright checkout
# (node_modules, lib, packs, scripts — the exact tree pack:check runs against
# on the dev machine) plus only the harness packages its node_modules symlinks
# into, at the same relative layout (/app/deepseek-harness + /app/docwright).
# pnpm is installed via npm because corepack's own downloader cannot reach the
# network directly (mirrors are baked into npm only).
#
# node22-dev provides node + npm; the app user (uid 1000) matches the host
# uid so the copied tree stays readable.
ARG BASE_IMAGE=crpi-nt9q5pom2saqwub9.cn-zhangjiakou.personal.cr.aliyuncs.com/sharework/node22-dev:1.1
FROM ${BASE_IMAGE}

USER root
COPY deepseek-harness /app/deepseek-harness
COPY docwright /app/docwright
COPY verifier /app/verifier
# /app/docwright must be writable by the runtime user: tsx writes loader
# temp files into the project root when pack:check runs.
RUN npm install -g pnpm@12.11.2 \
 && chown -R app:app /app/docwright

USER app
ENV DOCWRIGHT_ROOT=/app/docwright \
    VERIFIER_PORT=8399 \
    VERIFIER_TMP=/tmp/verifier
EXPOSE 8399
WORKDIR /app/docwright
CMD ["node", "/app/verifier/index.mjs"]
