# Concurrency, Ordering & Backpressure

M8 defines how Azums behaves when many workers and producers contend for the same queues.

## Guarantees

| Behavior | Contract |
|---|---|
| Active claim exclusivity | One runnable job can be leased by at most one worker at a time. |
| Running ownership | ACK, retry, DLQ, heartbeat, and running cancellation require the worker that owns the lease. |
| Queue isolation | Workers lease only from the queue name they request. |
| Priority | Higher-priority runnable jobs lease before lower-priority runnable jobs. |
| Scheduling | Future jobs are not eligible until `run_at <= now()`. |
| Durable visibility | A committed job is either queued, running under a lease, terminal, or recoverable after lease expiry. |

## Non-Guarantees

Azums does not guarantee:

- Exactly-once handler execution.
- Exactly-once external side effects.
- Equal distribution of jobs across workers.
- Completion order across multiple workers.
- Global ordering across different queues.
- Automatic producer throttling, job shedding, or autoscaling.

## Ordering

FIFO ordering is a lease-order guarantee. It means eligible jobs are selected by priority, then schedule time, then creation order where the backend supports that ordering.

FIFO does not mean strict completion order once multiple workers are executing jobs concurrently. A later-leased job can finish before an earlier-leased job if its handler is faster.

## Backpressure Modes

`BackendCapabilities::backpressure` declares the overload behavior.

| Mode | Meaning | Backends |
|---|---|---|
| `BacklogOnly` | Accepted jobs remain queued until workers can lease them. Overload increases backlog. Azums does not drop, reject, block, or auto-scale by itself. | Memory, SQLite, Redis |
| `ExecutionRateLimit` | The backend can throttle worker leases through queue policy gates without losing jobs. Throttled jobs stay queued, are deferred with a later `run_at`, and produce observable policy decisions. | PostgreSQL |

For a producer rate of 100k jobs/sec and a consumer ACK rate of 10k jobs/sec:

- In `BacklogOnly`, Azums accepts successful enqueues and backlog grows by about 90k jobs/sec.
- In PostgreSQL `ExecutionRateLimit`, queue policies can cap execution pressure; jobs are still preserved as queued work.
- In all modes, Azums does not silently shed committed jobs.

Producer-side behavior such as blocking, rejecting, rate-limiting HTTP requests, shedding optional work, or scaling workers is an application policy. Azums exposes the backend capability and persisted queue state so that policy can be implemented explicitly.

## Stress Harness

The normal test suite runs the M8 worker matrix with a CI-sized job count:

```powershell
cargo test -p azums --test m8_concurrency_backpressure
```

The large matrix is available as an ignored stress test:

```powershell
$env:AZUMS_M8_STRESS = "1"
$env:AZUMS_M8_JOB_COUNTS = "10000,50000,100000,1000000"
$env:AZUMS_M8_WORKERS = "1,2,5,10,50,100"
cargo test -p azums --test m8_concurrency_backpressure -- --ignored --nocapture
```

The stress test validates that each matrix cell drains all jobs, rejects duplicate active claims, and leaves no queued or running residue.
