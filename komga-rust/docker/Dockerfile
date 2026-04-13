# syntax=docker/dockerfile:1.7

FROM --platform=$TARGETPLATFORM alpine:3.23 AS runner
ARG TARGETOS
ARG TARGETARCH
RUN apk add --no-cache tzdata
WORKDIR /app
COPY --chmod=755 docker-artifacts/${TARGETOS}-musl/${TARGETARCH}/komga-rust ./komga-rust
COPY docker-artifacts/${TARGETOS}-musl/${TARGETARCH}/libpdfium.so ./libpdfium.so
RUN <<'EOF'
set -eu
install -m 755 /dev/null ./docker-runtime.sh
cat > ./docker-runtime.sh <<'SCRIPT'
#!/bin/sh
set -eu

command="${1:-run}"
if [ "$command" = "run" ]; then
  shift
  if [ -z "${KOMGA_RUST_ADDR:-}" ]; then
    export KOMGA_RUST_ADDR="[::]:${SERVER_PORT:-25600}"
  else
    export KOMGA_RUST_ADDR
  fi
  exec /app/komga-rust "$@"
fi

if [ "$command" = "healthcheck" ]; then
  port="${SERVER_PORT:-25600}"
  if [ -n "${KOMGA_RUST_ADDR:-}" ]; then
    port="${KOMGA_RUST_ADDR##*:}"
  fi

  path="${SERVER_SERVLET_CONTEXT_PATH:-}/actuator/health"

  if response="$(wget -qO- "http://[::1]:${port}${path}" 2>/dev/null)"; then
    :
  elif response="$(wget -qO- "http://127.0.0.1:${port}${path}" 2>/dev/null)"; then
    :
  else
    exit 1
  fi

  printf '%s' "$response" | grep -q '"status":"UP"'
  exit 0
fi

exit 64
SCRIPT
EOF
VOLUME /config
ENV KOMGA_CONFIG_DIR="/config"
ENV KOMGA_RUST_MODE="localdb"
ENV KOMGA_RUST_PLATFORM_PROFILE="docker"
ENV LOGGING_FILE_NAME="/config/logs/komga.log"
ENV LANG='C.UTF-8' LC_ALL='C.UTF-8'
ENTRYPOINT ["./docker-runtime.sh", "run"]
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 CMD ["./docker-runtime.sh", "healthcheck"]
EXPOSE 25600
LABEL org.opencontainers.image.source="https://github.com/huihuimoe/komga-riir"
