# Exception Taxonomy v1

## Objective

Define the durable exception model used to classify and index divergence discovered by reconciliation without silently mutating execution truth.

## Exception Model

### Exception Case

| Field | Type | Meaning |
|---|---|---|
| `exception_case_id` | string | Durable case identity |
| `tenant_id` | string | Hard tenant boundary |
| `recon_subject_id` | string | Parent reconciliation subject |
| `recon_run_id` | string | Run that emitted the case |
| `category` | enum | Normalized divergence category |
| `severity` | enum | Normalized severity scale |
| `state` | enum | Case workflow state |
| `summary` | string | Human-readable case title |
| `dedupe_key` | string | Stable subject-scoped key for replay-safe upsert |
| `cluster_key` | string | Search/grouping key for similar divergence |
| `occurrence_count` | uint64 | Number of times the same case has reappeared |
| `opened_at_ms` | uint64 | Case open time |
| `updated_at_ms` | uint64 | Latest state-change time |
| `resolved_at_ms` | uint64 optional | Resolution time if closed out |

### Exception Evidence

| Field | Type | Meaning |
|---|---|---|
| `evidence_id` | string | Evidence identity |
| `exception_case_id` | string | Parent case |
| `evidence_type` | string | receipt ref, transition ref, callback ref, observation ref, external snapshot, etc. |
| `evidence_ref` | string | durable row key, signature, callback id, provider ref, URL, or blob pointer |
| `captured_at_ms` | uint64 | Evidence capture time |
| `details` | object | structured evidence payload or summary |

## Normalized Categories

| Category | Meaning |
|---|---|
| `observation_missing` | recon could not obtain a trustworthy observation |
| `state_mismatch` | observed state disagrees with expected state |
| `amount_mismatch` | observed amount differs materially from expected amount |
| `destination_mismatch` | observed destination differs materially from expected destination |
| `delayed_finality` | expected finality has not arrived within policy window |
| `duplicate_signal` | duplicate external signal or multiple competing observations exist |
| `external_state_unknown` | external source returned ambiguous or unrecognized state |
| `policy_violation` | observation contradicts an explicit platform or tenant policy |
| `manual_review_required` | automation intentionally escalates to human review |

## Severity Scale

| Severity | Meaning |
|---|---|
| `info` | no immediate operational action required |
| `warning` | divergence exists but is low-risk or likely transient |
| `high` | material divergence requiring timely review |
| `critical` | high-risk divergence with customer, financial, or policy exposure |

## Exception State Machine v1

| State | Meaning |
|---|---|
| `open` | Case has been created and awaits handling |
| `acknowledged` | An operator has seen the case and taken ownership of first response |
| `investigating` | An operator or system workflow is actively reviewing it |
| `resolved` | Case is resolved by later observation or explicit operator action |
| `dismissed` | Case is intentionally closed without asserting that the divergence was valid |
| `false_positive` | Case was raised but later determined to be non-actionable or incorrect |

### State Rules

- state changes are append-friendly and auditable
- dismissal does not delete evidence
- resolution does not rewrite the originating recon or execution history
- terminal cases may be reopened through an explicit workflow transition rather than mutating history

## Classification Guidance

| Situation | Category | Typical Severity |
|---|---|---|
| no trusted chain/provider observation yet | `observation_missing` | `warning` |
| observed chain state differs from expected terminal state | `state_mismatch` | `high` |
| observed amount differs from expected amount | `amount_mismatch` | `critical` |
| observed destination differs from expected destination | `destination_mismatch` | `critical` |
| expected finality window exceeded | `delayed_finality` | `warning` or `high` |
| duplicate callback or duplicate chain observation | `duplicate_signal` | `info` or `warning` |
| provider returns unknown state code | `external_state_unknown` | `warning` |
| observed behavior violates configured policy | `policy_violation` | `high` or `critical` |
| automation cannot safely conclude | `manual_review_required` | `high` |

## Boundary Rules

- exception intelligence may classify divergence
- exception intelligence may create cases and evidence
- exception intelligence may propose operator actions

But:

- exception intelligence does not change `execution_core_*` truth directly
- exception intelligence does not mark reconciliation `matched` without a recon run
- exception intelligence does not delete contradictory history

## Solana-First Examples

### Delayed Finality

- execution truth: succeeded with signature recorded
- observed fact: signature still not finalized after policy window
- exception category: `delayed_finality`
- severity: `warning` or `high`

### Wrong Destination

- execution truth: expected destination `A`
- observed fact: destination `B`
- exception category: `destination_mismatch`
- severity: `critical`

### Missing Observation

- execution truth: terminal receipt exists
- observed fact: no trustworthy external state reachable
- exception category: `observation_missing`
- severity: `warning`
