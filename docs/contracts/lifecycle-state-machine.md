# Lifecycle State Machine

## Canonical States
| State | Meaning |
|---|---|
| `received` | Intake accepted request boundary |
| `validated` | Contract/schema/auth checks passed |
| `rejected` | Intake rejected request |
| `queued` | Durable work ready for leasing |
| `leased` | Worker currently owns execution attempt |
| `executing` | Core has dispatched adapter |
| `retry_scheduled` | Retryable failure has next attempt scheduled |
| `succeeded` | Execution completed successfully |
| `failed_terminal` | Terminal failure (no auto retry) |
| `dead_lettered` | Retry policy exhausted |
| `replayed` | Replay path created from previous lineage |

## Transition Guidance
| From | To | Rule |
|---|---|---|
| `received` | `validated` or `rejected` | Intake validation/auth decision |
| `validated` | `queued` | Durable enqueue completes |
| `queued` | `leased` | Worker lease granted |
| `leased` | `executing` | Core begins adapter dispatch |
| `executing` | `succeeded` | Adapter outcome classified successful |
| `executing` | `retry_scheduled` | Retryable failure classification |
| `executing` | `failed_terminal` | Terminal classification |
| `retry_scheduled` | `leased` | Retry attempt lease |
| `failed_terminal` | `replayed` | Authorized replay lineage creation |
| `retry_scheduled` | `dead_lettered` | Retry budget exhausted |

## Illegal Transition Examples
| Illegal Transition | Why Illegal |
|---|---|
| `queued` -> `succeeded` | Skips lease/execute stages |
| `rejected` -> `executing` | Rejected requests are non-executable |
| `succeeded` -> `retry_scheduled` | Terminal success cannot be retried automatically |
| Adapter-invented state | Core state vocabulary must remain canonical |

## Policy Notes
| Policy | Description |
|---|---|
| Retry policy | Core-owned, adapter only signals retryability |
| Replay policy | Core validates eligibility and preserves lineage |
| Manual action policy | Strictly authorized and auditable |

