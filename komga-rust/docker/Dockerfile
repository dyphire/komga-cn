# syntax=docker/dockerfile:1.7

FROM --platform=$TARGETPLATFORM alpine:3.23 AS runner
ARG TARGETOS
ARG TARGETARCH
RUN apk add --no-cache tzdata
WORKDIR /app
COPY docker-artifacts/${TARGETOS}-musl/${TARGETARCH}/komga-rust ./komga-rust
COPY docker-artifacts/${TARGETOS}-musl/${TARGETARCH}/libpdfium.so ./libpdfium.so
RUN chmod +x ./komga-rust
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
