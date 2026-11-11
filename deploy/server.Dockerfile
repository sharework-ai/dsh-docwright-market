# docwright pack market — Rust service image.
# Build via deploy/build.sh (assembles the build context from sibling repos
# and mounts the host cargo caches into the builder for speed).
#
# Runtime is the shared `base` bottom layer (non-root app user, uid 1000, same
# convention as the devcontainer toolchain images).
ARG BASE_IMAGE=crpi-nt9q5pom2saqwub9.cn-zhangjiakou.personal.cr.aliyuncs.com/sharework/rust-dev:1.3
ARG RUNTIME_IMAGE=crpi-nt9q5pom2saqwub9.cn-zhangjiakou.personal.cr.aliyuncs.com/sharework/base:1.1

FROM ${BASE_IMAGE} AS build
USER root
COPY server-rs /build/server-rs
WORKDIR /build/server-rs
# /build/target is volume-mounted by build.sh (persistent incremental cache).
# Cargo reads the mounted registry cache first and only reaches the network
# for crates it is missing — no --offline here on purpose: a builder without
# a warm cache must still work. 8 parallel rustc units keep the memory
# footprint sane on the 20G dev machine.
ENV CARGO_TARGET_DIR=/build/target \
    CARGO_BUILD_JOBS=8
RUN cargo build --release
# Build-time volume content is NOT captured into image layers, so the final
# stage cannot COPY --from the mounted target dir — materialize the binary
# into the layer first.
RUN cp /build/target/release/docwright-market /out-docwright-market

FROM ${RUNTIME_IMAGE}
COPY --from=build /out-docwright-market /app/docwright-market
COPY web /app/web
# The runtime user is non-root (uid 1000): files copied with restrictive host
# modes (e.g. a 600 file from a private edit) would 404 in ServeDir. Normalize.
USER root
RUN chmod -R a+rX /app/web
USER app
WORKDIR /app
ENV LISTEN_HOST=0.0.0.0 \
    PORT=8398 \
    WEB_DIR=/app/web
EXPOSE 8398
ENTRYPOINT ["/app/docwright-market"]
