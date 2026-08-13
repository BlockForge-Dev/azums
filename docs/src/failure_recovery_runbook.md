# Failure And Recovery Runbook

Use this runbook when production behavior is unclear, jobs are stuck, workers are failing, or DLQ is
growing.

## First Five Minutes

1. Identify affected backend, queue, and deployment version.
2. Check queue metrics: queue depth, retries, DLQ, execution latency, claim latency, worker count.
3. Pick one failing `job_id` and run `explain_job(job_id)`.
4. Check whether the job is `queued`, `running`, `succeeded`, `canceled`, or `dlq`.
5. Decide whether the incident is producer overload, worker failure, dependency failure, database
   failure, Redis failure, bad payload, or migration/configuration mismatch.

Implementation:

- [observability.rs](../../crates/azums-core/src/backend/observability.rs)
- [quickstart.rs](../../crates/azums/src/quickstart.rs)

Verification:

- [m17_observability.rs](../../crates/azums/tests/m17_observability.rs)

## Scenario: Queue Depth Growing

Symptoms:

- Producers enqueue faster than consumers complete jobs.
- `queue_depth` grows.
- Claim latency or execution latency may rise.

Actions:

1. Check worker count and whether workers are connected to the expected queue.
2. Check handler latency and downstream dependency health.
3. Increase workers only if the backend and downstream dependencies can absorb load.
4. Apply producer throttling or reject traffic if backlog would violate SLO.
5. For PostgreSQL admin/API enqueue, tighten `AZUMS_MAX_ENQUEUE_PER_MINUTE`.

Evidence:

- [concurrency_backpressure.md](concurrency_backpressure.md)
- [m8_concurrency_backpressure.rs](../../crates/azums/tests/m8_concurrency_backpressure.rs)
- [enqueue_guard.rs](../../crates/azums/src/jobs/enqueue_guard.rs)

## Scenario: Jobs Stuck Running

Symptoms:

- Jobs remain `running`.
- Worker process disappeared or stopped heartbeating.
- `LEASE_EXPIRED` appears after recovery.

Actions:

1. Check worker logs and process health.
2. Wait for `lock_expires_at`.
3. Trigger or wait for expired lease reaping.
4. Confirm the job returns to executable state or reaches terminal state after retry/DLQ.
5. Inspect handler idempotency before assuming replay or duplicate execution is safe.

Evidence:

- [leasing.md](leasing.md)
- [lease_recovery.rs](../../crates/azums/tests/lease_recovery.rs)
- [reliability_worker_crash.rs](../../crates/azums/tests/reliability_worker_crash.rs)

## Scenario: DLQ Spike

Symptoms:

- `jobs_dlq` increases.
- `explain_job` shows permanent failures, max attempts exceeded, timeouts, or panic.

Actions:

1. Group DLQ jobs by reason code and job type.
2. If reason is `BAD_PAYLOAD`, stop or fix producer.
3. If reason is `MAX_ATTEMPTS_EXCEEDED`, inspect downstream dependency health and retry policy.
4. If reason is `PANIC`, roll back the handler or deploy a hotfix.
5. Replay only after the underlying cause is fixed and side effects are safe to repeat.

Evidence:

- [failure_handling.md](failure_handling.md)
- [dlq.rs](../../crates/azums/tests/dlq.rs)
- [failure_semantics.rs](../../crates/azums/tests/failure_semantics.rs)
- [replay_semantics.md](replay_semantics.md)

## Scenario: Database Failure

Symptoms:

- Workers cannot lease, ACK, or enqueue.
- Database health checks fail.
- Connection acquisition times out.

Actions:

1. Treat the backend as source of truth. Do not infer job loss from worker errors.
2. Restore database service.
3. Run `health_check`.
4. Run migrations only if schema state requires it.
5. Restart a single worker canary.
6. Reap expired leases.
7. Watch retries and DLQ for jobs that may have executed side effects before ACK.

Evidence:

- [transactional_integrity.md](transactional_integrity.md)
- [transactional_enqueue.rs](../../crates/azums/tests/transactional_enqueue.rs)
- [chaos_engineering.md](chaos_engineering.md)

## Scenario: Redis Disconnect Or Restart

Symptoms:

- Redis backend commands fail or time out.
- Workers stop leasing or stream consumers stop reading.

Actions:

1. Verify Redis persistence and failover state.
2. Restore Redis connectivity.
3. Restart workers after Redis is healthy.
4. Inspect queue and stream state from Redis.
5. Replay or re-enqueue only after confirming whether the job/event exists.

Evidence:

- [redis_backend.md](redis_backend.md)
- [backend_equivalence.md](backend_equivalence.md)
- [redis_backend.rs](../../crates/azums/tests/redis_backend.rs)

## Scenario: Bad Migration Or Incompatible Upgrade

Symptoms:

- New workers fail at startup.
- Queries fail due to missing or incompatible columns/indexes.
- Old and new workers disagree about schema.

Actions:

1. Stop rollout.
2. Keep existing healthy workers running only if they are compatible with current schema.
3. Roll back application image.
4. Apply explicit migration repair if needed.
5. Run smoke tests before resuming traffic.
6. Add a regression test for the failed upgrade path.

Evidence:

- [production_deployment.md](production_deployment.md)
- [RELEASE.md](../RELEASE.md)

## Scenario: Suspected Duplicate Side Effect

Symptoms:

- External system shows duplicate email, payment, webhook, AI task, or write.
- Azums shows retry, lease expiry, worker crash, or replay.

Actions:

1. Confirm Azums delivery history with `explain_job`.
2. Check idempotency key and handler-side deduplication.
3. Check whether the worker crashed after side effect but before ACK.
4. Repair the external system according to application policy.
5. Add or tighten handler idempotency before replaying.

Evidence:

- [idempotency.md](idempotency.md)
- [semantics.md](semantics.md)
- [idempotency.rs](../../crates/azums/tests/idempotency.rs)

## Recovery Rule

Never delete jobs, attempts, DLQ rows, stream events, or consumer offsets during an incident unless
you have:

- backed up the backend
- identified the affected queue/stream range
- documented the expected state after repair
- run the repair in staging or on a copied dataset
- assigned an owner for post-incident verification
