# Execution Core Worker

Runs the `execution_core` worker loop against PostgresQ (`jobs` table) and routes intents to registered adapters.

## Required Env

- `DATABASE_URL`

## Useful Env

- `EXECUTION_DISPATCH_QUEUE` (default: `execution.dispatch`)
- `EXECUTION_CALLBACK_QUEUE` (default: `execution.callback`)
- `EXECUTION_WORKER_MODE` (default: `all`; use `dispatch` or `callback` when running split workers)
- `EXECUTION_WORKER_ID` (default: `execution-core-worker`)
- `EXECUTION_CALLBACK_WORKER_ID` (default: `execution-callback-worker`)
- `EXECUTION_LEASE_SECONDS` (default: `30`)
- `EXECUTION_BATCH_SIZE` (default: `32`, legacy shared fallback)
- `EXECUTION_DISPATCH_BATCH_SIZE` (recommended when split: `1`)
- `EXECUTION_CALLBACK_BATCH_SIZE` (recommended when split: `4`)
- `EXECUTION_DISPATCH_DB_MAX_CONNECTIONS` (recommended when split: `8`)
- `EXECUTION_CALLBACK_DB_MAX_CONNECTIONS` (recommended when split: `4`)
- `EXECUTION_DISPATCH_NOTIFY_MAX_WAIT_MS` (default: `500`; max wait on `LISTEN/NOTIFY` before fallback loop)
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

Split worker mode examples:

```bash
EXECUTION_WORKER_MODE=dispatch cargo run
EXECUTION_WORKER_MODE=callback cargo run
```

For scaled dispatch workers, run at least two `dispatch` processes with different `EXECUTION_WORKER_ID` values and the same dispatch queue.
