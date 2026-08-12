# Job Lifecycle State Machine

This page defines the canonical Azums job execution model. All code paths that mutate job state must fit this state machine.

## Canonical States

```text
SCHEDULED
    |
    v
QUEUED
    |
    v
RUNNING
    |----> COMPLETED
    |----> RETRY_WAIT ---> QUEUED
    |----> CANCELLED
    `----> DLQ
```

Azums persists compact storage statuses and derives the canonical state from persisted job and attempt rows:

| Canonical state | Persisted evidence |
|---|---|
| `SCHEDULED` | `jobs.status = 'queued'`, `jobs.run_at > now()`, and no prior failed attempt |
| `QUEUED` | `jobs.status = 'queued'` and `jobs.run_at <= now()` |
| `RUNNING` | `jobs.status = 'running'`, `locked_by`, `locked_at`, and `lock_expires_at` are set |
| `COMPLETED` | `jobs.status = 'succeeded'` |
| `RETRY_WAIT` | `jobs.status = 'queued'`, `jobs.run_at > now()`, and at least one failed attempt |
| `CANCELLED` | `jobs.status = 'canceled'` |
| `DLQ` | `jobs.status = 'dlq'`, `dlq_reason_code`, and `dlq_at` are set |

`failed` is not a canonical job state. Handler failures are represented by durable `JobAttempt` rows. A failed attempt moves the job either to `RETRY_WAIT` or `DLQ`.

## Legal Transitions

| From | To | Trigger | Persisted evidence |
|---|---|---|---|
| `SCHEDULED` | `QUEUED` | Backend clock reaches `run_at` | Same row becomes lease-eligible because `run_at <= now()` |
| `QUEUED` | `RUNNING` | Worker leases the job | Job row gets `status = 'running'`, `locked_by`, `locked_at`, `lock_expires_at` |
| `RUNNING` | `COMPLETED` | Handler returns success | Running `JobAttempt` finishes as `succeeded`; job becomes `succeeded` |
| `RUNNING` | `RETRY_WAIT` | Retryable handler failure and attempts remain | Running `JobAttempt` finishes as `failed`; job becomes `queued` with future `run_at` and error fields |
| `RETRY_WAIT` | `QUEUED` | Retry delay expires | Same row becomes lease-eligible because `run_at <= now()` |
| `RUNNING` | `CANCELLED` | Cooperative cancellation path owns the running execution | Job becomes `canceled` |
| `RUNNING` | `DLQ` | Non-retryable failure or exhausted attempts | Running `JobAttempt` finishes as `failed`; job becomes `dlq` with reason code |

Replay is not a state transition on the original job. Replay creates a new `QUEUED` or `SCHEDULED` job with `replay_of_job_id` pointing at the source job.

Lease expiry recovery is an operational recovery path from abandoned `RUNNING` back to `QUEUED`. It is legal only when `lock_expires_at <= now()` and represents at-least-once recovery after the worker failed to ACK the execution. Backends with durable attempt rows close any running attempt for that job as `failed` with `error_code = 'LEASE_EXPIRED'` before the job is re-queued.

## Illegal Transitions

Every transition not listed above is illegal. Important examples:

| Illegal transition | Why |
|---|---|
| `SCHEDULED` -> `RUNNING` | The job must first become lease-eligible as `QUEUED` |
| `QUEUED` -> `COMPLETED` | Completion requires a worker-owned running lease and durable attempt |
| `QUEUED` -> `DLQ` | DLQ requires a failed running attempt |
| `COMPLETED` -> any state | `COMPLETED` is terminal |
| `CANCELLED` -> any state | `CANCELLED` is terminal |
| `DLQ` -> any state | `DLQ` is terminal; replay creates a new job instead |
| `RETRY_WAIT` -> `RUNNING` | Retry delay must expire before leasing |
| `RUNNING` -> `QUEUED` without lease expiry or retry failure | Running work cannot be silently abandoned |

Backends reject completion, retry, DLQ, and attempt-start operations unless the job is currently `running` under the expected worker lease.

## Model Separation

| Model | Responsibility |
|---|---|
| `Job` | Durable work item: ID, type, queue, payload, metadata, priority, status, schedule, lease fields, timestamps, replay lineage, and error summary |
| `JobAttempt` | Durable handler attempt: attempt number, worker, start/end timestamps, status, latency, and error information |
| `JobExecution` | Runtime claim tying a job, attempt, worker, and lease together while work is in flight |
| `Worker` | Stable worker identity used for leases and attempts |
| `Queue` | Queue name plus ordering policy |
| `Event` | Append-only stream event with sequence number, stream name, type, payload, and timestamp |

## Proof Obligations

Azums must preserve these invariants:

- Invalid state transitions are rejected.
- Every successful state transition is observable from persisted job fields, attempt rows, or replay lineage.
- Every handler execution attempt is durable before the handler outcome is applied.
- Terminal states are terminal; completion, cancellation, and DLQ cannot be mutated back into active work.
- Job lifecycle can be reconstructed from persisted `jobs`, `job_attempts`, and replay lineage.
