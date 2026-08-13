# Production Deployment Guide

This guide is the handoff document for an engineering team deploying Azums as infrastructure.

## 1. Choose The Backend

| Need | Recommended backend |
|---|---|
| Unit tests, examples, local ephemeral jobs | Memory |
| Embedded single-binary service or local durable queue | SQLite |
| Distributed workers, SQL transactions with app data, strongest durability | PostgreSQL |
| Redis-native low-latency deployment with Redis persistence configured | Redis |

Check capabilities at startup:

```rust,no_run
let queue = azums::quickstart(std::env::var("DATABASE_URL")?).await?;
let caps = queue.capabilities();
assert!(caps.durable_jobs, "production requires durable jobs");
```

Do not choose Memory for production durability. Do not choose SQLite for multi-host worker
coordination. Do not choose Redis when enqueue must commit atomically with application SQL data.

## 2. Configure Runtime

Required:

- `DATABASE_URL` or explicit URL passed to `quickstart`.
- Unique `AZUMS_WORKER_ID` per worker process.
- `AZUMS_QUEUE` for the queue this worker polls.

Recommended:

- `AZUMS_LEASE_SECONDS`: longer than normal handler latency, shorter than acceptable recovery time.
- `AZUMS_DEQUEUE_BATCH_SIZE`: start conservative, then tune with metrics.
- `AZUMS_REAP_INTERVAL_MS`: keep near default unless recovery or database load requires tuning.
- `AZUMS_MAX_PAYLOAD_BYTES`: set to your operational payload limit.
- `AZUMS_MAX_ENQUEUE_PER_MINUTE`: set per queue for producer storm protection.
- `AZUMS_ADMIN_ADDR=off` unless the admin service is intentionally exposed.
- `AZUMS_API_TOKEN`: required when admin endpoints are reachable beyond localhost.
- `AZUMS_MIGRATE_ON_STARTUP=0` for controlled production releases unless your team explicitly uses
  startup migrations.

Implementation:

- [config.rs](../../crates/azums/src/config.rs)
- [quickstart.rs](../../crates/azums/src/quickstart.rs)

## 3. Run Migrations

Preferred production rollout:

1. Back up the database.
2. Run migrations as a separate deploy step.
3. Verify schema readiness.
4. Start a small worker canary.
5. Increase worker count after metrics are stable.

Startup migrations are useful for development and small deployments, but production teams should
review every migration before rollout.

Implementation:

- [migrations](../../crates/azums/migrations)
- [db.rs](../../crates/azums/src/db.rs)

Verification:

- [transactional_enqueue.rs](../../crates/azums/tests/transactional_enqueue.rs)
- [sqlite.rs](../../crates/azums/tests/sqlite.rs)

## 4. Deploy Workers

Start with one worker per queue. Scale horizontally only after observing:

- queue depth
- claim latency
- execution latency
- retries
- DLQ rate
- database CPU and lock contention
- Redis memory and persistence health, if using Redis

Use graceful shutdown on deploy:

1. Stop accepting new web/API traffic if the worker is colocated with producers.
2. Signal workers with normal process termination.
3. Let in-flight handlers complete within the orchestrator grace period.
4. After shutdown, verify queue depth and running jobs.
5. Reap expired leases if a worker was killed before ACK.

Implementation:

- [quickstart.rs](../../crates/azums/src/quickstart.rs)
- [leasing.md](leasing.md)

Verification:

- [phantom_recovery.rs](../../crates/azums/tests/phantom_recovery.rs)
- [lease_recovery.rs](../../crates/azums/tests/lease_recovery.rs)

## 5. Protect Admin Access

The admin API can inspect jobs, payloads, errors, DLQ records, metrics, and replay jobs. Treat it as
privileged production access.

Required when exposed:

- Set `AZUMS_API_TOKEN`.
- Put the service behind TLS.
- Restrict network access to operators or internal systems.
- Do not expose job payloads to users who cannot see the underlying application data.

Implementation:

- [api/mod.rs](../../crates/azums-dashboard/src/api/mod.rs)
- [admin_api.md](admin_api.md)

## 6. Observe The System

At minimum, alert on:

- `queue_depth` above expected backlog
- `jobs_failed` and `jobs_retried` spikes
- `jobs_dlq > 0` for critical queues
- `execution_latency_ms_avg` outside SLO
- repeated `LEASE_EXPIRED`
- database disconnects, deadlocks, or Redis disconnects

Use:

- [observability.md](observability.md)
- [m17_observability.rs](../../crates/azums/tests/m17_observability.rs)

## 7. Release And Roll Back

For every release:

1. Run CI and production readiness checks.
2. Review migrations for backward compatibility.
3. Deploy migrations first when possible.
4. Deploy one worker canary.
5. Watch retry, DLQ, queue depth, and latency.
6. Complete rollout.

Rollback:

1. Stop rollout.
2. Roll back application image/version.
3. If a migration changed schema incompatibly, apply the migration-specific repair plan.
4. Keep workers down until schema/application compatibility is restored.
5. Record incident notes and add a regression test.

Reference:

- [RELEASE.md](../RELEASE.md)
- [production_readiness.md](production_readiness.md)
