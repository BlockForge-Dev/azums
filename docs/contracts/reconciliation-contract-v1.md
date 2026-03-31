# Reconciliation Contract v1

## Objective

Define an adapter-neutral reconciliation framework that consumes durable Azums execution truth and compares it against observed facts without replacing execution truth.

## Core Definitions

### Reconciliation Subject

A reconciliation subject is the immutable identity of what is being reconciled.

| Field | Type | Meaning |
|---|---|---|
| `recon_subject_id` | string | Unique reconciliation identity |
| `tenant_id` | string | Hard tenant boundary |
| `intent_id` | string | Execution intent lineage root |
| `job_id` | string | Specific execution attempt under review |
| `adapter_id` | string | Adapter that produced the committed result |
| `execution_state` | string | Canonical execution state at recon start |
| `execution_classification` | string | Core classification at recon start |
| `source_receipt_cursor` | string optional | Watermark or receipt linkage used to derive expected facts |
| `created_at_ms` | uint64 | Recon subject creation time |

### Expected Facts

Expected facts are derived from committed Azums truth.

| Field | Type | Meaning |
|---|---|---|
| `subject_id` | string | Parent reconciliation subject |
| `facts` | map<string,string> or structured object array | What Azums expects to be true |
| `derived_from` | object | receipt/transition/job references used to build expectation |
| `version` | string | Rule-pack contract version |
| `built_at_ms` | uint64 | Build timestamp |

### Observed Facts

Observed facts are collected from downstream durable evidence or external observation connectors.

| Field | Type | Meaning |
|---|---|---|
| `subject_id` | string | Parent reconciliation subject |
| `facts` | map<string,string> or structured object array | What was externally observed |
| `observation_source` | string | connector or store name |
| `observed_at_ms` | uint64 | Observation time |
| `fresh_until_ms` | uint64 optional | staleness boundary |
| `raw_evidence_refs` | array<string> | links to evidence rows, signatures, provider refs, or external IDs |

### Reconciliation Run

| Field | Type | Meaning |
|---|---|---|
| `recon_run_id` | string | Run identity |
| `subject_id` | string | Reconciliation subject |
| `rule_pack_id` | string | Adapter-neutral framework rule pack identity |
| `rule_pack_version` | string | Versioned rules |
| `started_at_ms` | uint64 | Run start |
| `completed_at_ms` | uint64 optional | Run completion |
| `status` | enum | Top-level reconciliation outcome |

### Reconciliation Outcome

| Outcome | Meaning |
|---|---|
| `queued` | Reconciliation subject created but collection has not started |
| `collecting_observations` | Observation collection is in progress |
| `matching` | Expected and observed facts are being matched |
| `matched` | Expected facts materially agree with observed facts |
| `partially_matched` | Some expected facts match, some diverge |
| `unmatched` | Material divergence exists |
| `stale` | Observations are too old or incomplete to trust |
| `manual_review_required` | Automated matching produced a case that must be reviewed |
| `resolved` | A previously divergent subject is now closed by a later recon or operator action |

### Reconciliation Receipt

The reconciliation receipt is the explainability object for a recon run.

| Field | Type | Meaning |
|---|---|---|
| `recon_run_id` | string | Parent run |
| `subject_id` | string | Parent subject |
| `entries[]` | array | Ordered timeline of collection, match, and classification events |
| `final_outcome` | enum | Terminal recon outcome for this run |

## Rule-Pack Interface v1

The framework stays generic. Adapters supply rule packs.

### Required Framework Interface

| Stage | Contract |
|---|---|
| Build expected facts | `build_expected_facts(subject, execution_snapshot) -> ExpectedFacts` |
| Collect observed facts | `collect_observed_facts(subject, collection_context) -> ObservedFacts` |
| Match | `match(expected, observed) -> MatchResult` |
| Classify | `classify(match_result) -> ReconClassification` |
| Emit result | `emit_recon_result(subject, run, classification, receipt) -> durable recon records` |

### Framework Constraints

- rule packs must not mutate execution history
- rule packs must not redefine framework-level reconciliation outcomes
- rule packs may add adapter-specific evidence keys, but not adapter-specific top-level outcome enums
- rule packs must be versioned

## Matched Versus Execution Succeeded

These terms are intentionally different.

| Term | Meaning |
|---|---|
| `execution succeeded` | Azums core committed a successful execution outcome |
| `matched` | Downstream observations agree with what Azums expected after execution |

Examples:

- execution may succeed while reconciliation is `stale` because no trustworthy external observation exists yet
- execution may succeed while reconciliation is `unmatched` because the observed amount or destination diverges
- execution may be `failed_terminal` while reconciliation is still `matched` if the observed external reality agrees that no terminal success occurred

## Solana Mapping Examples

These examples are Solana-first, not Solana-shaped framework rules.

### Example: Transfer Finalized As Expected

Expected facts may include:

- expected destination
- expected amount
- expected signature or final tx reference
- expected provider/network context

Observed facts may include:

- observed final signature
- observed destination
- observed amount
- observed finality state

Outcome:

- `matched`

### Example: Submitted But Delayed Finality

Expected facts:

- signature exists
- finality should eventually advance

Observed facts:

- signature present
- finality not yet advanced within policy window

Outcome:

- `stale` or `manual_review_required` depending on policy and age

### Example: Destination Mismatch

Expected facts:

- destination `A`

Observed facts:

- destination `B`

Outcome:

- `unmatched`
- exception category `destination_mismatch`

## Versioning Rules

- framework contract versions independently from rule-pack versions
- breaking framework changes require a new contract version
- rule-pack changes that affect classification semantics require a version bump and migration note
