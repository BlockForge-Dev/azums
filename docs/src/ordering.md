# Ordering Guarantees & Queue Policies

`azums` supports per-queue ordering policies to control the precise sequence in which background jobs are leased and executed by workers.

## Queue Ordering Policies (`QueueOrdering`)

When creating or configuring a queue, you can specify one of two ordering modes via `QueueConfig`:

```rust
use azums::{quickstart, QueueConfig, QueueOrdering};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let flow = quickstart("memory").await?;

    // Configure "order_processing" queue for strict FIFO execution (default)
    flow.configure_queue("order_processing", QueueConfig::new(QueueOrdering::Fifo)).await;

    // Configure "bulk_notifications" queue for maximum throughput
    flow.configure_queue("bulk_notifications", QueueConfig::new(QueueOrdering::Fastest)).await;

    Ok(())
}
```

### 1. `QueueOrdering::Fifo` (Default)
- **Behavior**: Jobs are leased strictly in First-In, First-Out order based on priority (`priority DESC`), scheduled run time (`run_at ASC`), enqueued timestamp (`created_at ASC`), and unique identifier (`id ASC` / `rowid ASC`).
- **Use Case**: Financial transactions, payment processing, stateful workflows, audit logging, sequential data processing.
- **Guarantee**: With a single worker process (or single-partition worker), jobs are guaranteed to execute in exact enqueued sequence.

### 2. `QueueOrdering::Fastest`
- **Behavior**: Jobs are leased as quickly as possible based on priority (`priority DESC`) and scheduled run time (`run_at ASC`) without enforcing strict creation timestamp sorting.
- **Use Case**: High-volume telemetry, email notifications, cache warming, webhooks.

---

## Storage Backend Guarantees

| Storage Backend | FIFO Mechanism | Details |
|---|---|---|
| **PostgreSQL** | `ORDER BY priority DESC, run_at ASC, created_at ASC, id ASC FOR UPDATE SKIP LOCKED` | Uses composite index `jobs_fifo_queue_created_idx` to scan runnable jobs in exact enqueued order without locking un-leased rows. |
| **SQLite** | `ORDER BY priority DESC, run_at ASC, created_at ASC, rowid ASC` | Uses monotonically increasing `rowid` to break sub-millisecond ties. Single-writer WAL mode guarantees write serialization order. |
| **Redis** | `RPUSH` + `LMOVE LEFT RIGHT` | Enqueues jobs to the right (tail) of the list and atomically moves jobs from the left (head) into worker processing queues, preserving FIFO insertion order. |
| **In-Memory** | `created_at ASC` + `id ASC` | Deterministic sorting on in-memory job state structures. |

---

## Multi-Worker Concurrent Leasing Nuances

When running multiple concurrent workers on the same `QueueOrdering::Fifo` queue:

- **Batch Leasing & Lock Contention**: PostgreSQL uses `FOR UPDATE SKIP LOCKED` and Redis uses `LMOVE` to prevent worker lock contention.
- **Best-Effort FIFO Across Workers**: Each worker receives an atomically leased batch ordered by `created_at ASC`. However, because parallel workers process leased batches concurrently, network jitter or varying handler execution times may cause small overlaps in completion order across workers.
- **Strict Single-Key FIFO**: If absolute strict linear ordering is required across multiple nodes, use dataset partitioning or assign a single dedicated worker to that queue partition.

