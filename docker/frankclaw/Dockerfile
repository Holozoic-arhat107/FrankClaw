FROM rust:1.89-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY examples ./examples

RUN cargo build --release -p frankclaw

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/frankclaw /usr/local/bin/frankclaw

ENV FRANKCLAW_CONFIG=/config/frankclaw.json
ENV FRANKCLAW_STATE_DIR=/var/lib/frankclaw

EXPOSE 18789

ENTRYPOINT ["frankclaw", "gateway"]
