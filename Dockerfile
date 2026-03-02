FROM rust:1.85-slim AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY config ./config

RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/rustchain /usr/local/bin/rustchain
COPY config/default.toml /app/config/default.toml

EXPOSE 6000 7000
VOLUME ["/app/data"]

ENTRYPOINT ["rustchain"]
CMD ["--config", "/app/config/default.toml", "start-node"]
