# Data Flow

## Flow A: Inbound Execution
| Step | Action | Persisted Effect |
|---|---|---|
| 1 | Client calls API/webhook | Request enters public boundary |
| 2 | Reverse proxy routes to ingress | Edge checks applied |
| 3 | Ingress authenticates, validates, normalizes | Intake audit + intent persistence |
| 4 | Worker/core leases job | Lease and attempt context |
| 5 | Core selects adapter and dispatches | Executing transition recorded |
| 6 | Adapter executes and returns structured result | Outcome metadata captured |
| 7 | Core classifies and writes canonical state | Transition + receipt + retry/terminal decision |
| 8 | Callback core handles outward notification | Delivery attempt history |
| 9 | Status API/UI query complete journey | Read model reflects durable truth |

## Flow B: Retry
| Step | Action |
|---|---|
| 1 | Adapter returns retryable failure |
| 2 | Core marks `retry_scheduled` and computes next retry time |
| 3 | Retry schedule is persisted |
| 4 | Worker later re-leases job |
| 5 | Adapter re-executes under new attempt |
| 6 | Receipt/history show attempt sequence and state path |

## Flow C: Terminal Failure
| Step | Action |
|---|---|
| 1 | Adapter returns terminal failure classification |
| 2 | Core transitions to `failed_terminal` |
| 3 | Receipt captures where/why and remediation posture |
| 4 | Callback core may deliver failure callback |
| 5 | Status/API/UI display durable terminal classification |

## Flow D: Replay
| Step | Action |
|---|---|
| 1 | Authorized operator/user requests replay |
| 2 | Status API performs authz checks |
| 3 | Core validates replay eligibility |
| 4 | New replay record created and linked to source lineage |
| 5 | New execution scheduled via queue/store |
| 6 | Query layer shows both original and replayed paths |

