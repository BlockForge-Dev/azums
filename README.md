
# Azums 🦀⚡

[![Crates.io](https://img.shields.io/crates/v/azums.svg)](https://crates.io/crates/azums)
[![Docs.rs](https://docs.rs/azums/badge.svg)](https://docs.rs/azums)
[![📊 Live Benchmarks](https://img.shields.io/badge/%F0%9F%93%8A_Live_Benchmarks-Dashboard-blue)](https://blockforge-dev.github.io/azums/)
[![Durable Execution](https://img.shields.io/badge/Rust-Durable_Execution_Layer-brightgreen)](./docs/src/semantics.md)
[![CI Status](https://github.com/BlockForge-Dev/azums/actions/workflows/ci.yml/badge.svg)](https://github.com/BlockForge-Dev/azums/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/BlockForge-Dev/azums#license)

> **The durable execution layer for Rust.**
> Run important async Rust functions outside the immediate request path without introducing a separate message broker.

Azums is an embedded execution runtime for Rust applications.

Applications register ordinary async Rust handlers. Azums takes responsibility for the execution lifecycle around those handlers: persistence, scheduling, claiming, leasing, heartbeats, retries, crash recovery, dead-letter handling, replay, execution history, and observability.

It runs on the storage environment that fits your application:

* **Memory** for tests and ephemeral workloads
* **SQLite** for embedded, desktop, CLI, edge, and single-binary applications
* **PostgreSQL** for durable transactional applications and distributed workers
* **Redis** for Redis-native distributed deployments

You do **not** need Redis, Kafka, RabbitMQ, or another broker just to execute background Rust functions reliably.

If your architecture already uses Redis, Azums can use it. If your application already depends on PostgreSQL, Azums can keep execution state there. If you are building an embedded application, SQLite is enough.

The programming model remains the same while the operational capabilities of each backend remain explicit.

---

## Why Azums Exists

Rust applications frequently need to perform work after the operation that requested it has finished:

* send an email after creating an account
* process a payment webhook
* call an unreliable external API
* generate reports or exports
* index documents after a database mutation
* process uploaded media
* execute long-running AI inference or tool workflows
* process telemetry from edge devices
* schedule work for later
* publish durable events
* rebuild projections from event history

You can use `tokio::spawn`:

```text
request
   |
   v
tokio::spawn(handler())
```

but the task belongs to the lifetime of that process.

If the process disappears, so does the in-memory future.

Azums changes the model:

```text
application
    |
    | submit durable work
    v
  Azums
    |
    +--> persist execution intent
    |
    +--> lease to worker
    |
    +--> run handler
    |
    +--> heartbeat ownership
    |
    +--> record attempt
    |
    +--> complete
    |
    +--> retry after failure
    |
    +--> recover after worker loss
    |
    +--> DLQ when execution cannot continue
```

The central idea is:

> **Failure should not silently erase an important execution.**

Once durable work is successfully accepted, Azums retains responsibility for its execution lifecycle until it reaches a defined terminal outcome, subject to the guarantees of the selected backend and worker availability.

Azums provides **at-least-once execution**, not exactly-once external side effects.

---

## ⚡ Quickstart

Add Azums:

```toml
[dependencies]
azums = "1.0"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
anyhow = "1"
```

Register a handler, enqueue work, and execute it:

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
            Job::new(
                "greet",
                serde_json::json!({ "name": "World" }),
            )
            .queue("default")
            .max_attempts(5)
            .idempotency_key("greet:world"),
        )
        .await?;

    client.run_until_empty().await?;

    println!("{:?}", client.explain_job(job_id).await?);

    Ok(())
}
```

The same handler model works across supported storage environments:

```rust
let memory = quickstart("memory").await?;

let sqlite =
    quickstart("sqlite://jobs.db?mode=rwc").await?;

let postgres =
    quickstart("postgres://user:pass@localhost/app").await?;

let redis =
    quickstart("redis://127.0.0.1:6379").await?;
```

The API is portable.

The guarantees are not assumed to be identical.

Azums exposes backend differences through `BackendCapabilities`.

For the progressive install → enqueue → process → retry → inspect path:

```bash
cargo run -p azums --example install_enqueue_process_retry_inspect
```

Then read [Developer Experience & Integration](./docs/src/developer_experience.md).

---

## 🧠 The Execution Model

Azums is built around a controlled execution lifecycle:

```text
SCHEDULED
    |
    v
QUEUED
    |
    v
RUNNING ---------> COMPLETED
   | |
   | +-----------> CANCELLED
   |
   +-------------> DLQ
   |
   v
RETRY_WAIT
   |
   +-------------> QUEUED
```

`COMPLETED`, `CANCELLED`, and `DLQ` are terminal states.

Every transition outside the documented lifecycle is illegal.

While work executes, Azums uses:

```text
CLAIM
  |
  v
LEASE
  |
  v
HEARTBEAT
  |
  v
ATTEMPT
  |
  v
HANDLER
  |
  v
ACK
```

The lease answers:

> Which worker currently owns the right to execute and mutate this running job?

The heartbeat answers:

> Is that worker still alive?

The attempt records:

> What happened during this invocation?

The ACK records successful completion.

If a worker disappears before completion:

```text
worker disappears
       |
       v
heartbeat stops
       |
       v
lease expires
       |
       v
abandoned execution is recorded
       |
       v
work becomes recoverable
```

This is why Azums provides at-least-once execution.

---

## 🛡️ Durable Execution, Not Just Queueing

Azums combines several execution primitives under one runtime.

### Durable Jobs

* individual and batch enqueue
* named job types
* queues
* arbitrary JSON payloads
* typed payload deserialization
* priorities
* idempotency keys
* delayed execution
* execution deadlines
* recurring execution
* replay lineage

### Worker Runtime

* Tokio-native execution
* handler registration
* worker identities
* batch leasing
* exclusive leases
* heartbeat extension
* handler timeouts
* graceful shutdown
* notification wake-up with polling fallback
* periodic expired-lease recovery

### Failure Handling

* retryable failures
* permanent failures
* timeouts
* panic isolation
* system failures
* cancellation
* configurable retry budgets
* exponential backoff
* jitter
* dead-letter queue
* DLQ inspection
* replay

### Execution History

Azums separates current job state from execution history.

A durable job can retain information about:

* current lifecycle state
* attempt count
* workers
* execution timings
* failures
* retry history
* DLQ reason
* replay lineage
* trace information

This allows an application to ask:

> What happened?

instead of reconstructing execution from unstructured logs.

---

## 🔁 Durable Event Streams

Jobs answer:

> **What work must be executed?**

Streams answer:

> **What happened that consumers must be able to observe?**

Azums supports durable event streams with:

* append-only events
* monotonic stream-local sequence numbers
* consumer groups
* durable offsets
* monotonic ACK
* independent consumer progress
* replay
* retention-aware pruning
* notification subscriptions

Example:

```rust
use azums::{quickstart, NewEvent};
use serde_json::json;

async fn example() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let orders = client.stream("orders");

    let sequence = orders
        .publish(
            "order_created",
            json!({ "order_id": "ord-1001" }),
        )
        .await?;

    let events = orders.read_next("billing", 100).await?;

    for event in events {
        // Process the event.

        orders
            .ack("billing", event.sequence_no)
            .await?;
    }

    println!("Published sequence {sequence}");

    Ok(())
}
```

Reading an event does not automatically advance the consumer offset.

If a consumer crashes before ACK, the event may be delivered again.

Streams therefore use the same durability philosophy as jobs:

> Persist responsibility and make recovery explicit.

---

## 🗄️ Use the Storage You Already Need

Azums does not require a dedicated Azums database or a separate broker.

### Memory

```text
Rust application
      |
    Azums
      |
   Memory
```

Best for:

* tests
* local development
* short-lived workloads

Memory is process-local and non-durable.

### SQLite

```text
Rust application
      |
    Azums
      |
    SQLite
```

Best for:

* embedded systems
* desktop software
* edge applications
* CLI tools
* single-binary deployments
* single-process services

SQLite provides durable local storage without operating another service.

### PostgreSQL

```text
Application instances
         |
       Azums
         |
    PostgreSQL
         |
   Worker instances
```

Best for:

* production backend services
* multi-host workers
* microservices
* transactional applications
* applications already using PostgreSQL

PostgreSQL can also provide same-database transactional enqueue.

### Redis

```text
Application
     |
   Azums
     |
   Redis
```

Best for deployments that already want Redis-native distributed execution and have intentionally configured Redis persistence and eviction behavior.

Redis is supported.

Redis is not required.

---

## 🔐 Transactional Enqueue

One of the most important failure boundaries in background execution is:

```text
application mutation succeeds
           |
           v
enqueue fails
```

or the inverse:

```text
job becomes visible
       |
       v
application transaction rolls back
```

With SQLite and PostgreSQL, application data and Azums work can share the same database transaction:

```rust
use azums::{Job, PostgresBackend};
use serde_json::json;

async fn create_user(
    pool: &sqlx::PgPool,
    backend: &PostgresBackend,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO users (id) VALUES ($1)"
    )
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

The boundary is precise:

```text
BEGIN
  application mutation
  Azums enqueue
COMMIT
```

Commit preserves both.

Rollback preserves neither.

Azums does not claim transactions across unrelated external systems.

---

## ♻️ Idempotency and At-Least-Once Execution

Two different duplicate problems exist.

### Duplicate submission

```text
100 enqueue calls
same idempotency key
        |
        v
one logical job
```

### Duplicate execution

```text
handler performs external effect
            |
            v
worker crashes before ACK
            |
            v
job is recovered
            |
            v
handler may execute again
```

An enqueue idempotency key cannot make an arbitrary external side effect exactly once.

For external systems, use a stable idempotency key:

```rust
let external_key =
    format!("azums-job:{}", job.id);

payment_api
    .charge_with_idempotency_key(
        external_key,
        amount,
    )
    .await?;
```

For database effects, record the processed job ID or stream sequence under a unique constraint in the same transaction as the application mutation.

Azums makes duplicate execution **visible and controllable**.

It does not pretend distributed side effects are magically exactly once.

---

## 🏗️ Backend Capabilities

One API does not mean every backend provides the same operational guarantees.

| Capability                          |           Memory |               SQLite |                         PostgreSQL |                      Redis |
| ----------------------------------- | ---------------: | -------------------: | ---------------------------------: | -------------------------: |
| Portable job API                    |                ✅ |                    ✅ |                                  ✅ |                          ✅ |
| Durable jobs                        |                ❌ |                    ✅ |                                  ✅ |    Configuration-dependent |
| Idempotent enqueue                  |    Process-local |                    ✅ |                                  ✅ |                          ✅ |
| Same-database transactional enqueue |                ❌ |                    ✅ |                                  ✅ |                          ❌ |
| Streams                             |                ✅ |                    ✅ |                                  ✅ |                          ✅ |
| Consumer offsets                    |    Process-local |                    ✅ |                                  ✅ |                          ✅ |
| Distributed workers                 |                ❌ |                    ❌ |                                  ✅ |                          ✅ |
| Notifications                       |       In-process | In-process + polling | `LISTEN/NOTIFY` + polling fallback | Pub/Sub + polling fallback |
| Retention                           | Process lifetime | Explicit maintenance |               Explicit maintenance |          Backend-dependent |
| Execution-rate policies             |          Backlog |              Backlog |                                  ✅ |                    Backlog |

Applications can inspect these guarantees at runtime:

```rust
let client = azums::quickstart(
    std::env::var("DATABASE_URL")?
).await?;

let capabilities = client.capabilities();

anyhow::ensure!(
    capabilities.durable_jobs,
    "this deployment requires durable jobs"
);

anyhow::ensure!(
    capabilities.distributed_workers,
    "this deployment runs workers on multiple hosts"
);
```

The rule is:

> **Same programming model. Explicit operational differences.**

---

## 🌍 One Execution Model Across Rust Applications

Azums is not tied to web servers.

The same execution primitive can be used for:

### Web Backends

```text
HTTP request
    |
    +--> database mutation
    |
    +--> Azums job
              |
              +--> email
              +--> webhook
              +--> billing
              +--> indexing
```

### AI Systems

```text
AI request
    |
    +--> inference job
    +--> tool workflow
    +--> agent task
    +--> durable event
    +--> retry / timeout / recovery
```

### Embedded and Edge Systems

```text
device
  |
Azums
  |
SQLite
  |
telemetry / sync / deferred work
```

### Gaming

```text
game backend
     |
   Azums
     |
     +--> asset processing
     +--> asynchronous world tasks
     +--> notifications
     +--> durable state workflows
```

### CLI and Desktop Applications

```text
application
    |
  Azums
    |
 SQLite
```

No external broker process is required.

---

## 🌐 Rust Framework Integrations

Azums ships integration crates for common Rust web frameworks:

* [`azums-axum`](https://crates.io/crates/azums-axum)
* [`azums-actix`](https://crates.io/crates/azums-actix)
* [`azums-poem`](https://crates.io/crates/azums-poem)
* [`azums-rocket`](https://crates.io/crates/azums-rocket)

Example integration:

```rust
async fn create_user(
    queue: JobQueue,
    Json(payload): Json<UserPayload>,
) -> impl IntoResponse {
    let job_id = queue
        .enqueue_now(
            "default",
            "welcome_email",
            json!(payload),
        )
        .await?;

    Json(json!({
        "status": "queued",
        "id": job_id
    }))
}
```

Web framework integration is convenience around the same Azums execution model.

---

## 📜 What Azums 1.0 Promises

The most important source of truth is [Execution Semantics](./docs/src/semantics.md).

Every important behavior is classified as:

```text
Guaranteed
Backend-dependent
Unspecified
```

### Portable guarantees include

* at-least-once delivery for retained runnable work while workers are available
* rejection of illegal lifecycle transitions
* terminal states remain terminal
* at most one valid active lease for a job
* expired-lease recovery
* deterministic failure classification
* retry and DLQ behavior
* non-null idempotency keys identify one logical job
* no intentional leasing before scheduling eligibility
* monotonically increasing stream sequences
* monotonically advancing consumer offsets
* replay with preserved lineage and history

### Backend-dependent behavior includes

* durability through process or machine failure
* transaction scope
* worker distribution
* ordering strength
* notifications
* wake-up latency
* retention
* backpressure
* Redis persistence and eviction behavior
* backend clock behavior

### Azums explicitly does not guarantee

* exactly-once execution
* exactly-once external side effects
* exact wall-clock execution time
* global completion ordering
* worker fairness
* transactions across arbitrary external services
* automatic consumer-group balancing
* permanent retention
* automatic scaling
* cancellation undoing an external effect that already happened

Stable semantics matter more than a benchmark number.

---

## 🧪 Tested for Failure, Not Only the Happy Path

Azums 1.0 was tested against the types of failures its execution contract is designed to survive.

Release evidence includes:

* full workspace test suite
* lifecycle invariant tests
* property-based execution tests
* malformed-input and fuzz-hardening tests
* worker crash and lease-recovery tests
* heartbeat and wrong-owner tests
* transaction commit and rollback boundaries
* connection-loss and failure-boundary tests
* retry and DLQ tests
* idempotency scenarios
* scheduling and timeout tests
* stream replay and consumer-offset tests
* randomized chaos scenarios
* concurrency testing from 1 to 100 workers
* large-scale runs reaching one million jobs per matrix case
* 6.96 million job executions in the final recorded large-scale matrix
* benchmark regression gates
* documentation build
* dependency security audit
* API compatibility checks

The release evidence means:

> No known documented Azums 1.0 guarantee was violated by the tested release.

It does **not** mean arbitrary infrastructure or external side effects can never fail.

See [Release Candidate Evidence](./docs/src/release_candidate.md).

---

## ⚡ Performance

Correctness comes first, but durable execution still needs to be fast.

Azums includes reproducible benchmark tooling:

```bash
cargo run -p azums --release --bin azums-perf

cargo bench -p azums
```

Performance numbers depend on:

* backend
* hardware
* worker count
* job shape
* persistence configuration
* contention
* benchmark version

For that reason, benchmark results are evidence, not semantic guarantees.

**[View the live benchmark dashboard](https://blockforge-dev.github.io/azums/).**

---

## 📊 Observability

Azums can explain execution state directly:

```rust
if let Some(explanation) =
    client.explain_job(job_id).await?
{
    println!("{}", explanation.summary);
    println!("status: {}", explanation.status);
    println!(
        "retries: {}",
        explanation.retry_count
    );
    println!(
        "last worker: {:?}",
        explanation.last_worker_id
    );
    println!(
        "last error: {:?}",
        explanation.last_error
    );
}
```

Queue metrics include information such as:

* queue depth
* completions
* failures
* retries
* DLQ count
* execution latency
* claim information
* worker counts where measurable

The goal is not merely to know that something failed.

The goal is to answer:

> **What happened, why did it happen, who owned the execution, and what can happen next?**

---

## 🚀 Start Small, Scale Without Relearning the Execution Model

A project can begin with:

```text
Tests
  |
Azums
  |
Memory
```

move to:

```text
CLI / Desktop / Edge
        |
      Azums
        |
      SQLite
```

and later operate:

```text
Application instances
         |
       Azums
         |
    PostgreSQL
         |
   Worker instances
```

without changing the fundamental handler model.

That is one of Azums' core design goals:

> **The execution model should remain understandable as the deployment grows.**

---

## 📚 Documentation

### Start Here

* **[Azums Product and Implementation Handbook](./docs/src/handbook.md)** — Product model, implementation walkthrough, guarantees, release evidence, and learning path.
* **[Execution Semantics](./docs/src/semantics.md)** — Canonical source of truth for Azums guarantees.
* **[Developer Experience & Integration](./docs/src/developer_experience.md)** — Install-to-production adoption path.
* **[Storage Backend Equivalence](./docs/src/backend_equivalence.md)** — Capability matrix across Memory, SQLite, PostgreSQL, and Redis.

### Reliability

* **[Job Lifecycle](./docs/src/job_lifecycle.md)**
* **[Lease Recovery](./docs/src/leasing.md)**
* **[Retry, Failure Classification & DLQ](./docs/src/failure_handling.md)**
* **[Idempotency & Duplicate Execution](./docs/src/idempotency.md)**
* **[Transactional Integrity](./docs/src/transactional_integrity.md)**
* **[Scheduling & Time Semantics](./docs/src/time_semantics.md)**

### Streams and Operations

* **[Durable Event Streaming](./docs/src/event_streaming.md)**
* **[Concurrency & Backpressure](./docs/src/concurrency_backpressure.md)**
* **[Observability](./docs/src/observability.md)**
* **[Production Deployment](./docs/src/production_deployment.md)**
* **[Failure & Recovery Runbook](./docs/src/failure_recovery_runbook.md)**

### Architecture and Release Evidence

* **[Architecture Overview](./ARCHITECTURE.md)**
* **[Azums Low-Level Design](./docs/architecture/LLD.md)**
* **[Primitive Correctness](./docs/src/primitive_correctness.md)**
* **[Chaos Engineering](./docs/src/chaos_engineering.md)**
* **[Property Testing](./docs/src/property_testing.md)**
* **[Fuzzing & Input Hardening](./docs/src/fuzzing_input_hardening.md)**
* **[Release Candidate Evidence](./docs/src/release_candidate.md)**
* **[API Stability Policy](./STABILITY.md)**

API documentation is available on [docs.rs](https://docs.rs/azums).

---

## 🎯 The Mental Model

Think of Azums as three related layers:

```text
1. Durable Intent

   Job
   schedule
   priority
   idempotency
   stream event

            |
            v

2. Controlled Execution

   claim
   lease
   heartbeat
   attempt
   handler
   ACK
   retry
   DLQ

            |
            v

3. Explanation & Recovery

   execution history
   worker history
   consumer offsets
   replay
   metrics
   lifecycle explanation
```

The durable record is the source of truth.

Notifications improve wake-up latency but do not define correctness.

Leases make abandoned execution recoverable.

Retries make transient failure survivable.

Execution history makes failure explainable.

Backend capabilities keep operational promises honest.

And the application remains responsible for making external side effects safe under at-least-once execution.

> **Azums turns ordinary async Rust functions into recoverable, observable, durable execution.**

---


## 🤝 Contributing

Contributions are welcome.

Please read:

* [`CONTRIBUTING.md`](./CONTRIBUTING.md)
* [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md)

---


## 📚 Documentation & Book

- **[Docs.rs API Guide](https://docs.rs/azums)**: Comprehensive module documentation & inline examples.
- **[Architecture & Technical Design (ARCHITECTURE.md)](./ARCHITECTURE.md)**: State machine, `FOR UPDATE SKIP LOCKED` leasing algorithm, phantom recovery, and partitioning.
- **[Azums Low-Level Design (LLD + DSA)](./docs/architecture/LLD.md)**: Deep-dive architecture specs, data structures, and algorithm complexity.
- **[Execution Semantics](./docs/src/semantics.md)**: Canonical guarantee matrix for scheduling, DLQ, idempotency, transactional enqueue, streams, consumer groups, replay, and cancellation.
- **[Storage Backend Equivalence](./docs/src/backend_equivalence.md)**: Runtime capability model and compatibility matrix for Memory, SQLite, PostgreSQL, and Redis.
- **[Transactional Integrity](./docs/src/transactional_integrity.md)**: Commit/rollback contract for SQL transactional enqueue.
- **[Retry, Failure Classification & DLQ](./docs/src/failure_handling.md)**: Deterministic failure classes, backoff policy, DLQ inspection, and replay.
- **[Idempotency & Duplicate Execution](./docs/src/idempotency.md)**: Enqueue dedupe keys and application-side side-effect idempotency.
- **[Developer Experience & Integration](./docs/src/developer_experience.md)**: Install-to-inspect adoption path and integration notes.
- **[Azums Architecture Book](https://blockforge-dev.github.io/azums/)**: FOR UPDATE SKIP LOCKED leasing, DLQ sequence diagrams, and table partitioning.

---

## 💬 Community & Support

- **[GitHub Discussions](https://github.com/BlockForge-Dev/azums/discussions)**: Have questions, feature requests, or architecture ideas? Join our GitHub Discussions.
- **[Issue Tracker](https://github.com/BlockForge-Dev/azums/issues)**: Found a bug or issue? Report it on our GitHub Issues tracker.

---

## 🤝 Contributing & License

Contributions are welcome! Please read [`CONTRIBUTING.md`](./CONTRIBUTING.md) and [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md).

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT License](./LICENSE-MIT) at your option.
