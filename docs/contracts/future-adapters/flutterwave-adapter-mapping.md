# Flutterwave Adapter Mapping

## First plausible intents

- transaction verification
- billing confirmation
- refund verification

## Execution adapter mapping

| Field | Mapping |
|---|---|
| `adapter_id` | `flutterwave` |
| likely normalized intent kinds | `flutterwave.transaction.verify`, `flutterwave.billing.confirm`, `flutterwave.refund.verify` |
| stable execution references | transaction id, `tx_ref` |
| likely `fetch_status(...)` | yes |

## Expected facts

- customer identity
- amount
- currency
- transaction reference
- expected billing state
- settlement window

## Observed facts

- observed transaction id
- observed tx_ref
- observed amount and currency
- observed verification state
- settlement or completion state
- webhook event lineage

## Mismatch mapping

| Subcode | Generic category | Baseline severity | Default operator path |
|---|---|---|---|
| `transaction_reference_missing` | `observation_missing` | `warning` | `investigate` |
| `payment_status_mismatch` | `state_mismatch` | `high` | `investigate` |
| `amount_mismatch` | `amount_mismatch` | `critical` | `investigate` |
| `currency_mismatch` | `state_mismatch` | `high` | `investigate` |
| `settlement_pending_too_long` | `delayed_finality` | `warning` | `acknowledge` |
| `duplicate_event` | `duplicate_signal` | `info` | `false_positive` |

## Evidence mapping

| Evidence source | Purpose |
|---|---|
| adapter-local billing row | expected amount, currency, customer identity, provider ref |
| provider verification snapshot | observed billing or transaction state |
| provider webhook delivery row | webhook lineage and duplicate-event evidence |

## Non-goals for v1

- merchant settlement reporting beyond the targeted transaction
- chargeback/dispute intelligence
