# Receipt Schema

## Purpose
A receipt is the durable explainability object for a request across its entire lifecycle.

## Receipt Record Shape
| Field | Type | Description |
|---|---|---|
| `tenant_id` | string | Tenant isolation boundary |
| `intent_id` | string | Request/intent identifier |
| `entries[]` | array | Ordered timeline of receipt events |

## Receipt Entry Shape
| Field | Type | Description |
|---|---|---|
| `receipt_id` | string | Stable receipt entry identifier |
| `job_id` | string | Execution attempt lineage identifier |
| `receipt_version` | uint32 | Receipt schema version; `1` for legacy rows, `2` for recon-upgraded rows |
| `recon_subject_id` | string optional | Deterministic future reconciliation subject identity derived from `job_id` |
| `reconciliation_eligible` | bool | Whether downstream reconciliation may start from this receipt |
| `execution_correlation_id` | string optional | Stable execution correlation identifier carried from intake/request context |
| `adapter_execution_reference` | string optional | Adapter-owned durable execution reference such as a provider reference or tx signature |
| `external_observation_key` | string optional | Stable external lookup key used by downstream observation collectors |
| `expected_fact_snapshot` | object optional | Minimal, versioned fact seed for downstream recon; never a full recon document |
| `occurred_at_ms` | uint64 | Event timestamp |
| `state` | enum | Canonical lifecycle state at event |
| `classification` | enum | Platform result class for this event |
| `attempt_no` | uint32 optional | Attempt number for retries/replays |
| `summary` | string | Human-readable explanation |
| `details` | object optional | Event metadata such as reason code, actor, replay lineage, and adapter evidence |

## Receipt Versions

| Version | Meaning |
|---|---|
| `1` | Legacy receipt rows written before reconciliation-facing fields existed |
| `2` | Current receipt rows with stable reconciliation references and snapshots |

Backward compatibility rule:

- missing `receipt_version` defaults to `1`
- missing recon-facing fields deserialize to `None` / `false`
- status/query APIs must continue to deserialize both versions

## Reconciliation-Facing Additions

These fields exist only to provide stable downstream reference points.

They must not turn execution receipts into reconciliation documents.

| Field | Ownership Rule |
|---|---|
| `recon_subject_id` | written by execution core only |
| `reconciliation_eligible` | decided by execution core only |
| `execution_correlation_id` | copied from request/intake context |
| `adapter_execution_reference` | copied from adapter outcome metadata only |
| `external_observation_key` | copied from adapter outcome metadata only |
| `expected_fact_snapshot` | minimal seed only; no matching logic |

## Recon Intake Signals

Execution receipts remain the durable truth source. Downstream systems consume stable intake signals derived from committed receipt/state writes.

Current signal kinds:

- `submitted_with_reference`
- `adapter_completed`
- `terminal_failure`
- `finalized`
- `callback_committed`

Signal storage:

- `platform_recon_intake_signals`

Execution core emits signals only after durable receipt writes.
Callback core emits `callback_committed` after durable callback delivery records are written.

## Required Receipt Coverage
| Category | Coverage Requirement |
|---|---|
| Intake | acceptance/rejection and key identifiers |
| Queue lifecycle | queued, leased, retry scheduling |
| Execution | adapter dispatch and classified outcomes |
| Failure details | stage, class, message, reason code |
| Delivery | callback outcome linkage (separate from execution success) |
| Replay lineage | replay trigger and linkage to source |

## Design Rules
| Rule | Requirement |
|---|---|
| Append-friendly | New entries append; prior history remains intact |
| Chronological readability | Ordered for operator timeline review |
| Machine + human balance | Include structured fields and readable messages |
| Tenant-safe | No cross-tenant leakage in receipt content |
