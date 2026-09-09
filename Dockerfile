# Stage 1: Build Modern React UI
FROM node:22-alpine AS ui-builder
WORKDIR /app/web
COPY web/package*.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# Stage 2: Build Rust Backend Binary
FROM rust:1.94-slim-bookworm AS rust-builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler curl && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock* build.rs ./
COPY proto/ ./proto/
COPY crates/ ./crates/
COPY src/ ./src/
COPY --from=ui-builder /app/web/dist ./web/dist
RUN cargo build --release

# Stage 3: Minimal Production Container with Integrated Nginx Reverse Proxy
FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update && apt-get install -y \
    ca-certificates \
    tzdata \
    curl \
    nginx \
    openssl \
    && rm -rf /var/lib/apt/lists/*

COPY --from=rust-builder /app/target/release/conduit /app/conduit
COPY docker/entrypoint.sh /app/entrypoint.sh
RUN chmod +x /app/entrypoint.sh

ENV CONDUIT_BIND="0.0.0.0"
ENV CONDUIT_PORT=4242
ENV CONDUIT_HTTP_PORT=4242
ENV CONDUIT_HTTPS_PORT=443
ENV CONDUIT_INTERNAL_PORT=42420
ENV CONDUIT_SSL_ENABLED=auto
ENV CONDUIT_CONFIG="/data/config.json"
ENV CONDUIT_DB="/data/db.sqlite"

VOLUME ["/data", "/queue", "/store"]

EXPOSE 80 443 4242 42420

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD curl -f http://127.0.0.1:42420/api/health || curl -f http://127.0.0.1:4242/api/health || exit 1

ENTRYPOINT ["/app/entrypoint.sh"]
