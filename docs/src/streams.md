# Redis-Style Event Streams & Durable Pub/Sub

`azums` unifies background job queues and durable event streaming into a single, high-performance API. Modeled after Redis Streams, `azums` supports append-only, replayable event logs with consumer group sequence tracking and real-time notifications across PostgreSQL, Redis, SQLite, and In-Memory backends.

---

## 1. Stream Operations & Usage

```rust,no_run
use azums::{quickstart, NewEvent};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let orders_stream = client.stream("orders");

    // 1. Publish Event
    let seq = orders_stream.publish("order_created", json!({
        "order_id": "ord_1001",
        "amount": 250.00,
        "customer_id": "cust_42"
    })).await?;
    println!("Appended event at sequence_no: {seq}");

    // 2. Read Events (from sequence offset 0)
    let events = orders_stream.read_events(0, 100).await?;
    for event in &events {
        println!("Event #{}: {} => {:?}", event.sequence_no, event.event_type, event.payload_json);
    }

    // 3. Acknowledge Consumer Group Offset
    orders_stream.ack("analytics_group", seq).await?;

    // 4. Fetch Consumer Group Status
    let group_info = orders_stream.consumer_group_info().await?;
    for cg in &group_info {
        println!("Group: {}, Last Acked Seq: {}", cg.consumer_group, cg.last_acked_seq);
    }

    Ok(())
}
```

---

## 2. Delivery Guarantees & Replay Semantics

### At-Least-Once Delivery
Every stream entry receives a monotonically increasing 1-based sequence number (`sequence_no`). Consumer group sequence offsets (`last_acked_seq`) track processed entries. If a consumer process terminates unexpectedly, unacknowledged events remain fully replayable from sequence offset `sequence_no > last_acked_seq`.

### Sequence Offset Replay
Because stream logs are durable append-only structures, consumers can inspect or replay historical entries at any time without removing data from the log:

```rust,no_run
// Replay 100 historical events starting immediately after sequence 5000
let historical_events = stream.read_events(5000, 100).await?;
```

### Idempotency & External Side Effects
Azums provides at-least-once stream replay. It does not guarantee exactly-once external side effects. If duplicate processing would be harmful, pair stream replay with application-level deduplication using `sequence_no`:

```sql
INSERT INTO processed_stream_events (stream_name, consumer_group, sequence_no)
VALUES ('orders', 'billing_service', $1)
ON CONFLICT (stream_name, consumer_group, sequence_no) DO NOTHING;
```
