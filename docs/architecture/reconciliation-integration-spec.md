# Reconciliation Integration Specification

## Purpose

Freeze the current Azums integration shape before reconciliation and exception intelligence are implemented.

The goal is not to redesign execution core. The goal is to define where downstream reconciliation and exception subsystems attach without weakening current boundaries.

## Truth Domains

| Domain | Owned By | Meaning | Must Not Do |
|---|---|---|---|
| Execution truth | `execution_core` + durable core tables | Canonical lifecycle, attempt lineage, durable receipts, replay lineage | Be overwritten by recon or exception processing |
| Delivery truth | `callback_core` tables | Outbound delivery state and attempt history | Redefine execution success/failure |
| Reconciliation truth | future `recon_core` | Expected vs observed fact matching over already-committed execution truth | Replace or silently mutate execution history |
| Exception truth | future `exception_intelligence` | Divergence classification, severity, evidence, and case workflow | Rewrite core or recon records without explicit operator action |

## Current Azums Component Map

| Subsystem | Current Implementation | Owns | Must Not Own |
|---|---|---|---|
| Reverse proxy | `crates/reverse-proxy` | Edge routing, request filtering, public surface control | Business execution, reconciliation, exception classification |
| Ingress | `apps/ingress_api` | Authentication, validation, normalization, durable submission, intake audits | Long-running execution, reconciliation decisions |
| PostgresQ + durable store | `crates/execution_core::integration::postgresq` | Intent/job storage, transitions, receipts, replay decisions, dispatch queue integration | Adapter-specific recon logic |
| Execution core | `crates/execution_core` | Lifecycle truth, routing, classification, retry/replay policy | External observation matching |
| Solana adapter | `crates/adapter_solana` | Solana-specific execution and evidence capture | Framework-level reconciliation semantics |
| Delivery core | `crates/callback_core` | Delivery state, retries, attempt history, destination management | Execution truth or reconciliation truth |
| Status/query layer | `crates/status_api` | Tenant-scoped reads, replay endpoint, callback reads, intake audit reads | Silent execution mutation |
| Customer/operator UI backends | `apps/operator_ui` and `apps/operator_ui_next` | Human-facing query and controlled command surfaces | Direct lifecycle mutation outside API contracts |

## Current Lifecycle Truth Map

### Canonical Execution Lifecycle

These are the current top-level execution states owned by `execution_core`.

| State | Meaning | Durable Surface |
|---|---|---|
| `received` | Intake boundary reached and request accepted into core flow | `execution_core_jobs`, `execution_core_state_transitions`, `execution_core_receipts` |
| `validated` | Contract checks passed | same |
| `rejected` | Intake rejected and non-executable | same |
| `queued` | Durable work scheduled for leasing | same |
| `leased` | Worker owns the next attempt | same |
| `executing` | Core dispatched adapter execution | same |
| `retry_scheduled` | Retryable failure scheduled for later | same |
| `succeeded` | Canonical successful completion | same |
| `failed_terminal` | Terminal failure without auto-retry | same |
| `dead_lettered` | Retry policy exhausted | same |
| `replayed` | Replay lineage created | same plus `execution_core_replay_decisions` |

### Solana Evidence Phases

Solana-specific terms such as `submitted`, `confirmed`, and `finalized` are not a second platform lifecycle. They are adapter evidence and observed-chain facts.

| Solana Fact | Current Representation | Owner |
|---|---|---|
| submitted | receipt/details metadata, adapter outcome details, Solana attempt rows | `adapter_solana` plus core receipt write |
| confirmed | Solana adapter polling evidence and result details | `adapter_solana` |
| finalized | Solana adapter finality evidence and final signature fields | `adapter_solana` |
| signature / tx hash | receipt details and `solana.tx_*` rows | `adapter_solana` |
| provider / rpc provenance | `provider_used`, `rpc_url`, `rpc_urls`, `simulation_outcome` details | `adapter_solana` |

This distinction matters:

- execution truth answers: `what did Azums classify and commit?`
- reconciliation answers: `does external reality still agree with the committed expectation?`

### Callback Delivery Lifecycle

These states live in `callback_core` and remain separate from execution outcome.

| State | Meaning | Durable Surface |
|---|---|---|
| `queued` | Delivery job published after durable execution commit | `callback_core_deliveries` |
| `delivering` | Delivery worker currently owns the callback | `callback_core_deliveries` |
| `retry_scheduled` | Delivery retry scheduled | `callback_core_deliveries` |
| `terminal_failure` | Delivery exhausted or failed terminally | `callback_core_deliveries`, `callback_core_delivery_attempts` |
| `delivered` | Delivery completed | `callback_core_deliveries`, `callback_core_delivery_attempts` |

## Persisted Execution and Evidence Surfaces

