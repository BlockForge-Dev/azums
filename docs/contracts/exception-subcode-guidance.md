# Exception Subcode Guidance

## Purpose

Keep exception intelligence explainable and comparable across adapters without flattening every adapter into the same domain vocabulary.

## Rules

### Naming

- use `snake_case`
- make subcodes stable and reviewable
- keep them adapter-specific when needed
- avoid raw provider error strings as durable subcodes

Good:

- `amount_mismatch`
- `destination_mismatch`
- `payment_status_mismatch`
- `channel_not_found`

Bad:

- `rpc_err_502_from_provider_x`
- `sdk-threw-random-error`

### Mapping

Every subcode must map to:

- one generic exception category
- one baseline severity
- one default operator path

Recommended default operator paths:

- `acknowledge`
- `investigate`
- `resolve`
- `false_positive`
- `replay_review`

### Evidence

Every subcode must point to evidence, not only text:

- execution receipt reference
- recon receipt reference
- evidence snapshot reference
- adapter-local durable source reference where applicable

Minimum evidence bundle for any subcode:

1. execution receipt or request lineage reference
2. recon run or recon receipt reference
3. observed evidence reference
4. machine-readable payload that explains the mismatch

### Clustering

Subcodes should support case dedupe and clustering.

Use stable, reviewable keys derived from:

- adapter id
- mismatch subcode
- tenant scope
- execution reference or logical destination

Do not cluster on raw provider error text.

## Generic Category Checklist

Use the smallest generic category that still makes sense:

| Generic category | Use when |
|---|---|
| `observation_missing` | expected evidence or observation did not appear |
| `state_mismatch` | observed state contradicts expected state |
| `amount_mismatch` | amount/value differs materially |
| `destination_mismatch` | destination/recipient/channel differs materially |
| `delayed_finality` | observed progress is too slow for policy |
| `duplicate_signal` | duplicate delivery/execution/provider signal detected |
| `external_state_unknown` | external source cannot be trusted or resolved |
| `policy_violation` | execution/recon violates explicit product policy |
| `manual_review_required` | automated path is intentionally insufficient |

## Severity Guidance

| Severity | Meaning |
|---|---|
| `info` | useful, low-risk operator signal |
| `warning` | should be reviewed but not urgent |
| `high` | materially risky or customer-visible divergence |
| `critical` | launch-blocking, trust-breaking, or safety-critical divergence |

## Mapping Template

Every adapter mapping doc should fill a table like this:

| Subcode | Generic category | Baseline severity | Default operator path | Minimum evidence |
|---|---|---|---|---|
| `example_subcode` | `state_mismatch` | `high` | `investigate` | recon snapshot + provider ref |

## Adapter Examples

| Adapter | Example subcode | Generic category |
|---|---|---|
| Solana | `signature_missing` | `observation_missing` |
| EVM | `revert_reason_differs_from_expected` | `state_mismatch` |
| HTTP | `status_code_mismatch` | `state_mismatch` |
| Slack | `channel_not_found` | `destination_mismatch` |
| Email | `bounce_observed` | `state_mismatch` |
| Stripe | `payment_status_mismatch` | `state_mismatch` |
| Paystack | `verification_reference_missing` | `observation_missing` |
| Flutterwave | `settlement_pending_too_long` | `delayed_finality` |

## Guardrails

- Do not use raw provider exception strings as durable subcodes.
- Do not use one subcode to cover unrelated operator paths.
- Do not create generic top-level categories for one adapter only.
- Do not attach a subcode without operator-safe evidence.
