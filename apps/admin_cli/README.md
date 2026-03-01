# Execution Core Worker

Runs the `execution_core` worker loop against PostgresQ (`jobs` table) and routes intents to registered adapters.

## Required Env

- `DATABASE_URL`

## Useful Env

- `EXECUTION_DISPATCH_QUEUE` (default: `execution.dispatch`)
- `EXECUTION_CALLBACK_QUEUE` (default: `execution.callback`)
- `EXECUTION_WORKER_ID` (default: `execution-core-worker`)
- `EXECUTION_CALLBACK_WORKER_ID` (default: `execution-callback-worker`)
- `EXECUTION_LEASE_SECONDS` (default: `30`)
- `EXECUTION_BATCH_SIZE` (default: `32`)
- `EXECUTION_ALLOWED_ADAPTERS` (default: `adapter_solana`)
- `SOLANA_SYNC_MAX_POLLS` (default: `8`)
- `SOLANA_SYNC_POLL_DELAY_MS` (default: `1200`)
- `EXECUTION_CALLBACK_DELIVERY_URL` (optional: if set, callbacks are POSTed to this URL; otherwise printed to stdout)
- `EXECUTION_CALLBACK_DELIVERY_TOKEN` (optional bearer token for callback POSTs)

Tenant callback destination precedence:

- If a tenant-specific destination exists in `callback_core_tenant_destinations`, worker delivery uses it.
- If no tenant destination exists, worker falls back to the env-based dispatcher (`EXECUTION_CALLBACK_DELIVERY_URL` or stdout).

## Run

```bash
cargo run
```
