# EVM Adapter Mapping

## First plausible intents

- native transfer
- ERC-20 transfer
- contract call with known ABI

## Execution adapter mapping

| Field | Mapping |
|---|---|
| `adapter_id` | `evm` |
| likely normalized intent kinds | `evm.transfer.native`, `evm.transfer.erc20`, `evm.contract.call` |
| stable execution references | transaction hash, chain id, nonce |
| likely `fetch_status(...)` | yes |

## Expected facts

- source address
- destination address
- asset or token contract
- amount or call value
- method selector or action
- chain id
- execution reference
- finality expectation

## Observed facts

- observed tx hash
- receipt status
- log-derived transfer values
- source and destination
- amount
- block number / confirmations
- revert reason when present

## Mismatch mapping

| Subcode | Generic category | Baseline severity | Default operator path |
|---|---|---|---|
| `tx_hash_missing` | `observation_missing` | `warning` | `investigate` |
| `amount_mismatch` | `amount_mismatch` | `critical` | `investigate` |
| `destination_mismatch` | `destination_mismatch` | `critical` | `investigate` |
| `chain_mismatch` | `policy_violation` | `high` | `investigate` |
| `nonce_gap_detected` | `external_state_unknown` | `warning` | `investigate` |
| `revert_reason_differs_from_expected` | `state_mismatch` | `high` | `replay_review` |
| `confirmations_pending_too_long` | `delayed_finality` | `warning` | `acknowledge` |

## Evidence mapping

| Evidence source | Purpose |
|---|---|
| adapter-local intent table | expected source, destination, asset, amount, method |
| adapter-local attempt table | execution reference and provider routing metadata |
| durable tx receipt snapshot | observed receipt status, gas use, block number |
| decoded log evidence | token transfer facts and event correlation |

## Non-goals for v1

- generic log decoding for arbitrary protocols
- mempool intelligence beyond durable provider observations
- cross-chain bridge reconciliation
