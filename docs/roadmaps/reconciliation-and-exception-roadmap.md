# Reconciliation and Exception Roadmap

## Delivery Philosophy

This roadmap is governed by four non-negotiable rules.

| Rule | Law |
|---|---|
| Rule 1 | Execution truth remains owned by Azums core. |
| Rule 2 | Reconciliation consumes truth; it does not replace it. |
| Rule 3 | Exception intelligence classifies and indexes divergence; it does not silently mutate history. |
| Rule 4 | Everything remains adapter-neutral at the framework level, with Solana-first rule packs. |

## Scope

This roadmap adds two bounded subsystems downstream of existing Azums execution:

- `recon_core`: builds expected facts, collects observed facts, matches, and emits reconciliation outcomes
- `exception_intelligence`: classifies divergence into durable, queryable cases with severity and evidence

Both subsystems are downstream consumers of durable execution truth. Neither subsystem becomes the execution source of truth.

## Milestone 0: Architecture Freeze and Integration Specification

### Objective

Create the exact integration map before implementation so reconciliation and exception intelligence enter as bounded subsystems, not invasive logic inside execution core.

### Key Workstreams

- Audit and freeze current boundaries for:
  - ingress
  - PostgresQ
  - execution core
  - Solana adapter
  - callback/delivery core
  - status/query layer
- Document current lifecycle truth already present:
  - canonical execution lifecycle in core
  - Solana adapter evidence phases such as submitted, confirmed, and finalized where they exist as adapter facts rather than top-level lifecycle states
  - callback delivery lifecycle
- Map currently persisted execution surfaces:
  - requests/intents
  - jobs/attempts
  - receipts
  - adapter result payloads
  - callbacks
  - audit events
- Define exact downstream hook points:
  - after durable receipt writes
  - after adapter result persistence
  - after terminal or semi-terminal lifecycle transitions
  - via event or table-driven downstream intake
- Define hard boundaries for:
  - recon core
  - exception intelligence
  - downstream read/query models

### Deliverables

- Current Azums component map
- Current lifecycle state map
- Receipt schema audit
- Integration spec for reconciliation and exception downstream hooks
- Ownership matrix by subsystem
- ADR: `Reconciliation and exception intelligence as downstream bounded subsystems`

### Done Means

Milestone 0 is done when:

- the team can point to exactly where execution truth is written today
- the team can point to exactly which record or event reconciliation will consume
- every subsystem has a documented `owns / must not own` boundary
- the difference between execution truth, reconciliation truth, and exception truth is explicit and unambiguous
- a written integration spec is approved internally

### Failure Mode If Skipped

If this milestone is done poorly, reconciliation logic will leak into execution code and Azums will lose the clean execution-core shape it already has.

## Milestone 1: Reconciliation Contract and Exception Taxonomy v1

### Objective

Define the generic contracts before any real reconciliation logic is built.

### Key Workstreams

- Design normalized schemas for:
  - reconciliation subject
  - expected facts
  - observed facts
  - reconciliation run
  - reconciliation outcome
  - reconciliation receipt
  - exception case
  - exception evidence
  - exception state transitions
- Define normalized top-level reconciliation outcomes:
  - `queued`
  - `collecting_observations`
  - `matching`
  - `matched`
  - `partially_matched`
  - `unmatched`
  - `stale`
  - `manual_review_required`
  - `resolved`
- Define normalized exception categories:
  - `observation_missing`
  - `state_mismatch`
  - `amount_mismatch`
  - `destination_mismatch`
  - `delayed_finality`
  - `duplicate_signal`
  - `external_state_unknown`
  - `policy_violation`
  - `manual_review_required`
- Define severity scale:
  - `info`
  - `warning`
  - `high`
  - `critical`
- Define the adapter-neutral reconciliation rule-pack interface:
  - build expected facts
  - collect observed facts
  - match
  - classify
  - emit reconciliation result

### Deliverables

- Reconciliation contract spec v1
- Exception taxonomy spec v1
- Reconciliation rule-pack interface spec
- Sample Solana mappings against generic contracts
- ADR: `Generic reconciliation framework with adapter-specific rule packs`

### Done Means

Milestone 1 is done when:

- a future EVM, email, or Slack-style adapter could implement the same framework-level reconciliation interface without changing the framework
- Solana-specific language does not leak into generic reconciliation contracts
- exception categories are stable enough for v1 shipment
- the team can explain exactly what `matched` means versus `execution succeeded`
- the contracts are written, versioned, and referenced by the implementation roadmap

### Failure Mode If Skipped

If this milestone is done poorly, the framework becomes Solana-shaped, future adapters become painful, and exception handling degenerates into case-by-case special logic.

## Current Repo Mapping

The current Azums implementation already has the right insertion points for this roadmap:

- execution truth in `execution_core_*`
- delivery truth in `callback_core_*`
- tenant-scoped query truth in `status_api`
- adapter evidence in `adapter_solana` plus receipt details

The roadmap therefore extends the system downstream. It does not justify moving lifecycle truth out of core.
