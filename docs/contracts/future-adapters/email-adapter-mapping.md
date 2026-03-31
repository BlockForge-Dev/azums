# Email Adapter Mapping

## First plausible intents

- transactional email send
- verification email send
- password-reset email send

## Execution adapter mapping

| Field | Mapping |
|---|---|
| `adapter_id` | `email` |
| likely normalized intent kinds | `email.send.transactional`, `email.send.verification`, `email.send.password_reset` |
| stable execution references | provider message id, template id, campaign id |
| likely `fetch_status(...)` | yes when delivery state is asynchronous |

## Expected facts

- recipient
- sender
- subject hash
- body/template hash
- provider message reference
- delivery expectation window

## Observed facts

- provider message id
- queued/sent/delivered/bounced status
- recipient
- subject hash when available
- suppression/bounce data

## Mismatch mapping

| Subcode | Generic category | Baseline severity | Default operator path |
|---|---|---|---|
| `provider_message_missing` | `observation_missing` | `warning` | `investigate` |
| `recipient_mismatch` | `destination_mismatch` | `critical` | `investigate` |
| `subject_hash_mismatch` | `state_mismatch` | `warning` | `investigate` |
| `bounce_observed` | `state_mismatch` | `high` | `investigate` |
| `delivery_pending_too_long` | `delayed_finality` | `warning` | `acknowledge` |
| `suppression_state_mismatch` | `policy_violation` | `high` | `investigate` |

## Evidence mapping

| Evidence source | Purpose |
|---|---|
| adapter-local send intent row | expected recipient, template hash, sender identity |
| provider delivery webhook row | delivery, bounce, suppression, provider event lineage |
| provider message lookup snapshot | observed provider message id and current state |

## Non-goals for v1

- inbox rendering fidelity
- spam-placement guarantees
- generalized mailbox engagement analytics
