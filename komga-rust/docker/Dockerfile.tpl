FROM rust:1.85-bookworm AS builder
ARG ARCHIVE={{distributionArtifactFile}}
WORKDIR /builder
COPY assembly/${ARCHIVE} /tmp/komga-rust-docker-context.tar.gz
RUN mkdir source && tar -xzf /tmp/komga-rust-docker-context.tar.gz -C source
WORKDIR /builder/source/komga-rust
RUN cargo build --release --bin komga-rust

# amd64 builder
FROM ubuntu:24.10 AS build-amd64
RUN sed -i -re 's/([a-z]{2}\.)?archive.ubuntu.com|security.ubuntu.com/old-releases.ubuntu.com/g' /etc/apt/sources.list.d/ubuntu.sources && \
    apt -y update && \
    apt -y install ca-certificates locales libjxl-dev libheif-dev libwebp-dev libarchive-dev wget curl && \
    echo "en_US.UTF-8 UTF-8" >> /etc/locale.gen && \
    locale-gen en_US.UTF-8 && \
    wget "https://github.com/pgaskin/kepubify/releases/latest/download/kepubify-linux-64bit" -O /usr/bin/kepubify && \
    chmod +x /usr/bin/kepubify && \
    apt -y autoremove && rm -rf /var/lib/apt/lists/*
ENV LD_LIBRARY_PATH="${LD_LIBRARY_PATH}:/usr/lib/x86_64-linux-gnu"

# arm64 builder
FROM ubuntu:24.10 AS build-arm64
RUN sed -i -re 's/([a-z]{2}\.)?ports.ubuntu.com\/ubuntu-ports/old-releases.ubuntu.com\/ubuntu/g' /etc/apt/sources.list.d/ubuntu.sources && \
    apt -y update && \
    apt -y install ca-certificates locales libjxl-dev libheif-dev libwebp-dev libarchive-dev wget curl && \
    echo "en_US.UTF-8 UTF-8" >> /etc/locale.gen && \
    locale-gen en_US.UTF-8 && \
    wget "https://github.com/pgaskin/kepubify/releases/latest/download/kepubify-linux-arm64" -O /usr/bin/kepubify && \
    chmod +x /usr/bin/kepubify && \
    apt -y autoremove && rm -rf /var/lib/apt/lists/*
ENV LD_LIBRARY_PATH="${LD_LIBRARY_PATH}:/usr/lib/aarch64-linux-gnu"

# arm builder
FROM ubuntu:24.10 AS build-arm
RUN sed -i -re 's/([a-z]{2}\.)?ports.ubuntu.com\/ubuntu-ports/old-releases.ubuntu.com\/ubuntu/g' /etc/apt/sources.list.d/ubuntu.sources && \
    apt -y update && \
    apt -y install ca-certificates locales libjxl-dev libheif-dev libwebp-dev libarchive-dev wget curl && \
    echo "en_US.UTF-8 UTF-8" >> /etc/locale.gen && \
    locale-gen en_US.UTF-8 && \
    wget "https://github.com/pgaskin/kepubify/releases/latest/download/kepubify-linux-arm" -O /usr/bin/kepubify && \
    chmod +x /usr/bin/kepubify && \
    apt -y autoremove && rm -rf /var/lib/apt/lists/*

FROM build-${TARGETARCH} AS runner
VOLUME /config
WORKDIR /app
COPY --from=builder /builder/source/komga-rust/target/release/komga-rust ./komga-rust
ENV KOMGA_CONFIGDIR="/config"
ENV KOMGA_CONFIG_DIR="/config"
ENV KOMGA_RUST_MODE="localdb"
ENV KOMGA_RUST_PLATFORM_PROFILE="docker"
ENV LOGGING_FILE_NAME="/config/logs/komga.log"
ENV KOMGA_KEPUBIFY_PATH="/usr/bin/kepubify"
ENV LANG='en_US.UTF-8' LANGUAGE='en_US:en' LC_ALL='en_US.UTF-8'
ENTRYPOINT ["./komga-rust"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 CMD ["curl", "-fsS", "http://127.0.0.1:25600/health/ready"]
EXPOSE 25600
LABEL org.opencontainers.image.source="https://github.com/gotson/komga"
