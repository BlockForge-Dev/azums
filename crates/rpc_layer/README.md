# RPC Layer

`rpc_layer` provides transport/provider abstractions for domain adapters.

## Current Scope

- Solana JSON-RPC client helpers:
  - `rpc_get_latest_blockhash(...)`
  - `rpc_simulate_transaction(...)`
  - `rpc_send_transaction(...)`
  - `rpc_get_signature_status(...)`
- Normalized RPC error types:
  - `RpcCallError::Transport`
  - `RpcCallError::Provider`
- Shared client bootstrap with configurable timeout:
  - `SOLANA_RPC_TIMEOUT_MS`

This keeps raw provider transport logic out of adapters while preserving adapter->core contracts.
