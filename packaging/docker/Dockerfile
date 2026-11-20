# syntax=docker/dockerfile:1

FROM rust:1.86-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY apps ./apps
COPY crates ./crates

RUN cargo build --release -p clipforge-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /app --shell /usr/sbin/nologin clipforge

WORKDIR /app

COPY --from=builder /src/target/release/clipforge-server /usr/local/bin/clipforge-server
COPY apps/web /app/web

RUN mkdir -p /data/clips && chown -R clipforge:clipforge /app /data

USER clipforge

ENV CLIPFORGE_BIND_ADDR=0.0.0.0:3000 \
    CLIPFORGE_DATABASE_PATH=/data/clipforge-server.db \
    CLIPFORGE_STORAGE_BACKEND=local \
    CLIPFORGE_STORAGE_DIR=/data/clips \
    CLIPFORGE_WEB_DIR=/app/web

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3000/health || exit 1

ENTRYPOINT ["/usr/local/bin/clipforge-server"]
