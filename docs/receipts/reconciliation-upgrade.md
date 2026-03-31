# Execution Receipt Reconciliation Upgrade

## Objective

Upgrade Azums execution receipts so downstream reconciliation can consume durable execution truth without pushing matching logic into `execution_core`.

This document is the Milestone 2 implementation note.

## What Changed

Execution receipt rows now carry the minimum stable references required for downstream reconciliation intake:

- `recon_subject_id`
- `reconciliation_eligible`
- `execution_correlation_id`
- `adapter_execution_reference`
- `external_observation_key`
- `expected_fact_snapshot`
- `receipt_version`

These fields are written by `execution_core` and remain part of execution truth.

They do not introduce reconciliation outcomes or matching semantics into the execution lifecycle.

## Updated Schema

Execution receipt entries are versioned.

| Field | Description |
|---|---|
| `receipt_version` | Schema version for the receipt entry |
| `recon_subject_id` | Deterministic reconciliation subject seed derived from `job_id` |
| `reconciliation_eligible` | Signals that downstream recon may begin from this receipt |
| `execution_correlation_id` | Intake/request correlation propagated into execution truth |
| `adapter_execution_reference` | Adapter execution reference such as provider reference or durable tx reference |
| `external_observation_key` | Stable key for downstream observation lookup |
| `expected_fact_snapshot` | Minimal versioned expected-fact seed, not a recon document |

## Receipt Migration Plan

No destructive migration is required.

Implementation strategy:

1. Keep existing `execution_core_receipts.receipt_json` rows valid.
2. Make all new fields additive and serde-defaulted.
3. Default missing `receipt_version` to `1`.
4. Write new receipt rows as version `2`.
5. Leave legacy rows queryable by the current status APIs.

Database impact:

- no existing receipt rows need rewriting
- no existing receipt IDs or ordering rules change
- new downstream intake records are stored separately in `platform_recon_intake_signals`

## Backward Compatibility Notes

Legacy receipt rows remain valid because:

- missing `receipt_version` defaults to `1`
- missing recon-facing fields default to `None` or `false`
- `status_api` continues to deserialize both legacy and upgraded receipt rows

Compatibility guarantees:

- existing receipt endpoints still return ordered receipt histories
- legacy consumers that ignore unknown JSON fields continue to work
- downstream reconciliation can deterministically derive the same future subject ID from `job_id`

## Recon Intake Event Design

Downstream reconciliation now consumes a narrow intake signal table:

- `platform_recon_intake_signals`

Signal fields include:

- signal identity
- source system
- signal kind
- tenant / intent / job / adapter references
- receipt / transition / callback linkage
- canonical execution state and classification
- execution correlation and adapter reference fields
- expected fact snapshot
- payload for audit/debug context

### Emission Rules

Execution-core signals:

- `submitted_with_reference`
  - emitted when a committed receipt includes an adapter execution reference
- `adapter_completed`
  - emitted when the adapter has returned a committed non-in-progress outcome
- `terminal_failure`
  - emitted when committed execution truth reaches terminal failure / dead-letter style states
- `finalized`
  - emitted when committed execution truth reaches `succeeded`

Callback-core signals:

- `callback_committed`
  - emitted after durable callback delivery publication so recon/exception systems can correlate delivery verification without mutating execution truth

## Ownership Rules

Execution core still owns:

- lifecycle transitions
- receipt writes
- replay policy
- canonical execution truth

Reconciliation still owns:

- expected-vs-observed matching
- recon runs and recon receipts

Exception intelligence still owns:

- divergence classification
- case severity
- evidence indexing

## Done Criteria Covered

Milestone 2 is satisfied in code when:

- every recon-eligible execution receipt carries deterministic downstream references
- receipt writes remain owned by `execution_core`
- matching logic is still absent from execution-core transitions
- status APIs deserialize both legacy and upgraded receipt rows
- one execution receipt can be traced deterministically to one future recon subject
