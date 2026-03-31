# Paystack Adapter Mapping

## First plausible intents

- transaction verification
- charge confirmation
- refund verification

## Execution adapter mapping

| Field | Mapping |
|---|---|
| `adapter_id` | `paystack` |
| likely normalized intent kinds | `paystack.transaction.verify`, `paystack.charge.confirm`, `paystack.refund.verify` |
| stable execution references | reference, transaction id |
| likely `fetch_status(...)` | yes |

## Expected facts

- customer reference
- amount
- currency
- provider reference
- expected transaction state

## Observed facts

- observed reference
- observed amount and currency
- observed verification state
- provider settlement state
- webhook lineage

## Mismatch mapping

| Subcode | Generic category | Baseline severity | Default operator path |
|---|---|---|---|
| `verification_reference_missing` | `observation_missing` | `warning` | `investigate` |
| `payment_status_mismatch` | `state_mismatch` | `high` | `investigate` |
| `amount_mismatch` | `amount_mismatch` | `critical` | `investigate` |
| `currency_mismatch` | `state_mismatch` | `high` | `investigate` |
| `verification_pending_too_long` | `delayed_finality` | `warning` | `acknowledge` |
| `duplicate_event` | `duplicate_signal` | `info` | `false_positive` |

## Evidence mapping

| Evidence source | Purpose |
|---|---|
| adapter-local transaction row | expected amount, currency, customer ref, provider ref |
| provider verification snapshot | observed provider verification state |
| provider webhook event row | webhook lineage and duplicate-event evidence |

## Non-goals for v1

- full settlement ledger reconciliation beyond targeted transaction objects
- card fraud/dispute analytics
