# syntax=docker/dockerfile:1.7

FROM rust:1.88-slim-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY migrations ./migrations
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked \
    && cp /app/target/release/memoir-server /tmp/memoir-server

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /tmp/memoir-server /usr/local/bin/memoir-server
COPY migrations ./migrations
COPY admin/dist ./admin/dist
ENV LISTEN_ADDR=0.0.0.0:8080
ENV ADMIN_STATIC_DIR=/app/admin/dist
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=5s --start-period=20s --retries=5 \
  CMD curl -fsS http://127.0.0.1:8080/health >/dev/null || exit 1
CMD ["memoir-server"]
