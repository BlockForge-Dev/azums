# Azums Redis-Style Event Streams & Durable Pub/Sub

`azums` includes a high-performance, database-backed event streaming engine modeled after Redis Streams. Event logs are append-only, replayable, and partitioned per stream log with consumer group sequence tracking.

---

## 1. Quickstart

```rust,no_run
use azums::{quickstart, NewEvent};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let orders_stream = client.stream("orders");

    // 1. Publish event to stream
    let seq = orders_stream.publish("order_created", json!({"order_id": 1001})).await?;
    println!("Published order_created at sequence_no: {seq}");

    // 2. Read unread events
    let events = orders_stream.read_events(0, 50).await?;
    for event in &events {
        println!("Event #{}: {} => {:?}", event.sequence_no, event.event_type, event.payload_json);
    }

    // 3. Acknowledge sequence number for consumer group
    orders_stream.ack("analytics_group", seq).await?;

    Ok(())
}
```

---

## 2. Stream Guarantees & Semantics

### At-Least-Once Delivery
- Events appended to stream logs are guaranteed to persist with monotonically increasing 1-based sequence numbers (`sequence_no`).
- Consumers maintain their acknowledged sequence offset per consumer group (`last_acked_seq`). If a consumer process crashes during processing, the unacknowledged events remain replayable from offset `sequence_no > last_acked_seq`.

### Replayability & Arbitrary Offset Reads
- Unlike traditional queues where dequeued items are removed, stream logs are durable append-only logs.
- Any consumer can replay historical events from sequence 0 (`read_events(stream, 0, limit)`) or resume from any specific sequence offset (`read_events(stream, last_seq, limit)`).

### Idempotency Patterns & External Side Effects
- Azums provides **At-Least-Once delivery**. It does not guarantee exactly-once external side effects.
- When processing stream events, consumers should perform state updates atomically inside database transactions using `event.sequence_no` or unique payload keys as deduplication guards.

```sql
INSERT INTO processed_events (stream_name, consumer_group, sequence_no)
VALUES ('orders', 'billing', 42)
ON CONFLICT DO NOTHING;
```

---

## 3. Storage Backend Drivers

| Backend | Sequence Generation | Notification Mechanism | Consumer Offset Storage |
|:---|:---|:---|:---|
| **PostgreSQL** | `BIGSERIAL PRIMARY KEY` | `NOTIFY azums_stream_<stream>` | `stream_offsets` (`GREATEST` updates) |
| **SQLite** | `INTEGER PRIMARY KEY AUTOINCREMENT` | In-process Broadcast + Interval Fallback | `stream_offsets` (`MAX` updates) |
| **In-Memory** | Monotonic Vector index | `tokio::sync::broadcast` | Thread-safe `HashMap` |
