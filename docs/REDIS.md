# Azums Redis Storage & Streaming Backend

`azums` natively supports Redis (5.0+) as a high-throughput background job processing engine and durable event streaming store.

---

## 1. Quickstart

To use Redis, pass a valid Redis connection URL (e.g., `redis://127.0.0.1:6379` or `rediss://...`) to `azums::quickstart()`.

```rust,no_run
use azums::{quickstart, Job};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connect to Redis
    let client = quickstart("redis://127.0.0.1:6379").await?;

    // 1. Enqueue job
    let job_id = client.enqueue(Job::new("send_email", json!({"to": "user@example.com"}))).await?;
    println!("Enqueued job: {job_id}");

    // 2. Stream event
    let stream = client.stream("orders");
    let seq = stream.publish("order_created", json!({"order_id": 42})).await?;
    println!("Published stream event: {seq}");

    Ok(())
}
```

---

## 2. Key Architecture & Data Structures

| Operation | Redis Data Structure | Key Schema |
|:---|:---|:---|
| **Job Details** | Hash (`HSET` / `HGET`) | `azums:jobs` -> `<job_id>: <json>` |
| **Job Queue** | List (`RPUSH` / `RPOPLPUSH`) | `azums:queue:<queue_name>` |
| **Worker Processing** | List (`RPOPLPUSH` / `LREM`) | `azums:processing:<queue_name>:<worker_id>` |
| **Instant Notifications** | PubSub (`PUBLISH` / `SUBSCRIBE`) | `azums:notify:<queue_name>` |
| **Event Stream Logs** | List & Hash | `azums:stream_events:<stream_name>` |
| **Stream Consumer Offsets** | Hash (`HSET` / `HGETALL`) | `azums:stream_offsets:<stream_name>` |

---

## 3. Switching Backends (PostgreSQL ↔ Redis ↔ SQLite ↔ Memory)

Because `azums` exposes a backend-agnostic public API (`azums::quickstart`), switching storage engines requires changing **only the connection string**:

```rust,no_run
// PostgreSQL (ACID & relational DB compliance)
let client = azums::quickstart("postgres://user:pass@localhost:5432/db").await?;

// Redis (Ultra-low latency & zero relational overhead)
let client = azums::quickstart("redis://127.0.0.1:6379").await?;

// Embedded SQLite (Single binary / local dev)
let client = azums::quickstart("sqlite://app.db").await?;

// Ephemeral In-Memory (Unit testing)
let client = azums::quickstart("memory").await?;
```
