# Azums Performance Tuning & Optimization Guide

`azums` is designed from the ground up for single-digit millisecond latency and maximum job throughput across PostgreSQL, Redis, SQLite, and In-Memory storage backends.

This guide details best practices for tuning `azums` in high-scale production environments.

---

## 1. PostgreSQL Optimization

### Database Connection Pool (`max_connections`)
- **Worker Concurrency**: Set `max_connections` on `PgPool` equal to `2 * worker_threads + 5`.
- **Statement Timeout**: Keep transaction hold time short by configuring `max_lifetime(Duration::from_secs(1800))` and `idle_timeout(Duration::from_secs(600))`.

```rust,no_run
use azums::db::make_pool;

let pool = make_pool("postgres://user:pass@host/db").await?;
```

### PostgreSQL `LISTEN / NOTIFY`
- Ensure PostgreSQL configuration allows `max_locks_per_transaction = 128` (default: 64) for large numbers of active queues.
- Channels follow `azums_job_enqueued_<queue>` and `azums_stream_<stream>` naming. Workers drop idle CPU usage to `0.0%` by awaiting notifications instead of polling.

---

## 2. Redis Optimization (`azums-redis`)

### Connection Manager & Pipelining
- `RedisBackend` uses `redis::aio::ConnectionManager` for auto-reconnecting multiplexed connections.
- Ensure your Redis server configuration enables:
  ```ini
  maxmemory-policy volatile-lru
  save "" # Disable synchronous disk saves if purely ephemeral queue
  ```

---

## 3. SQLite Optimization (`azums-sqlite`)

### WAL Mode & Synchronous PRAGMAs
- `azums` automatically configures SQLite with:
  ```sql
  PRAGMA journal_mode = WAL;
  PRAGMA synchronous = NORMAL;
  PRAGMA busy_timeout = 5000;
  ```
- **Shared Memory Cache**: Use `sqlite://file:app.db?mode=memory&cache=shared` for multi-threaded in-memory SQLite queues.

---

## 4. Batch Leasing & Worker Tuning

### Batch Size Tuning
- When consuming high-volume queues, set `batch_size` between `25` and `100` in `lease_jobs_batch()`.
- Batch leasing claims up to $N$ jobs in a single database roundtrip, boosting throughput by up to 10x over single-job leasing.

```rust,no_run
// Process 50 jobs per batch
let batch = backend.lease_jobs_batch("orders", "worker_1", 30, 50).await?;
```

---

## 5. Summary Benchmark Checklist

- [x] Enable PostgreSQL `LISTEN/NOTIFY` (or Redis PubSub) to eliminate polling delay.
- [x] Use `lease_jobs_batch(..., 50)` for bulk job processing.
- [x] Enable SQLite `WAL` mode for embedded apps.
- [x] Maintain separate read/write pools for high-scale admin dashboard inspection.
