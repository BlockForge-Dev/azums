# Execution Semantics

This page is the canonical contract for Azums behavior. Use the labels below when deciding whether a behavior is guaranteed by Azums, depends on the selected backend, or is intentionally unspecified.

## Classification Labels

| Label | Meaning |
|---|---|
| **Guaranteed** | Azums treats this as a product invariant across supported backends for the documented API. A regression should be treated as a bug. |
| **Backend-dependent** | The API exists, but the strength, durability, timing, isolation, retention, or wake-up behavior depends on the backend's declared `BackendCapabilities`. |
| **Unspecified** | Azums does not make a contract for this behavior. Applications must not rely on it unless they enforce it themselves. |

## Guarantee Summary

| Behavior | Classification | Contract |
|---|---|---|
| Job execution delivery | **Guaranteed** | Azums provides **at-least-once execution** for jobs that are successfully enqueued, runnable, not canceled, and have available workers. |
| Exactly-once external side effects | **Unspecified** | Azums does **not** guarantee exactly-once calls to external systems such as email, payment APIs, webhooks, LLM providers, or user handlers. |
| Scheduling | **Guaranteed** for eligibility; **backend-dependent** for wake-up latency | A job with `run_at` in the future is not eligible for leasing until `run_at <= now()` according to the backend clock. Azums does not guarantee execution exactly at `run_at`. |
| Retries | **Guaranteed** | Retryable, timeout, and system-failure classes are rescheduled until `max_attempts` is reached. Backoff and jitter are computed by Azums retry policy. |
| DLQ transition | **Guaranteed** | Permanent failures, panics, and exhausted retry budgets move the job to `dlq` with a reason code and timestamp where the backend supports persisted job metadata. |
| Idempotency | **Unspecified** | Azums does not deduplicate jobs or external side effects by payload, job type, stream payload, request ID, or business key. Applications must provide their own idempotency keys and dedupe storage. |
| Transactional enqueue | **Backend-dependent** | PostgreSQL and SQLite can participate in database transaction semantics. Redis and In-Memory enqueue are atomic inside their own backend operations but are not ACID transactions with the application database. |
| Job leasing exclusivity | **Guaranteed** | A runnable job is leased to at most one worker at a time. Expired leases can be reaped and made runnable again. |
| Worker crash recovery | **Guaranteed** after lease expiry/reap | If a worker dies after leasing and before ACK, the job can be retried after its lease expires and recovery runs. Backends with durable attempts record the abandoned attempt as `LEASE_EXPIRED`. The handler may already have performed partial external work. |
| Completion ordering | **Unspecified** across parallel workers | FIFO affects lease order. Azums does not guarantee completion order when multiple workers or batches execute concurrently. |
| Stream append | **Guaranteed** | Stream events are append-only through the stream API and receive monotonically increasing sequence numbers per stream. |
| Stream delivery | **Guaranteed** as at-least-once replay | Consumers can read events with `sequence_no > after_seq`. Unacknowledged events remain readable while retained by the backend. |
| Consumer-group offsets | **Guaranteed** | Acknowledgment advances a consumer group's offset monotonically; acknowledging a lower sequence number does not move the offset backward. |
| Replay | **Guaranteed** for jobs and streams through their APIs | Job replay creates a new queued job with lineage to the source job. Stream replay reads historical events after an offset. Replay does not undo, erase, or dedupe the original work. |
| Cancellation | **Guaranteed** | `cancel_job` cancels queued or scheduled jobs directly. Running jobs require the owning worker lease. Terminal jobs reject cancellation. |
| Notification delivery | **Backend-dependent** | LISTEN/NOTIFY, PubSub, broadcast, or polling-style wake-ups are optimization paths. Durable state remains in the backend; consumers must still lease/read from storage. |
| Retention and archive visibility | **Backend-dependent** | Maintenance, archive, and history retention behavior depends on backend support and configured operations. |

## Scheduling Semantics

Azums schedules by eligibility, not by real-time execution.

A job is eligible to be leased when all of these are true:

- `status = 'queued'`
- `queue` matches the worker queue
- `run_at <= now()` according to the backend
- policy gates such as in-flight limits and retry-rate limits allow leasing

Azums guarantees that workers do not intentionally lease future jobs before `run_at`. It does not guarantee millisecond-precise execution at `run_at`; actual execution depends on worker availability, database latency, queue depth, policy throttling, backend notification behavior, and handler runtime.

## DLQ Semantics

Azums moves jobs to the Dead-Letter Queue when retry cannot or should not continue.

Guaranteed:

- Retry exhaustion moves the job to `status = 'dlq'`.
- Permanent error classes move the job to `status = 'dlq'` immediately.
- Panics move the job to `status = 'dlq'` immediately with panic information where available.
- DLQ jobs carry a reason code such as `MAX_ATTEMPTS_EXCEEDED`, `PERMANENT_ERROR`, or `PANIC`.
- Attempt history is preserved until retention or maintenance removes it.
- DLQ jobs can be replayed as new queued jobs.

Not guaranteed:

- Azums does not guarantee that DLQ means no external side effect happened.
- Azums does not guarantee automatic human review, alerting, compensation, or refund behavior.
- Azums does not guarantee permanent retention unless the backend and maintenance policy are configured for it.

