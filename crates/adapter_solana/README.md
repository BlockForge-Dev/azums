# Solana Adapter

`adapter_solana` executes Solana intents directly via RPC while preserving the core adapter contract.

It also implements `adapter_contract::DomainAdapter` methods so Solana adapts cleanly to the existing core contract:

- `validate(...)`
- `execute(...)` / `execute_solana_intent(...)`
- `resume(...)` / `resume_solana_intent(...)`
- `fetch_status(...)` / `check_submission_status(...)`
- `normalize_solana_error(...)`

Status progression exposed to core:

- `submitted`
- `confirming`
- `landed`
- `finalized`

Metadata surfaced in adapter details/status when available:

- `provider_used`
- `blockhash_used`
- `simulation_outcome`

Durable records managed by the adapter:

- `solana.tx_intents`
- `solana.tx_attempts`

Execution responsibilities handled inside the adapter:

- schema bootstrap for Solana intent/attempt tables
- build/sign flow (via `gen_tx.mjs` when `signed_tx_base64` is not supplied)
- optional simulation/preflight
- RPC submission
- status/finality polling
- provider error normalization into contract-stable categories
- durable active-attempt reuse to prevent accidental double-submit on re-dispatch
- bootstrap-time dedup of duplicate active attempts before enforcing unique active-attempt index

Adapter runtime knobs:

- `sync_max_polls` (default: `8`)
- `sync_poll_delay_ms` (default: `1200`)
- `SOLANA_PLATFORM_SIGNING_ENABLED` (default `true`; set `false` in production customer-signed mode)

Default routing helper:

- `register_default_solana_adapter(...)`

Supported intent kinds:

- `solana.transfer.v1`
- `solana.broadcast.v1`
