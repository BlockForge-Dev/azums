# Feature Comparison Matrix

Comparing **PostgresFlow** against popular background job frameworks (**Celery**, **BullMQ**, **Sidekiq**, and **Factotum**):

| Feature / Aspect | PostgresFlow | Celery (Python) | BullMQ (Node/TS) | Sidekiq (Ruby) | Factotum |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Primary Broker** | **Postgres only** | RabbitMQ / Redis | Redis | Redis | Postgres |
| **Transactional Enqueue (ACID)** | ✅ **Native** (Same DB transaction) | ❌ Impossible (Dual Write) | ❌ Impossible (Dual Write) | ❌ Impossible (Dual Write) | ✅ Native |
| **Operational Infrastructure** | **Zero extra servers** | Requires RabbitMQ/Redis | Requires Redis instance/cluster | Requires Redis instance | Postgres |
| **Concurrency Control** | `FOR UPDATE SKIP LOCKED` | AMQP Ack / Visibility Timeout | Lua Scripts / Visibility | RPOPLPUSH / Lua | Advisory Locks / Polling |
| **Dead-Letter Queue (DLQ)** | ✅ Built-in | ⚠️ Configurable via DLX | ✅ Built-in | ⚠️ Retry queue / Dead queue | ⚠️ Basic |
| **Dataset Time Partitioning** | ✅ Automatic time partitions | ❌ None | ❌ None | ❌ None | ❌ None |
| **Language Support** | Rust (Library & CLI) | Python | Node.js / TypeScript | Ruby | Rust |
| **Admin Web Console** | ✅ Embedded Axum web UI | ⚠️ Flower (separate tool) | ⚠️ Bull-Board (separate pkg) | ✅ Built-in Web UI | ❌ CLI only |
| **Prometheus Metrics** | ✅ Native `/metrics/prom` | ⚠️ Exporter needed | ⚠️ Exporter needed | ⚠️ Plugin needed | ❌ None |
| **State Drift Risk** | ❌ **Zero** (Atomically committed) | ⚠️ High (Crash between DB & broker) | ⚠️ High (Crash between DB & broker) | ⚠️ High (Crash between DB & broker) | ❌ Zero |

## Key Advantages of PostgresFlow

1. **Zero Infrastructure Overhead**: If you already run PostgreSQL for your application data, you can deploy background processing without managing or monitoring Redis or RabbitMQ clusters.
2. **Atomic Transactional Enqueue**: Never lose a job or send duplicate jobs caused by network drops between your primary database and external queue brokers.
3. **Bounded Growth**: Automatic dataset partitioning and archiving prevents queue table bloat over time.
