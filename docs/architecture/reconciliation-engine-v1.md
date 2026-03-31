# Reconciliation Engine V1

## Purpose

Milestone 4 defines the generic reconciliation engine that processes recon subjects without embedding adapter-specific logic into the worker runtime.

Execution truth remains owned by `execution_core`.

The engine:

- loads a recon subject and context
- resolves the adapter-specific rule pack through the registry
- loads adapter observations through a generic store interface
- computes expected facts, observed facts, match results, and normalized recon outcome
- writes a recon run, recon receipt, recon outcome summary, and evidence snapshot
- escalates exception cases when divergence or manual review is required
- schedules recon retries independently from execution retries

## Core Boundary

`ReconWorker` is now orchestration only.

`ReconEngine` owns:

- recon run lifecycle transitions
- retry scheduling for recon failures
- normalized result derivation
- evidence snapshot construction
- final recon receipt emission

The worker still owns:

- source watermark polling
- intake/backfill polling loops
- subject claiming cadence

## Recon Run Lifecycle

`ReconRunState` now records the processor lifecycle separately from execution lifecycle:

- `queued`
- `collecting_observations`
- `matching`
- `writing_receipt`
- `completed`
- `retry_scheduled`
- `failed`

These transitions are stored in:

- `recon_core_run_state_transitions`

## Normalized Result Model

Each run also stores a framework-level normalized result:

- `matched`
- `partially_matched`
- `unmatched`
- `pending_observation`
- `stale`
- `manual_review_required`

This is distinct from execution lifecycle and distinct from recon processor lifecycle.

## Retry Isolation

Recon retries are downstream retries only.

They do not mutate execution retry state.

Current recon retry controls:

- `EXECUTION_RECON_MAX_RETRY_ATTEMPTS`
- `EXECUTION_RECON_RETRY_BACKOFF_MS`

Current subject-side retry metadata:

- `recon_attempt_count`
- `recon_retry_count`
- `next_reconcile_after_ms`
- `last_recon_error`
- `last_run_state`

## Durable Evidence

Every recon run now leaves a durable evidence snapshot in:

- `recon_core_evidence_snapshots`

Each snapshot captures:

- serialized recon context
- adapter observation rows
- expected facts
- observed facts
- match result
- receipt details
- exception drafts
- normalized result and lifecycle state

## Durability Guarantees

Under normal conditions:

1. subject creation happens exactly once per signal
2. run identity is stable per processing attempt
3. final run write updates the run row, writes receipt/outcome/facts/evidence, records the final state transition, and updates subject retry state in one transaction

This preserves lineage across:

- success
- unmatched results
- stale results
- manual review
- retry-scheduled recon failures
