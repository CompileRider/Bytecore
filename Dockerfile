# Dockerfile — Terminal-only build (smaller image)
FROM rust:1-slim AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config && \
    rm -rf /var/lib/apt/lists/* && \
    cargo build --release --locked --no-default-features --features terminal

FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/bytecore /usr/local/bin/

ENTRYPOINT ["bytecore"]
