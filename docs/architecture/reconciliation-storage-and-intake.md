# Reconciliation Storage And Intake

## Purpose

Milestone 3 defines the downstream storage and intake pipeline that materializes reconciliation work from durable execution truth.

Execution truth remains owned by `execution_core`.

This layer only:

- records recon intake lineage
- materializes recon subjects
- stores intake-facing expected fact snapshots
- schedules reconciliation work
- persists recon runs, outcomes, receipts, and facts in recon-owned tables

## Storage Surfaces

Recon storage remains separate from execution tables.

Current recon-owned tables:

- `recon_core_subjects`
- `recon_core_intake_events`
- `recon_core_source_watermarks`
- `recon_core_runs`
- `recon_core_outcomes`
- `recon_core_expected_facts`
- `recon_core_observed_facts`
- `recon_core_receipts`

### Subject Row

`recon_core_subjects` now stores:

- tenant / intent / job / adapter lineage
- latest receipt / transition / callback linkage
- latest intake signal metadata
- execution correlation ID
- adapter execution reference
- external observation key
- expected fact snapshot from intake
- dirty scheduling flag
- `scheduled_at_ms`

## Intake Pipeline

Primary intake source:

- `platform_recon_intake_signals`

Primary service:

- `ReconIntakeService`

Primary flow:

1. intake worker polls `platform_recon_intake_signals`
2. intake service claims the signal in `recon_core_intake_events`
3. duplicate signals are ignored by signal-id idempotency
4. subject is materialized or updated in `recon_core_subjects`
5. intake-facing expected fact snapshot is stored on the subject
6. subject is scheduled by setting `dirty = true` and `scheduled_at_ms`
7. reconcile worker later claims dirty subjects and persists runs/outcomes/receipts/facts

## Idempotency And Dedupe

The dedupe rules are:

- every intake signal is keyed by `signal_id`
- `recon_core_intake_events.signal_id` is unique
- duplicate signal processing returns the already materialized subject instead of creating a new one
- subject identity is deterministic and unique on `(tenant_id, intent_id, job_id)`

This means:

- duplicate intake does not create duplicate subjects
- replay-safe reprocessing is allowed
- restart recovery is safe because scheduling state is durable

## Scheduling Mechanism

Recon scheduling stays adjacent to the execution queue machinery.

It does not reuse `PostgresQ` semantics directly.

Current mechanism:

- `dirty = true`
- `scheduled_at_ms`
- worker-side `claim_dirty_subjects(...)`

This survives restarts because both subject identity and scheduling state are stored durably in Postgres.

## Run Persistence

When reconciliation executes, recon-owned persistence records:

- `recon_core_runs`
- `recon_core_outcomes`
- `recon_core_expected_facts`
- `recon_core_observed_facts`
- `recon_core_receipts`

`recon_core_outcomes` is the durable summary table for run-level outcome materialization.

## Inspection Path

One real execution request can now be followed through:

1. execution receipt / transition / callback truth
2. recon intake signal
3. recon subject
4. queued scheduling state
5. recon run / outcome / receipt / facts

The status API continues to expose request-linked reconciliation reads on top of this storage.
