# Storage Abstraction & Backend Equivalence

Azums exposes one application-facing job API across multiple storage environments. Backend equivalence means the same business job code can enqueue, lease, run, retry, cancel, DLQ, replay, and stream events through `StorageBackend` without rewriting handler logic.

Equivalence does not mean every storage engine has the same operational guarantees. Applications can inspect `BackendCapabilities` at runtime:

```rust,no_run
let client = azums::quickstart("memory").await?;
let caps = client.capabilities();

if caps.distributed_workers {
    println!("safe for multi-process workers");
}
```

## Capability Model

```rust,ignore
pub struct BackendCapabilities {
    pub transactional_enqueue: bool,
    pub durable_jobs: bool,
    pub notifications: bool,
    pub streams: bool,
    pub consumer_groups: bool,
    pub distributed_workers: bool,
    pub ordering: OrderingCapability,
}
```

| Field | Meaning |
|---|---|
| `transactional_enqueue` | Enqueue can participate in the backend's transaction model with application data stored in that same backend. |
| `durable_jobs` | Jobs survive process restart when the backend itself is durable and configured persistently. |
| `notifications` | Workers can receive wake-up hints instead of relying only on polling. Notifications are not the durability contract. |
| `streams` | Backend supports append/read stream operations through `StreamBackend`. |
| `consumer_groups` | Backend stores monotonic consumer-group offsets. |
| `distributed_workers` | Multiple processes or hosts can coordinate worker leases through the backend. |
| `ordering` | Strength of lease ordering support: none, FIFO leasing, or FIFO plus fastest leasing modes. |

## Compatibility Matrix

| Capability | Memory | SQLite | PostgreSQL | Redis |
|---|---|---|---|---|
| One job API | Yes | Yes | Yes | Yes |
| Enqueue idempotency key | Yes, process-local | Yes, SQLite unique index | Yes, SQL unique index | Yes, Redis idempotency hash |
| Transactional enqueue | No | Yes, embedded SQL transaction via `enqueue_in_tx` | Yes, SQL transaction via `enqueue_in_tx` | No, Redis atomic operation only |
| Durable jobs | No, process-local | Yes, with file-backed DB | Yes | Yes, with Redis persistence configured |
| Notifications | Yes, in-process broadcast | Yes, in-process broadcast plus interval fallback | Yes, LISTEN/NOTIFY | Yes, Pub/Sub plus interval fallback |
| Streams | Yes | Yes | Yes | Yes |
| Consumer groups | Yes, process-local offsets | Yes, SQL offsets | Yes, SQL offsets | Yes, Redis hash offsets |
| Distributed workers | No | No, single-process embedded target | Yes | Yes |
| Ordering | FIFO and fastest leasing | FIFO and fastest leasing | FIFO and fastest leasing | FIFO leasing |
| Transactions with app DB | No | Yes, if app data uses same SQLite DB | Yes, if app data uses same Postgres DB | No |
| Best fit | Unit tests and local ephemeral runs | Embedded apps and single-binary services | Distributed production workers | Distributed low-latency Redis environments |

## Equivalent Application Code

Business handlers do not change by backend:

```rust,no_run
use azums::{quickstart, Job};
use serde_json::json;

async fn run(url: &str) -> anyhow::Result<()> {
    let client = quickstart(url).await?;

    client.register_handler("send_email", |job| async move {
        let to = job.payload["to"].as_str().unwrap_or_default();
        println!("send email to {to}");
        Ok(())
    }).await;

    client.enqueue(Job::new("send_email", json!({"to": "user@example.com"}))).await?;
    client.run_until_empty().await?;
    Ok(())
}
```

The same function can run with:

- `memory`
- `sqlite://jobs.db?mode=rwc`
- `postgres://user:pass@host/db`
- `redis://127.0.0.1:6379`

## Where Equivalence Holds

These semantics are portable across all backends:

- Job identity, idempotency key, type, payload, queue, priority, schedule, status, timestamps, retry budget, and replay lineage.
- At-least-once execution.
- Worker lease ownership before attempts and terminal mutations.
- Retry, DLQ, cancellation, and replay APIs.
- Stream append, read, ACK, consumer-group offset, and replay APIs.
- Handler code written against `Job`, `StorageBackend`, `QuickstartFlow`, and `StreamHandle`.

## Where Equivalence Stops

Azums does not fake these differences:

- Memory is not durable and does not coordinate across processes.
- SQLite is durable but targeted at embedded/single-process deployments; it does not provide multi-host distributed worker coordination.
- Redis operations are atomic inside Redis, but they are not a SQL transaction with your application database.
- Redis FIFO support follows list order; it does not expose the same SQL query-level ordering flexibility as PostgreSQL or SQLite.
- Notifications are wake-up hints; every backend still relies on storage state as the source of truth.
- Exactly-once external side effects are not guaranteed by any backend.

## Backend Selection Rule

Use the lowest operational backend that satisfies the required capabilities:

- Need fast tests: Memory.
- Need embedded durable jobs: SQLite.
- Need SQL transactions with app data and distributed workers: PostgreSQL.
- Need distributed Redis-native job/stream storage and accept Redis atomicity instead of SQL transactions: Redis.

For commit/rollback failure coverage, see [Transactional Integrity](transactional_integrity.md).
