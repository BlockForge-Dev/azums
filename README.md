# Azums

[![Crates.io](https://img.shields.io/crates/v/azums.svg)](https://crates.io/crates/azums)
[![Docs.rs](https://docs.rs/azums/badge.svg)](https://docs.rs/azums)
[![CI Status](https://github.com/BlockForge-Dev/azums/actions/workflows/ci.yml/badge.svg)](https://github.com/BlockForge-Dev/azums/actions/workflows/ci.yml)
[![Live Benchmarks](https://img.shields.io/badge/benchmarks-live-blue)](https://blockforge-dev.github.io/azums/)
[![Execution Semantics](https://img.shields.io/badge/contract-execution_semantics-brightgreen)](./docs/src/semantics.md)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

> **The durable execution layer for Rust.**
> Run important asynchronous Rust functions outside the immediate request path without requiring a separate message broker.

Azums is an embedded execution runtime for Rust applications. Applications register ordinary async
handlers; Azums manages the execution lifecycle around them: persistence, scheduling, claiming,
leases, heartbeats, attempts, retries, crash recovery, dead-letter handling, replay, event streams,
and observability.

Use the storage environment that fits the deployment:

- **Memory** for tests and ephemeral work.
- **SQLite** for embedded, desktop, CLI, edge, and single-binary applications.
- **PostgreSQL** for durable transactions and distributed workers.
- **Redis** for Redis-native distributed deployments.

The handler model remains portable. Backend capabilities remain explicit.

Azums provides **at-least-once execution**. It does not guarantee exactly-once execution or
exactly-once external side effects.

## Why Azums Exists

Rust applications regularly need to do work after the operation that requested it has finished:

- send email after account creation
- process payment webhooks
- call unreliable external APIs
- generate reports and exports
- index documents after database mutations
- process media uploads
- run AI inference or tool workflows
- defer telemetry synchronization on an edge device
- schedule work for later
- publish and replay durable events

`tokio::spawn` attaches that work to the lifetime of the current process:

```text
request -> tokio::spawn(handler()) -> process exits -> future disappears
```

Azums persists execution intent and controls what happens next:

```text
application
    |
    v
persist execution intent
    |
    v
claim -> lease -> heartbeat -> attempt -> handler -> ACK
                                    |
                                    +-> retry
                                    +-> cancel
                                    +-> DLQ
```

The central idea is simple:

> **Failure should not silently erase important work.**

Once a durable backend accepts committed work, Azums keeps its lifecycle recoverable until a
defined terminal outcome, subject to the selected backend's declared capabilities and worker
availability.

## Quickstart

```toml
[dependencies]
azums = "1.0"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
anyhow = "1"
```

Register a handler, enqueue work, execute it, and inspect the result:

```rust
use azums::{quickstart, Job};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = quickstart("memory").await?
        .with_queue("default");

    client
        .register_handler("greet", |job| async move {
            println!("Hello, {}!", job.payload["name"]);
            Ok(())
        })
        .await;

    let job_id = client
        .enqueue(
            Job::new("greet", serde_json::json!({ "name": "World" }))
                .queue("default")
                .max_attempts(5)
                .idempotency_key("greet:world"),
        )
        .await?;

    client.run_until_empty().await?;

    if let Some(explanation) = client.explain_job(job_id).await? {
        println!("{}", explanation.summary);
    }

    Ok(())
}
```

Run the complete install, enqueue, process, retry, and inspect example:

```bash
cargo run -p azums --example install_enqueue_process_retry_inspect
```

Change only the connection URL to select a backend:

```rust,no_run
# async fn example() -> anyhow::Result<()> {
let memory = azums::quickstart("memory").await?;
let sqlite = azums::quickstart("sqlite://jobs.db?mode=rwc").await?;
let postgres = azums::quickstart("postgres://user:pass@localhost/app").await?;
let redis = azums::quickstart("redis://127.0.0.1:6379").await?;
# Ok(())
# }
```

Portable API does not mean identical operational guarantees. Inspect requirements at startup:

```rust,no_run
# async fn example() -> anyhow::Result<()> {
let client = azums::quickstart(std::env::var("DATABASE_URL")?).await?;
let capabilities = client.capabilities();

anyhow::ensure!(capabilities.durable_jobs, "durable storage is required");
anyhow::ensure!(
    capabilities.distributed_workers,
    "this deployment runs workers on multiple hosts"
);
# Ok(())
# }
```

## Execution Model

The canonical lifecycle is:

```text
SCHEDULED
    |
    v
QUEUED
    |
    v
RUNNING ---------> COMPLETED
   | +-----------> CANCELLED
   +-------------> DLQ
   |
   v
RETRY_WAIT
   |
   +-------------> QUEUED
```

`COMPLETED`, `CANCELLED`, and `DLQ` are terminal. Invalid transitions are rejected. A running
mutation such as heartbeat, ACK, retry, DLQ, or cancellation must be made by the worker that owns
the valid lease.

If a worker disappears before ACK, its heartbeat stops, the lease expires, recovery records the
abandoned attempt where supported, and the job becomes executable again. The handler may already
have produced an external effect, which is why delivery is at least once.

## Jobs and Workers

Azums combines the primitives needed to run background work as infrastructure:

| Area | Included primitives |
|---|---|
| Job definition | ID, type, queue, JSON payload, typed deserialization, metadata, priority, schedule, deadline, timeout, retry budget, idempotency key |
| Execution | worker identity, exclusive claim, lease, heartbeat, durable attempt, handler timeout, ACK, graceful shutdown |
| Failure | retryable and permanent failures, panic isolation, system failures, exponential backoff, jitter, DLQ reason codes |
| Recovery | expired-lease reaping, retry, cancellation, replay lineage, retained execution history |
| Operations | lifecycle explanation, queue metrics, structured job event, maintenance and retention APIs |

Scheduling is based on **eligibility**, not exact wall-clock execution. A future job is not
intentionally leased before `run_at`; actual start time depends on worker availability, queue depth,
backend time, policy, and wake-up latency. A passed `deadline_at` moves eligible work to DLQ rather
than running it late.

## Durable Event Streams

Jobs answer, "What work must execute?" Streams answer, "What happened that consumers must observe?"

Azums streams provide append, monotonic stream-local sequence numbers, reads after an offset,
consumer-group offsets, monotonic ACK, replay, notifications, and retention-aware pruning.

```rust,no_run
use azums::quickstart;
use serde_json::json;

# async fn example() -> anyhow::Result<()> {
let client = quickstart("memory").await?;
let orders = client.stream("orders");

orders
    .publish("order_created", json!({ "order_id": "ord-1001" }))
    .await?;

for event in orders.read_next("billing", 100).await? {
    // Make this consumer's side effect idempotent before acknowledging.
    orders.ack("billing", event.sequence_no).await?;
}
# Ok(())
# }
```

Reading does not advance the offset. A crash before ACK can produce duplicate delivery. Consumer
groups track durable progress; they do not automatically assign partitions, balance members, or
provide exactly-once consumer execution.

## Backend Capabilities

One API is an architectural guarantee. Identical storage behavior is not.

| Capability | Memory | SQLite | PostgreSQL | Redis |
|---|---|---|---|---|
| Portable job and stream API | Yes | Yes | Yes | Yes |
| Job durability | Process-local | File-backed | Persistent database | Persistence/eviction dependent |
| Idempotency key | Process-local | Unique index | Unique index | Redis idempotency record |
| App-data transaction | No | Same SQLite database | Same PostgreSQL database | No cross-store transaction |
| Distributed workers | No | No | Yes | Yes |
| Notifications | In-process hint | In-process hint plus polling | `LISTEN/NOTIFY` hint | Pub/Sub hint plus polling |
| Lease ordering | FIFO and fastest | FIFO and fastest | FIFO and fastest | FIFO |
| Backpressure | Backlog | Backlog | Backlog or execution-rate policy | Backlog |
| Stream/group retention | Process lifetime | Explicit pruning | Explicit pruning | Backend configured |

Notifications improve wake-up latency; persisted backend state defines correctness. Retention,
durability, clock behavior, and notification delivery remain backend-dependent.

Read the complete [backend compatibility matrix](./docs/src/backend_equivalence.md).

## Transactional Enqueue

PostgreSQL and SQLite can commit application state and a job in the same database transaction. This
prevents the classic split result where a business mutation commits but its background work does
not, or a job becomes visible for application state that later rolls back.

```rust,no_run
use azums::{Job, PostgresBackend};
use serde_json::json;

async fn create_user(
    pool: &sqlx::PgPool,
    backend: &PostgresBackend,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("INSERT INTO users (id) VALUES ($1)")
        .bind("user-123")
        .execute(&mut *tx)
        .await?;

    backend
        .enqueue_in_tx(
            &mut tx,
            Job::new(
                "send_welcome_email",
                json!({ "user_id": "user-123" }),
            )
            .into(),
        )
        .await?;

    tx.commit().await?;
    Ok(())
}
```

Commit preserves both records. Rollback preserves neither. Azums does not coordinate distributed
transactions across Redis plus SQL, HTTP APIs, payment processors, or unrelated services.

## Idempotency

Azums separates two duplicate problems:

1. **Duplicate submission:** the same non-null `idempotency_key` maps repeated enqueue attempts to
   one logical job.
2. **Duplicate execution:** a handler can perform a side effect and crash before ACK, so recovery
   can invoke it again.

The enqueue key solves the first problem. The application must solve the second with a provider's
idempotency key or a unique application record committed with the side effect:

```sql
INSERT INTO processed_operations (operation_key, completed_at)
VALUES ($1, now())
ON CONFLICT (operation_key) DO NOTHING;
```

Replay intentionally creates a new job and does not deduplicate previous external effects.

## Guarantees

[Execution Semantics](./docs/src/semantics.md) is the canonical contract. Every important behavior
is classified as **Guaranteed**, **Backend-dependent**, or **Unspecified**.

### Guaranteed across the documented API

- at-least-once delivery for retained, runnable, non-cancelled work while workers are available
- rejection of illegal lifecycle transitions and terminal-state mutation
- at most one valid active lease per job
- recovery after lease expiry and reaping
- deterministic retry, failure classification, and DLQ transitions
- enqueue deduplication when a non-null idempotency key is supplied
- no intentional leasing before scheduling eligibility
- monotonic stream-local sequence numbers and consumer offsets
- replay through new work with preserved lineage

### Backend-dependent

- durability through process or machine failure
- transaction scope
- distributed worker coordination
- notification behavior and wake-up latency
- ordering strength, backend clocks, retention, and backpressure
- Redis persistence and eviction behavior

### Unspecified and not promised

- exactly-once handler execution or external side effects
- exact execution at `run_at`
- global ordering, parallel completion order, or worker fairness
- transactions across arbitrary external systems
- automatic consumer-group work balancing
- permanent retention, automatic scaling, compensation, or alerting
- cancellation undoing an external effect that already happened

## Where Azums Fits

Azums is a Rust execution primitive, not a web-framework-specific queue:

- **Web services:** email, billing, webhooks, indexing, exports, and transactional follow-up work.
- **AI systems:** inference, tools, agent tasks, timeouts, retries, and durable workflow events.
- **CLI and desktop:** SQLite-backed deferred work inside one distributable application.
- **Embedded and edge:** local telemetry, synchronization, and recoverable offline work.
- **Game backends:** asset processing, notifications, and asynchronous world tasks.
- **Data systems:** durable events, projection rebuilds, batch jobs, and replay.

Framework adapters provide ergonomic integration without changing the execution model:

- [`azums-axum`](https://crates.io/crates/azums-axum)
- [`azums-actix`](https://crates.io/crates/azums-actix)
- [`azums-poem`](https://crates.io/crates/azums-poem)
- [`azums-rocket`](https://crates.io/crates/azums-rocket)

## Observability

Azums keeps current state separate from execution history. Where the backend supports durable
observability, an explanation includes job ID, queue, status, attempts, workers, durations, retries,
errors, DLQ reason, replay lineage, and trace context.

```rust,no_run
# async fn example(client: &azums::Client, job_id: uuid::Uuid) -> anyhow::Result<()> {
if let Some(explanation) = client.explain_job(job_id).await? {
    println!("{}", explanation.summary);
    println!("status: {}", explanation.status);
    println!("retries: {}", explanation.retry_count);
    println!("last worker: {:?}", explanation.last_worker_id);
    println!("last error: {:?}", explanation.last_error);
}
# Ok(())
# }
```

The operational question is not only "Did it fail?" It is:

> **What happened, why, who owned the execution, and what can happen next?**

## Verification

The recorded 1.0 release evidence includes:

- full workspace, integration, documentation, property, and input-hardening suites
- lifecycle, lease recovery, heartbeat, retry, DLQ, idempotency, scheduling, and stream tests
- transaction commit, rollback, connection-loss, and failure-boundary tests
- 10,000 randomized memory chaos scenarios
- a 24-case stress matrix from 1 to 100 workers and 10,000 to 1,000,000 jobs
- 6.96 million completed job executions in the recorded large-scale matrix
- reproducible Criterion benchmarks and regression guards
- dependency audit and API compatibility checks
- 100% public API item documentation and 100% rustdoc example coverage for `azums`

These results mean no documented guarantee was known to be violated by that tested tree. They do
not prove that arbitrary infrastructure or external side effects can never fail. Exact commands,
conditions, caveats, and guarantee-to-test links live in
[Release Candidate Evidence](./docs/src/release_candidate.md).

## Performance

Correctness comes first, but durable execution still needs to be efficient. Azums includes
reproducible benchmark tooling:

```bash
cargo run -p azums --release --bin azums-perf
cargo bench -p azums
```

Results depend on backend, persistence configuration, hardware, worker count, payload shape, and
contention. Benchmark numbers are evidence, not semantic guarantees. See the
[live benchmark dashboard](https://blockforge-dev.github.io/azums/) for measured results and test
conditions.

## Documentation

Start with the path that matches your goal:

1. **Build something:** [Quickstart](./docs/src/quickstart.md) and
   [Developer Experience](./docs/src/developer_experience.md).
2. **Understand the product:** [Product and Implementation Handbook](./docs/src/product_handbook.md).
3. **Know exactly what is promised:** [Execution Semantics](./docs/src/semantics.md).
4. **Choose storage:** [Backend Equivalence](./docs/src/backend_equivalence.md).
5. **Understand internals:** [Architecture Overview](./ARCHITECTURE.md),
   [Architecture Book](./docs/src/architecture_book.md), and
   [Low-Level Design](./docs/architecture/LLD.md).
6. **Operate it:** [Production Deployment](./docs/src/production_deployment.md) and
   [Failure Recovery Runbook](./docs/src/failure_recovery_runbook.md).
7. **Audit the evidence:** [Primitive Correctness](./docs/src/primitive_correctness.md),
   [Chaos Engineering](./docs/src/chaos_engineering.md), and
   [Release Candidate Evidence](./docs/src/release_candidate.md).

API documentation is on [docs.rs](https://docs.rs/azums).

## Mental Model

```text
1. Durable intent
   job, schedule, priority, idempotency, stream event
                         |
                         v
2. Controlled execution
   claim, lease, heartbeat, attempt, handler, ACK, retry, DLQ
                         |
                         v
3. Explanation and recovery
   history, worker ownership, offsets, replay, metrics
```

The backend record is the source of truth. Notifications are wake-up hints. Leases make abandoned
execution recoverable. Retries make transient failure survivable. History makes failure
explainable. Capabilities keep backend promises honest.

> **Azums turns ordinary async Rust functions into recoverable, observable, durable execution.**

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](./CONTRIBUTING.md) and
[CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md).

Questions and design discussions belong in
[GitHub Discussions](https://github.com/BlockForge-Dev/azums/discussions). Bugs and implementation
issues belong in the [issue tracker](https://github.com/BlockForge-Dev/azums/issues).

## License

Azums is licensed under either [Apache License 2.0](./LICENSE-APACHE) or
[MIT License](./LICENSE-MIT), at your option.
