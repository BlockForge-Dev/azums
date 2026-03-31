# Exception Intelligence Index V1

## Purpose

Milestone 6 turns reconciliation divergence into a durable, searchable operator surface without changing execution truth ownership.

Execution truth remains in `execution_core_*`.

Reconciliation truth remains in `recon_core_*`.

Exception intelligence derives and indexes operational cases downstream.

## Storage Boundary

Exception intelligence now owns its own tables:

- `exception_cases`
- `exception_evidence`
- `exception_events`
- `exception_resolution_history`

These tables are separate from:

- `execution_core_*`
- `recon_core_*`
- `callback_core_*`

This keeps exception handling queryable without polluting execution or reconciliation storage.

## Intake and Creation Pipeline

`ReconEngine` escalates exception drafts through `PostgresExceptionStore::sync_subject_cases`.

That pipeline now:

1. loads linkage back to the latest execution receipt, recon outcome, recon receipt, and evidence snapshot
2. classifies each draft through `ExceptionClassifier`
3. deduplicates on `(tenant_id, dedupe_key)`
4. clusters related cases with `cluster_key`
5. attaches durable evidence records
6. records operator-visible case events
7. auto-resolves no-longer-active open cases without mutating upstream truth

## Searchable Read Model

The read surface is exposed through `status_api`:

- `GET /exceptions`
- `GET /exceptions/:case_id`
- `POST /exceptions/:case_id/state`

The index currently supports filtering by:

- `state`
- `severity`
- `category`
- `adapter_id`
- `subject_id`
- `intent_id`
- `cluster_key`
- free-text search over summary, machine reason, case id, and keys

Tenant scope is enforced before any exception data is returned.

## Operator Workflow

The durable operator states are:

- `open`
- `acknowledged`
- `investigating`
- `resolved`
- `dismissed`
- `false_positive`

Transition rules are explicit in the exception store:

- active cases can move between active workflow states and into terminal closure states
- terminal cases may be reopened to `open` or `investigating`
- terminal-to-terminal hops are rejected

Every change writes:

- an `exception_events` row
- an `exception_resolution_history` row for terminal transitions

## Evidence Model

Every meaningful case is linked back to durable evidence, not just a string summary.

Current evidence attachment types include:

- `execution_receipt`
- `recon_outcome`
- `recon_receipt`
- `observed_fact_snapshot`
- `adapter_details`
- rule-pack-specific evidence emitted by exception drafts

This makes each case explainable from:

- execution receipt lineage
- recon result lineage
- observed fact evidence
- adapter-specific details

## Dedupe and Clustering

`ExceptionClassifier` computes:

- `dedupe_key`: subject-scoped stable case identity
- `cluster_key`: broader grouping for similar divergence across subjects

This supports:

- replay-safe updates to the same active case
- operator grouping without overwriting the historical case trail

## Guarantees

Milestone 6 now guarantees:

1. recon divergence persists as durable exception truth
2. exception truth never overwrites execution truth
3. every case can point back to durable evidence
4. cases remain filterable and tenant-safe
5. operator workflow changes are audited through event and resolution history
