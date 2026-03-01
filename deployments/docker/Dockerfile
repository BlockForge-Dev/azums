# syntax=docker/dockerfile:1.7

FROM rust:1.85-bookworm AS builder
ARG APP_MANIFEST
ARG BIN_NAME
WORKDIR /workspace

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY . .

RUN test -n "${APP_MANIFEST}" && test -n "${BIN_NAME}"
RUN cargo build --release --manifest-path "${APP_MANIFEST}" \
    && test -f "target/release/${BIN_NAME}"

FROM debian:bookworm-slim AS runtime
ARG BIN_NAME
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --home-dir /home/appuser --shell /usr/sbin/nologin --uid 10001 appuser

COPY --from=builder /workspace/target/release/${BIN_NAME} /usr/local/bin/app

USER appuser
ENTRYPOINT ["/usr/local/bin/app"]
