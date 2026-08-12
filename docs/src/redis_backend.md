# Redis Storage & Streaming Backend

The `azums-redis` crate delivers a production-grade Redis 5.0+ storage and event streaming backend driver built on `redis::aio::ConnectionManager`.

---

## 1. Zero-Config Connection URLs

Switching your entire application from PostgreSQL, SQLite, or In-Memory to Redis requires changing **only the connection string**:

```rust,no_run
// Redis standalone
let client = azums::quickstart("redis://127.0.0.1:6379").await?;

// Redis TLS / Sentinel / Authenticated
let client = azums::quickstart("rediss://:secretpass@redis.prod.internal:6379/0").await?;
```

---

## 2. Redis Key Schema & Data Layout

| Purpose | Redis Data Structure | Key Naming Pattern | Description |
|:---|:---|:---|:---|
| **Job Payloads** | Hash (`HSET` / `HGET`) | `azums:jobs` | Hash mapping `job_id` string to serialized JSON `Job`. |
| **Queue List** | List (`RPUSH` / `RPOPLPUSH`) | `azums:queue:<queue>` | List holding queued `job_id` strings. |
| **Worker Processing** | List (`RPOPLPUSH` / `LREM`) | `azums:processing:<queue>:<worker_id>` | List holding leased job IDs actively processed by worker. |
| **Instant PubSub** | Channel (`PUBLISH` / `SUBSCRIBE`) | `azums:notify:<queue>` | Real-time wake-up notifications for idle workers. |
| **Stream Events** | List & Hash | `azums:stream_events:<stream>` | Append-only event log holding JSON stream events. |
| **Stream Offsets** | Hash (`HSET` / `HGETALL`) | `azums:stream_offsets:<stream>` | Hash mapping `consumer_group` to `last_acked_seq`. |

---

## 3. High-Throughput Batch Leasing

`RedisBackend` leverages `RPOPLPUSH` (or `LMOVE` in Redis 6.2+) to atomically claim jobs from `azums:queue:<queue>` to `azums:processing:<queue>:<worker_id>`. If a worker host crashes mid-execution, the processing list keeps the leased job visible for later recovery.

Expired locks are automatically reclaimed back to the queue via `backend.reap_expired_locks()`.