| Surface | Current Table / Store | Kind of Truth | Candidate Recon Use |
|---|---|---|---|
| Intake audit | `ingress_api_intake_audits` | intake decision truth | optional evidence for expected-fact provenance |
| Normalized intent | `execution_core_intents` | accepted request truth | recon subject seed |
| Execution job | `execution_core_jobs` | current canonical job state | recon subject seed and lifecycle trigger |
| State transitions | `execution_core_state_transitions` | append-only lifecycle history | expected-fact build input |
| Receipts | `execution_core_receipts` | explainability timeline | expected-fact build input |
| Replay decisions | `execution_core_replay_decisions` | replay authorization lineage | exception and recon lineage context |
| Callback delivery | `callback_core_deliveries` | delivery truth | downstream exception correlation only |
| Callback attempts | `callback_core_delivery_attempts` | delivery attempt evidence | downstream exception correlation only |
| Solana intent evidence | `solana.tx_intents` | adapter-local durable evidence | observed facts source |
| Solana attempt evidence | `solana.tx_attempts` | adapter-local durable evidence | observed facts source |
| Query audit | `status_api_query_audit` | operator/query audit | not recon input; investigation trace only |
| Operator action audit | `status_api_operator_action_audit` | controlled action audit | case workflow evidence only |

## Approved Downstream Hook Points

Reconciliation must attach only after durable execution writes.

| Hook Point | Current Source | Why It Is Safe | Initial Integration Pattern |
|---|---|---|---|
| After receipt write | `execution_core_receipts` append | receipt already reflects committed core truth | table-driven recon intake by watermark |
| After state transition write | `execution_core_state_transitions` append | canonical state already committed | table-driven recon intake by transition filter |
| After recon-intake signal write | `platform_recon_intake_signals` append | signal is emitted only after durable receipt/transition/callback truth is written | primary downstream recon intake path |
| After job update on semi-terminal/terminal transitions | `execution_core_jobs.updated_at_ms` plus canonical state | efficient subject discovery for `succeeded`, `failed_terminal`, `dead_lettered`, `replayed` | recon subject scheduler |
| After adapter evidence persistence | `solana.tx_intents` / `solana.tx_attempts` | observed facts already durable | adapter rule-pack observation collector |
| After callback delivery updates | `callback_core_deliveries` / attempts | delivery divergence may deserve exception cases | exception correlation only, not execution mutation |

## Current Milestone 2 Intake Shape

Milestone 2 introduces a dedicated downstream intake table:

- `platform_recon_intake_signals`

Signal kinds currently emitted:

- `submitted_with_reference`
- `adapter_completed`
- `terminal_failure`
- `finalized`
- `callback_committed`

Ownership rules:

- `execution_core` writes execution-side signals after committed receipt writes
- `callback_core` writes delivery-side verification signals after committed callback publication
- `recon_core` consumes the signal table first, then may use legacy table scans for backfill compatibility

## Recommended Initial Intake Model

Initial implementation should be table-driven, not event-bus-first.

### Recommended Recon Intake

- poll committed rows using watermark-based readers
- derive reconciliation subjects from `execution_core_jobs` joined with latest receipt and transition context
- collect observed facts from adapter-local tables or external observation connectors
- write new reconciliation records into recon-owned tables

### Why Table-Driven First

- current Azums truth is already durable and queryable in Postgres
- the integration shape stays explicit and debuggable
- reconciliation can be replayed deterministically from stored truth
- no new bus dependency is required to begin

## Hard Boundaries for New Subsystems

### Recon Core

`recon_core` owns:

- reconciliation subject creation
- expected-fact construction from durable execution truth
- observed-fact collection
- fact matching
- reconciliation receipt emission

`recon_core` must never:

- write back a new canonical execution state into `execution_core_*`
- silently rewrite receipt history
- decide replay policy for core
- reinterpret callback delivery as execution truth

### Exception Intelligence

`exception_intelligence` owns:

- exception case creation
- divergence classification
- severity assignment
- evidence indexing
- manual-review workflow state

`exception_intelligence` must never:

- mutate core execution state implicitly
- mark reconciliation as matched without a recorded recon run
- suppress history without durable evidence and audit

### Query / Read Models

Downstream read models may compose:

- execution truth
- reconciliation truth
- exception truth
- delivery truth

But they must remain read models. They are not the system of record for execution.

## Ownership Matrix

| Subsystem | Owns | Must Not Own |
|---|---|---|
| `execution_core` | execution lifecycle truth | reconciliation decisions |
| `callback_core` | delivery lifecycle truth | execution lifecycle truth |
| `recon_core` | expected vs observed fact matching | core lifecycle mutation |
| `exception_intelligence` | divergence cases and severity | silent historical mutation |
| `status_api` | query and authorized commands | hidden reconciliation-side writes into core |
| UI | visualization and explicit user actions | inferred truth |

## Milestone 0 Completion Criteria

Milestone 0 is complete when:

- execution truth write points are explicitly identified
- reconciliation intake sources are explicitly identified
- exception intelligence boundaries are explicitly identified
- the team can explain the separation among execution truth, reconciliation truth, and exception truth without ambiguity

## Implementation Note

The current Azums system is already shaped correctly for this work.

The right move is to add downstream bounded subsystems, not to reopen execution core and blend reconciliation into lifecycle ownership.
