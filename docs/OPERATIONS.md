# Operations Runbook

This root-level runbook is retained as a short pointer. The production handoff now lives in the
mdBook:

- [Production Readiness](src/production_readiness.md)
- [Production Deployment Guide](src/production_deployment.md)
- [Failure And Recovery Runbook](src/failure_recovery_runbook.md)

## Runtime Topology
- `db`: Postgres / Redis / SQLite
- `azums`: background job processing worker node
- `azums-dashboard`: optional web dashboard console

## Required Environment
- `DATABASE_URL` required at runtime
- `AZUMS_WORKER_ID` optional (defaults from hostname/fallback)
- `AZUMS_QUEUE` optional (default `default`)
- `AZUMS_LEASE_SECONDS` optional (default `10`)
- `AZUMS_DEQUEUE_BATCH_SIZE` optional (default `256`)
- `AZUMS_MIGRATE_ON_STARTUP` optional
- `AZUMS_MAX_PAYLOAD_BYTES` optional
- `AZUMS_MAX_ENQUEUE_PER_MINUTE` optional

Maintenance envs:
- `ARCHIVE_SUCCEEDED_AFTER_DAYS` default `7`
- `PRUNE_HISTORY_AFTER_DAYS` default `7`
- `MAINTENANCE_INTERVAL_SECS` default `60`

## Start and Stop

Start:

```powershell
docker compose up --build -d
```

Scale workers:

```powershell
docker compose --profile worker up -d --scale worker=4
```

Stop:

```powershell
docker compose down
```

## Capacity and Tuning Notes
- tune DB connection pool and worker count together
- monitor WAL growth and autovacuum
- ensure indexes used by runnable scans and attempt lookups remain healthy
- benchmark changes before production rollout