## Idempotency Semantics

Azums is an at-least-once system. Handlers and stream consumers must be idempotent if duplicate execution would be harmful.

Guaranteed:

- Each enqueued job receives a unique Azums job ID.
- Each stream event receives a monotonically increasing sequence number.
- Consumer-group acknowledgments move forward monotonically.

Not guaranteed:

- No automatic deduplication by payload or business key.
- No exactly-once external calls.
- No protection from a handler being invoked again after a crash, timeout, expired lease, retry, manual replay, or operator action.
- No guarantee that handler success is durable until Azums ACKs the attempt and job state transition in storage.
- No automatic idempotency across job replay; replay intentionally creates a new job.

Recommended application pattern:

```sql
INSERT INTO processed_operations (operation_key, completed_at)
VALUES ($1, now())
ON CONFLICT (operation_key) DO NOTHING;
```

Only perform the external side effect when the insert wins, or make the external side effect itself idempotent with a provider-supported idempotency key.

## Transactional Enqueue Semantics

Transactional enqueue means the application state change and the queue insert commit or roll back together.

Guaranteed:

- Azums enqueue operations are atomic within the selected backend operation.
- PostgreSQL and SQLite can provide database transaction semantics when the enqueue is performed as part of the same database transaction or equivalent backend-supported operation.

Backend-dependent:

- PostgreSQL provides the strongest relational transaction model.
- SQLite provides embedded transactional behavior with its single-writer constraints.
- Redis enqueue is atomic with respect to Redis commands used by the backend, but it is not an ACID transaction with a separate SQL application database.
- In-Memory enqueue is process-local and ephemeral.

Not guaranteed:

- Azums does not automatically coordinate a two-phase commit across Redis plus SQL, HTTP APIs, payment processors, or other external systems.
- A successful enqueue notification is not the durability guarantee; the durable backend write is.

## Stream Semantics

Streams are durable, append-only event logs exposed through `publish`, `read_events`, `ack`, and `consumer_group_info`.

Guaranteed:

- `publish` appends an event and returns its `sequence_no`.
- Sequence numbers are monotonically increasing within a stream.
- `read_events(after_seq, limit)` returns events with `sequence_no > after_seq` in ascending order.
- `ack(consumer_group, seq)` records progress for the group without moving the group backward.
- Unacknowledged events are replayable while retained by the backend.

Not guaranteed:

- No exactly-once consumer execution.
- No automatic per-event claim ownership or pending-entry timeout contract equivalent to every Redis Streams feature.
- No global ordering across different streams.
- No unlimited retention unless configured and supported by the backend.

## Consumer-Group Semantics

Consumer groups are offset trackers, not distributed locks.

Guaranteed:

- Each `(stream_name, consumer_group)` has a `last_acked_seq`.
- Acknowledgments are monotonic.
- Multiple consumers can share the same group name and coordinate by reading from the group's last acknowledged offset if the application follows that pattern.

Unspecified:

- Azums does not guarantee automatic work balancing between consumers in a group.
- Azums does not guarantee pending-entry ownership transfer.
- Azums does not guarantee exactly-once processing for a group.

## Replay Semantics

Replay creates more work; it does not mutate history into a different truth.

Job replay:

- Creates a new queued job.
- Copies the source job's type, payload, priority, and retry budget unless overrides are provided.
- Records `replay_of_job_id` so lineage is visible.
- Does not delete or reset the source job.
- Does not dedupe previous external side effects.

Stream replay:

- Reads historical events after a selected sequence offset.
- Does not mark them processed unless the consumer acknowledges progress.
- May cause repeated consumer work if the consumer replays events it has already processed.

## Cancellation Semantics

Azums distinguishes job cancellation from worker shutdown.

Guaranteed:

- `cancel_job(job_id, None)` cancels queued and scheduled jobs.
- `cancel_job(job_id, Some(worker_id))` cancels a running job only when `worker_id` owns the lease.
- Cancellation transitions the job to terminal `status = 'canceled'`.
- Terminal jobs reject cancellation.
- Backends with durable attempt records close the latest running attempt with `error_code = 'CANCELLED'`.
- Worker shutdown through a cancellation token stops the in-process worker loop from taking more work.

Not guaranteed:

- Azums does not forcibly interrupt arbitrary user handler code already executing on a runtime thread.
- A running handler must cooperate with application-level cancellation if immediate interruption is required.

## What Azums Does Not Guarantee

Azums does not guarantee:

- Exactly-once external side effects.
- Exactly-once handler execution.
- Global ordering across queues, streams, partitions, or workers.
- Completion order under parallel execution.
- Millisecond-precise scheduled execution.
- Automatic deduplication by payload, business key, idempotency key, or request ID.
- Atomic commits across Azums plus arbitrary external services.
- Permanent retention of jobs, attempts, DLQ entries, stream events, offsets, metrics, or archives.
- Automatic alerting, compensation, refunds, rollback, or human approval workflows.
- Automatic load balancing or pending-entry ownership semantics for stream consumer groups.
- Cross-process guarantees for the In-Memory backend.

The shortest correct summary is:

> Azums provides at-least-once execution. It does not guarantee exactly-once external side effects.
