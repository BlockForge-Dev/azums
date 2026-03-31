# syntax=docker/dockerfile:1.7

FROM rust:1.88-bookworm AS builder
ARG APP_MANIFEST
ARG BIN_NAME
ARG INCLUDE_SOLANA_SIGNER=false
WORKDIR /workspace
ENV CARGO_TARGET_DIR=/workspace/target

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY . .

RUN mkdir -p crates/adapter_solana/node_modules \
    && if [ "${INCLUDE_SOLANA_SIGNER}" = "true" ]; then \
        apt-get update \
        && apt-get install -y --no-install-recommends nodejs npm \
        && rm -rf /var/lib/apt/lists/* \
        && cd crates/adapter_solana \
        && npm ci --omit=dev; \
    fi

RUN test -n "${APP_MANIFEST}" && test -n "${BIN_NAME}"
RUN cargo build --release --manifest-path "${APP_MANIFEST}" \
    && test -f "target/release/${BIN_NAME}"

FROM debian:bookworm-slim AS runtime
ARG BIN_NAME
ARG INCLUDE_SOLANA_SIGNER=false
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && if [ "${INCLUDE_SOLANA_SIGNER}" = "true" ]; then \
        apt-get install -y --no-install-recommends nodejs npm; \
    fi \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --home-dir /home/appuser --shell /usr/sbin/nologin --uid 10001 appuser

COPY --from=builder /workspace/target/release/${BIN_NAME} /usr/local/bin/app
RUN mkdir -p /workspace/crates/adapter_solana
COPY --from=builder /workspace/crates/adapter_solana/gen_tx.mjs /workspace/crates/adapter_solana/gen_tx.mjs
COPY --from=builder /workspace/crates/adapter_solana/package*.json /workspace/crates/adapter_solana/
COPY --from=builder /workspace/crates/adapter_solana/node_modules /workspace/crates/adapter_solana/node_modules

USER appuser
ENTRYPOINT ["/usr/local/bin/app"]
