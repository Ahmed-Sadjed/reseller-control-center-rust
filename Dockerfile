# syntax=docker/dockerfile:1
# =====================================================================
# Multi-stage build for reseller-control-center-rust
# Stage 1: compile
# =====================================================================
FROM rust:1.97-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations

# sqlx::migrate! embeds ./migrations into the binary at compile time.
RUN cargo build --release

# =====================================================================
# Stage 2: minimal runtime (native-tls requires libssl3 + CA certs)
# =====================================================================
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 10001 appuser

COPY --from=builder /app/target/release/reseller-control-center-rust /app/reseller-control-center-rust

USER appuser
EXPOSE 8080
CMD ["/app/reseller-control-center-rust"]
