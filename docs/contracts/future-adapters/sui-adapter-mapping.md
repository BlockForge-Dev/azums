# Sui Adapter Mapping

## First plausible intents

- coin transfer
- programmable transaction block execution
- package/function invocation

## Execution adapter mapping

| Field | Mapping |
|---|---|
| `adapter_id` | `sui` |
| likely normalized intent kinds | `sui.transfer.coin`, `sui.ptb.execute`, `sui.package.call` |
| stable execution references | digest, checkpoint |
| likely `fetch_status(...)` | yes |

## Expected facts

- sender
- recipient
- coin type
- amount
- package/module/function or action
- digest expectation
- checkpoint/finality expectation

## Observed facts

- observed digest
- observed status
- sender and recipient
- amount
- coin type
- checkpoint/finality state
- execution error payload

## Mismatch mapping

| Subcode | Generic category | Baseline severity | Default operator path |
|---|---|---|---|
| `digest_missing` | `observation_missing` | `warning` | `investigate` |
| `amount_mismatch` | `amount_mismatch` | `critical` | `investigate` |
| `recipient_mismatch` | `destination_mismatch` | `critical` | `investigate` |
| `coin_type_mismatch` | `state_mismatch` | `high` | `investigate` |
| `function_mismatch` | `state_mismatch` | `high` | `investigate` |
| `checkpoint_pending_too_long` | `delayed_finality` | `warning` | `acknowledge` |
| `observed_error_differs_from_expected` | `state_mismatch` | `high` | `replay_review` |

## Evidence mapping

| Evidence source | Purpose |
|---|---|
| adapter-local intent rows | expected sender, recipient, coin type, action |
| adapter-local attempt rows | digest, checkpoint, provider routing |
| transaction/digest evidence snapshot | observed execution status and payload |
| checkpoint status evidence | finality timing and convergence evidence |

## Non-goals for v1

- deep object graph reconciliation across unrelated Sui objects
- generalized package ABI understanding for arbitrary contracts
