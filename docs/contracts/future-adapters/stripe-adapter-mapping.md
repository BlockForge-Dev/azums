# Stripe Adapter Mapping

## First plausible intents

- payment intent confirmation
- capture
- refund
- payout verification

## Execution adapter mapping

| Field | Mapping |
|---|---|
| `adapter_id` | `stripe` |
| likely normalized intent kinds | `stripe.payment.confirm`, `stripe.payment.capture`, `stripe.refund.verify`, `stripe.payout.verify` |
| stable execution references | payment_intent id, charge id, refund id |
| likely `fetch_status(...)` | yes |

## Expected facts

- customer reference
- amount
- currency
- payment object reference
- status expectation
- settlement/finality window

## Observed facts

- observed payment object id
- observed amount and currency
- observed payment status
- settlement state
- webhook event lineage

## Mismatch mapping

| Subcode | Generic category | Baseline severity | Default operator path |
|---|---|---|---|
| `payment_reference_missing` | `observation_missing` | `warning` | `investigate` |
| `payment_status_mismatch` | `state_mismatch` | `high` | `investigate` |
| `amount_mismatch` | `amount_mismatch` | `critical` | `investigate` |
| `currency_mismatch` | `state_mismatch` | `high` | `investigate` |
| `settlement_pending_too_long` | `delayed_finality` | `warning` | `acknowledge` |
| `duplicate_event` | `duplicate_signal` | `info` | `false_positive` |

## Evidence mapping

| Evidence source | Purpose |
|---|---|
| adapter-local payment intent row | expected amount, currency, customer ref, payment object ref |
| provider event table | webhook lineage and state transitions |
| verification snapshot row | observed payment object state at recon time |

## Non-goals for v1

- full dispute lifecycle reconciliation
- balance and treasury ledger reconciliation outside the targeted payment object
