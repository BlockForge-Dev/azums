# Feature Comparison Matrix

Comparing **azums** against popular background job frameworks (**Celery**, **BullMQ**, **Sidekiq**, and **Factotum**):

| Feature / Aspect | azums | Celery (Python) | BullMQ (Node/TS) | Sidekiq (Ruby) | Factotum |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Primary Broker / Storage** | **Postgres, SQLite, Redis, In-Memory** | RabbitMQ / Redis | Redis | Redis | Postgres |
| **Strict FIFO Ordering** | ✅ **Per-queue configurable (default FIFO)** | ❌ Best-effort | ⚠️ FIFO per queue | ⚠️ FIFO per queue | ❌ Best-effort |
| **Transactional Enqueue (ACID)** | ✅ **Native** (Same DB transaction) | ❌ Impossible (Dual Write) | ❌ Impossible (Dual Write) | ❌ Impossible (Dual Write) | ✅ Native |
| **Operational Infrastructure** | **Zero extra servers** (embed SQLite or use Postgres/Redis) | Requires RabbitMQ/Redis | Requires Redis instance/cluster | Requires Redis instance | Postgres |
| **Concurrency Control** | `FOR UPDATE SKIP LOCKED` / `LMOVE` | AMQP Ack / Visibility Timeout | Lua Scripts / Visibility | RPOPLPUSH / Lua | Advisory Locks / Polling |
| **Dead-Letter Queue (DLQ)** | ✅ Built-in | ⚠️ Configurable via DLX | ✅ Built-in | ⚠️ Retry queue / Dead queue | ⚠️ Basic |
| **Dataset Time Partitioning** | ✅ Automatic time partitions | ❌ None | ❌ None | ❌ None | ❌ None |
| **Language Support** | Rust (Library & CLI) | Python | Node.js / TypeScript | Ruby | Rust |
| **Admin Web Console** | ✅ Embedded Axum web UI | ⚠️ Flower (separate tool) | ⚠️ Bull-Board (separate pkg) | ✅ Built-in Web UI | ❌ CLI only |
| **Prometheus Metrics** | ✅ Native `/metrics/prom` | ⚠️ Exporter needed | ⚠️ Exporter needed | ⚠️ Plugin needed | ❌ None |
| **State Drift Risk** | ❌ **Zero** (Atomically committed) | ⚠️ High (Crash between DB & broker) | ⚠️ High (Crash between DB & broker) | ⚠️ High (Crash between DB & broker) | ❌ Zero |

## Key Advantages of azums

1. **Zero Infrastructure Overhead**: Deploy background processing using your existing database (Postgres, SQLite, or Redis) without external broker dependencies.
2. **Atomic Transactional Enqueue**: Never lose a job or send duplicate jobs caused by network drops between your primary database and external queue brokers.
3. **Bounded Growth**: Automatic dataset partitioning and archiving prevents queue table bloat over time.

