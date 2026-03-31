# Config Pack

This folder provides reusable environment configuration templates for the Azums platform.

The runtime currently reads configuration from environment variables, so these files are
designed to be copied and adapted for each deployment environment.

## Layout

| Path | Purpose |
|---|---|
| `profiles/` | Environment bundles for common deployment modes |
| `services/` | Per-service env templates for isolated service runs |

## Quick Start

1. For docker compose local dev, copy `profiles/dev-compose.env.example` to `deployments/compose/.env`.
2. For direct local runs, copy `profiles/local.env.example` to `.env.local` and load it in your shell.
3. For production bootstrapping, copy `profiles/production.env.template` to your secret manager/env injection flow and replace placeholders.

## Notes

- Keep secrets out of git.
- Keep tenant and principal bindings explicit.
- Keep replay and callback permissions restricted by default in non-dev environments.
- For non-dev environments, set `OPERATOR_UI_REQUIRE_DURABLE_METERING=true` so usage/quota
  checks fail closed when durable metering is unavailable.
