# Primitive Correctness Audit

This page is the M2 audit ledger. Each primitive is tracked from definition to invariant, implementation, tests, failure behavior, and backend coverage.

Legend:

- **Unit** means a deterministic core or backend-local test.
- **Integration** means the primitive is exercised through `StorageBackend`, `QuickstartFlow`, repository APIs, or a real backend.
- **Concurrency** means multiple workers, ordering, leasing, or contention is covered.
- **Failure** means invalid input, illegal transition, crash, timeout, DLQ, cancellation, replay, or recovery is covered.
- Backend coverage uses `PG`, `SQLite`, `Redis`, `Memory`.

## Job Primitives

| Primitive | Definition | Invariant | Implementation | Test evidence | Backend coverage |
|---|---|---|---|---|---|
| Job identity | Every job has a unique `Uuid` identity. | Job IDs are immutable and identify one durable work item. Replay creates a new ID. | `Job::new`, backend `enqueue`, `replay_job` | Unit: `test_core_job_creation_and_typed_payload`; integration: `replay_creates_new_job_with_lineage`; failure: missing job errors in replay/cancel paths | PG, SQLite, Redis, Memory |
| Job type | Handler routing key. | A job type is preserved from enqueue through attempts and replay. | `Job.job_type`, `NewJob.job_type` | Unit: core job creation; integration: quickstart/API audit; failure: unknown job type routes to retry/DLQ through runner classification | PG, SQLite, Redis, Memory |
| Payload | JSON work input. | Payload is stored unchanged and typed decoding failures are explicit errors. | `payload_json`, `Job::payload_typed` | Unit: payload typed success/failure; integration: quickstart handlers | PG, SQLite, Redis, Memory |
| Metadata | Operational fields: replay lineage, error summary, DLQ reason, timestamps, locks. | Metadata is written only by the transition that owns it. | `Job` fields, `JobAttempt`, timeline | Integration: timeline, DLQ, replay, maintenance; failure: invalid transitions reject writes | PG, SQLite, Redis, Memory |
| Priority | Leasing precedence within eligible jobs. | Higher priority leases before lower priority, then scheduling/order policy applies. | backend lease ordering | Integration/concurrency: `leasing_respects_priority_then_run_at`, FIFO ordering tests | PG, SQLite, Redis, Memory |
| Scheduling | `run_at` controls eligibility. | Future jobs are not leased before `run_at <= now()`. | `enqueue_at`, `enqueue_in`, lease filters | Integration/failure: `delayed_job_is_not_leased_before_run_at`, `scheduled_job_is_not_leased_early_and_is_leased_after_run_at` | PG, SQLite, Redis, Memory |

## Execution Primitives

| Primitive | Definition | Invariant | Implementation | Test evidence | Backend coverage |
|---|---|---|---|---|---|
| Claim | Atomic move from eligible queued work to running ownership. | A job is claimed by at most one worker at a time. | `lease_jobs_batch`, `dequeue_and_lease` | Concurrency: `leasing_two_workers_never_claim_same_job`, high concurrency workers | PG, SQLite, Redis, Memory |
| Lease | Running ownership with worker ID and expiration. | Only the owning worker may finish, retry, DLQ, heartbeat, or cancel a running job. | `locked_by`, `lock_expires_at`, transition guards | Unit/failure: terminal and wrong-worker rejection tests; integration: leasing and retry tests | PG, SQLite, Redis, Memory |
| Heartbeat | Extend an active lease. | Lease extension succeeds only while the caller still owns the running job. | `extend_lease` | Integration/failure: heartbeat/phantom recovery tests | PG, SQLite, Redis, Memory |
| ACK | Successful execution acknowledgment. | ACK is legal only from a running attempt owned by the worker; terminal completion is final. | `mark_succeeded`, `mark_succeeded_batch` | Unit/failure: terminal transition rejection; integration: lifecycle tests | PG, SQLite, Redis, Memory |
| Retry | Failed retryable, timeout, or system execution scheduled for another attempt. | Retry records failed attempt data and moves job to retry wait/queued future state until `max_attempts` is exhausted. | `reschedule_for_retry`, `RetryConfig`, failure classification, `JobRunner::on_failure` | Unit/integration/failure: deterministic backoff, typed failure classes, retries, DLQ exhaustion, storm control | PG, SQLite, Redis, Memory |
| Timeout | Expired lease recovery. | Expired running jobs can become queued again for at-least-once recovery. | `reap_expired_locks` | Integration/failure: lease expiry, worker crash, phantom recovery | PG, SQLite, Redis, Memory |
| Cancel | Explicit cancellation. | Queued/scheduled jobs may be cancelled; running cancellation requires the owning worker; terminal jobs reject cancellation. | `cancel_job` | Unit/failure: cancellation ownership and terminality test | PG, SQLite, Redis, Memory |

## Durability Primitives

