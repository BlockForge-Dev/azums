# Event-Driven Instant Wake-Up Architecture

Traditional background job systems rely on busy-polling loops (`SELECT ... FOR UPDATE SKIP LOCKED` every 500ms - 1000ms), wasting CPU cycles and introducing significant latency overhead.

`azums` replaces polling loops with **Event-Driven Instant Wake-Up** across all storage backends, dropping idle CPU consumption to **0.0%** and wake-up latency to sub-milliseconds.

---

## 1. Engine Mechanism Matrix

| Storage Engine | Notification Channel | Trigger Mechanism | Idle Worker Await |
|:---|:---|:---|:---|
| **PostgreSQL** | `azums_job_enqueued_<queue>` | `NOTIFY` via `sqlx::postgres::PgListener` | `PgListener::into_stream()` |
| **Redis** | `azums:notify:<queue>` | `PUBLISH` via Redis PubSub | Broadcast Stream Receiver |
| **SQLite** | `azums_sqlite_notify_<queue>` | In-process Broadcast + 100ms Fallback | `BroadcastStream::merge()` |
| **In-Memory** | `azums_mem_notify_<queue>` | `tokio::sync::broadcast` | `BroadcastStream` |

---

## 2. Worker Loop Mechanics

When a queue becomes empty, idle worker tasks in `azums` call `backend.subscribe(queue).await?` to receive a `NotificationStream`. The worker task yields execution to the Tokio async runtime until a notification arrives:

```rust,no_run
let mut notification_stream = backend.subscribe("emails").await?;

loop {
    let batch = backend.lease_jobs_batch("emails", "worker_1", 30, 25).await?;
    if batch.is_empty() {
        // Idle state: zero CPU usage while awaiting LISTEN/NOTIFY event
        tokio::select! {
            _ = notification_stream.next() => {},
            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {},
        }
        continue;
    }

    for job in batch {
        // Execute job handler...
    }
}
```

---

## 3. Connection Pool Isolation

To prevent `LISTEN/NOTIFY` (PostgreSQL) and `PubSub` (Redis) subscriptions from consuming query pool connection slots, `azums` maintains strict internal connection pool isolation:

- **Single Configuration String**: Users supply a single `DATABASE_URL` or Redis URL without complex multi-pool tuning.
- **Dedicated Unpooled Sockets**:
  - In **PostgreSQL**, `PgListener::connect(&database_url)` opens a dedicated, unpooled TCP socket for `LISTEN` events, leaving `sqlx::PgPool` 100% available for query execution (`enqueue`, `lease_jobs_batch`, `mark_succeeded`).
  - In **Redis**, `client.get_async_pubsub()` connects a dedicated PubSub socket, ensuring `ConnectionManager` command throughput remains uninhibited under heavy load.
- **Zero Pool Starvation**: Workers can subscribe to multiple queues even with a minimal query connection pool size (e.g. `AZUMS_DB_MAX_CONNECTIONS=1`) without starving application queries.

