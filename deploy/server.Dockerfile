# docwright pack market — Rust service image.
# Build via deploy/build.sh (assembles the build context from sibling repos
# and mounts the host cargo cache into the builder for speed).
#
# Runtime is the shared `base` bottom layer (non-root app user, uid 1000, same
# convention as the devcontainer toolchain images).
ARG BASE_IMAGE=crpi-nt9q5pom2saqwub9.cn-zhangjiakou.personal.cr.aliyuncs.com/sharework/rust-dev:1.3
ARG RUNTIME_IMAGE=crpi-nt9q5pom2saqwub9.cn-zhangjiakou.personal.cr.aliyuncs.com/sharework/base:1.1

FROM ${BASE_IMAGE} AS build
USER root
COPY server-rs /build/server-rs
WORKDIR /build/server-rs
ENV CARGO_TARGET_DIR=/build/target
RUN cargo build --release

FROM ${RUNTIME_IMAGE}
COPY --from=build /build/target/release/docwright-market /app/docwright-market
COPY web /app/web
USER app
WORKDIR /app
ENV LISTEN_HOST=0.0.0.0 \
    PORT=8398 \
    WEB_DIR=/app/web \
    DATA_DIR=/tmp/market-data
EXPOSE 8398
ENTRYPOINT ["/app/docwright-market"]
