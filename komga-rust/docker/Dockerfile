# syntax=docker/dockerfile:1.7

ARG BUILDPLATFORM
ARG BUILDOS
ARG BUILDARCH
ARG BUILDVARIANT
ARG TARGETPLATFORM
ARG TARGETOS
ARG TARGETARCH
ARG TARGETVARIANT

FROM --platform=$BUILDPLATFORM node:18-alpine AS webui-builder
ARG BUILDOS
ARG BUILDARCH
ARG BUILDVARIANT
WORKDIR /workspace/komga-webui
COPY komga-webui/package.json komga-webui/package-lock.json ./
RUN --mount=type=cache,id=komga-webui-npm-${BUILDOS}-${BUILDARCH}${BUILDVARIANT},target=/root/.npm npm ci --no-audit --no-fund
COPY komga-webui ./
RUN --mount=type=cache,id=komga-webui-npm-${BUILDOS}-${BUILDARCH}${BUILDVARIANT},target=/root/.npm npm run build

# TODO: cross compile
FROM --platform=$TARGETPLATFORM rust:1-alpine AS rust-builder
ARG TARGETOS
ARG TARGETARCH
ARG TARGETVARIANT
WORKDIR /workspace
COPY komga-rust ./komga-rust
COPY --from=webui-builder /workspace/komga-webui/dist ./komga-webui/dist
RUN --mount=type=cache,id=komga-rust-cargo-registry-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},target=/usr/local/cargo/registry \
    --mount=type=cache,id=komga-rust-cargo-git-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},target=/usr/local/cargo/git \
    --mount=type=cache,id=komga-rust-target-${TARGETOS}-${TARGETARCH}${TARGETVARIANT},target=/workspace/komga-rust/target \
    cargo build --manifest-path /workspace/komga-rust/Cargo.toml --locked --release --bin komga-rust

FROM --platform=$TARGETPLATFORM alpine:3.23 AS runner
WORKDIR /app
COPY --from=rust-builder /workspace/komga-rust/target/release/komga-rust ./komga-rust
VOLUME /config
ENV KOMGA_CONFIG_DIR="/config"
ENV KOMGA_RUST_MODE="localdb"
ENV KOMGA_RUST_PLATFORM_PROFILE="docker"
ENV LOGGING_FILE_NAME="/config/logs/komga.log"
ENV LANG='C.UTF-8' LC_ALL='C.UTF-8'
ENTRYPOINT ["./komga-rust"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 CMD ["wget", "-qO-", "http://127.0.0.1:25600/health/ready"]
EXPOSE 25600
LABEL org.opencontainers.image.source="https://github.com/huihuimoe/komga-riir"
