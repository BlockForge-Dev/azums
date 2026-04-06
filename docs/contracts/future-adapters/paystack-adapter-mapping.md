# Paystack Adapter Mapping

## First plausible intents

- transaction verification
- refund create
- refund verification
- transfer create
- transfer verification

## Execution adapter mapping

| Field | Mapping |
|---|---|
| `adapter_id` | `adapter_paystack` |
| normalized intent kinds | `paystack.transaction.verify.v1`, `paystack.refund.create.v1`, `paystack.refund.verify.v1`, `paystack.transfer.create.v1`, `paystack.transfer.verify.v1` |
| stable execution references | reference, transaction id, refund id, transfer code |
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
- duplicate-reference count across execution and webhook evidence

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

## Implemented v1 note

Paystack is now wired into the generic Azums reconciliation engine as `paystack.v1`.

- execution truth stays in `execution_core`
- observed evidence comes from:
  - `paystack.executions`
  - `paystack.webhook_events`
- dirty-subject intake is triggered from both Paystack execution updates and Paystack webhook evidence updates
- mismatch results map into the existing generic exception taxonomy instead of introducing Paystack-specific exception tables