| Primitive | Definition | Invariant | Implementation | Test evidence | Backend coverage |
|---|---|---|---|---|---|
| Transaction | Atomic persistence boundary for a backend operation. | Partial transition writes must not represent success. SQL transactional enqueue keeps app state and job state together across successful transaction boundaries. | SQL transactions, `enqueue_in_tx`, Redis atomic list/hash commands, memory write lock | Integration/failure: transactional enqueue commit, rollback, commit-failure, connection-loss, and process-termination tests | PG, SQLite; Redis/Memory have scoped atomicity only |
| Persistence | Job and attempt state survive beyond handler call boundaries. | The job row and attempt row are the source of truth. | `jobs`, `job_attempts`, Redis hashes/lists, memory state | Integration: attempts, timeline, maintenance | PG, SQLite, Redis, Memory |
| Recovery | Reclaim abandoned running work. | Recovered jobs are re-eligible only after lease expiry. | `reap_expired_locks` | Failure: worker crash, phantom recovery | PG, SQLite, Redis, Memory |
| Replay | Create new work from old work or read stream history. | Replay creates new work and preserves original history. | `replay_job`, `read_events` | Integration: replay tests, stream replay tests | PG, SQLite, Redis, Memory |
| DLQ inspection | Inspect terminal failed work. | Original job row, payload, attempt history, workers, errors, timestamps, and reason code remain reconstructable until retention removes them. | `get_job`, `job_attempts`, timeline, replay | Integration/failure: DLQ inspection and replay | PG |
| Idempotency | Duplicate side-effect protection belongs to the application. | Azums does not deduplicate external effects; it exposes IDs and sequence numbers for dedupe. | documented semantics, `sequence_no`, job IDs | Unit/integration: stream offset monotonic tests; failure semantics documented in `semantics.md` | PG, SQLite, Redis, Memory |

## Coordination Primitives

| Primitive | Definition | Invariant | Implementation | Test evidence | Backend coverage |
|---|---|---|---|---|---|
| Workers | Worker identity owns leases and attempts. | Mutating running work requires the owning worker. | `Worker`, `locked_by`, attempt `worker_id` | Unit/failure: wrong-worker cancellation and transition rejection; integration: leasing tests | PG, SQLite, Redis, Memory |
| Concurrency | Multiple workers process the same queue safely. | No duplicate active claim for one job. | `FOR UPDATE SKIP LOCKED`, SQLite transactions, Redis `LMOVE`, memory lock | Concurrency: multi-worker FIFO, high concurrency, leasing no duplicates | PG, SQLite, Redis, Memory |
| Consumer groups | Stream offset coordination by group. | Offsets advance monotonically and never move backward. | `stream_offsets`, Redis hash, memory map | Integration: stream ack and consumer group tests | PG, SQLite, Redis, Memory |
| Partitioning | Dataset routing for hot job sets. | Job rows route by queue and scheduled time bucket where backend supports partitions. | `dataset_id_for`, partition migrations | Integration: replay/lease tests preserve dataset behavior; docs define backend limits | PG primary, SQLite/Redis/Memory use default dataset |
| Backpressure | Protect queues from overload. | Policy gates throttle without losing jobs. | queue policies, policy decisions | Integration/failure: storm control and policy timeline tests | PG primary; other backends rely on caller-side throttling |

## Event Primitives

| Primitive | Definition | Invariant | Implementation | Test evidence | Backend coverage |
|---|---|---|---|---|---|
| Append | Add event to stream log. | Sequence numbers increase monotonically per stream. | `publish` | Integration: stream publish/read tests | PG, SQLite, Redis, Memory |
| Offset | Consumer progress marker. | Acknowledgment is monotonic. | `ack`, `consumer_group_info` | Integration: stream ack tests | PG, SQLite, Redis, Memory |
| Subscribe | Wake consumers on new events. | Notifications are hints; durable state is read from storage. | `subscribe_stream` | Integration: stream/pubsub and wake-up tests | PG, SQLite, Redis, Memory |
| Replay | Read historical events by sequence. | Replay does not mutate offsets unless the consumer ACKs. | `read_events(after_seq, limit)` | Integration: stream replay tests | PG, SQLite, Redis, Memory |

## Backend Coverage Summary

| Backend | Jobs | Execution | Durability | Coordination | Events |
|---|---|---|---|---|---|
| PostgreSQL | Covered | Covered | Strong SQL transaction semantics | Strongest concurrency and partition coverage | Covered |
| SQLite | Covered | Covered | Embedded SQL transaction semantics | Single-writer coordination | Covered |
| Redis | Covered | Covered | Redis command atomicity, no SQL transaction coupling | Redis list/hash coordination | Covered |
| In-Memory | Covered | Covered | Process-local durability only | Process-local lock coordination | Covered |

For the runtime capability contract behind this table, see [Storage Backend Equivalence](backend_equivalence.md).

## Audit Rule

If a primitive is added or behavior changes, update this page in the same change as the implementation and tests. A primitive is not complete until its definition, invariant, implementation, test evidence, failure behavior, and backend coverage are all explicit.
