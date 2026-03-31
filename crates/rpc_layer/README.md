# RPC Layer

`rpc_layer` provides transport/provider abstractions for domain adapters.

## Current Scope

- Shared ordered-provider helpers for hybrid failover:
  - `resolve_provider_urls(...)`
  - `preferred_provider_urls(...)`
  - `primary_provider_url(...)`
  - `parse_provider_urls(...)`
- Solana JSON-RPC client helpers:
  - `rpc_get_latest_blockhash(...)`
  - `rpc_simulate_transaction(...)`
  - `rpc_send_transaction(...)`
  - `rpc_get_signature_status(...)`
  - `rpc_get_latest_blockhash_with_failover(...)`
  - `rpc_simulate_transaction_with_failover(...)`
  - `rpc_send_transaction_with_failover(...)`
  - `rpc_get_signature_status_with_failover(...)`
- Normalized RPC error types:
  - `RpcCallError::Transport`
  - `RpcCallError::Provider`
- Shared client bootstrap with configurable timeout:
  - `SOLANA_RPC_TIMEOUT_MS`

## Cross-Chain Provider Pattern

Azums keeps provider failover below the adapter->core contract boundary.

That means future adapters should keep the same core-facing shape:

- adapter receives normalized intent
- adapter returns normalized outcome
- receipt captures `provider_used`, ordered `rpc_urls`, signing mode, payer source, and tx reference
- execution core remains chain-agnostic

Recommended env convention for every chain adapter:

- `<CHAIN>_RPC_PRIMARY_URL`
- `<CHAIN>_RPC_FALLBACK_URLS`
- `<CHAIN>_RPC_URLS`
- `<CHAIN>_RPC_URL`

Resolution order is:

1. explicit request/workspace override
2. `<CHAIN>_RPC_PRIMARY_URL`
3. `<CHAIN>_RPC_URLS`
4. `<CHAIN>_RPC_FALLBACK_URLS`
5. `<CHAIN>_RPC_URL`
6. chain default

Recommended production posture:

- managed/external RPC first
- self-hosted RPC second
- customer-signed execution by default
- provider provenance recorded on every attempt

This keeps raw provider transport logic out of adapters while preserving stable adapter->core contracts across Solana, future EVM adapters, and future Sui adapters.
