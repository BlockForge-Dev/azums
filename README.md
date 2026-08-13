# Azums 🦀⚡

[![Crates.io](https://img.shields.io/crates/v/azums.svg)](https://crates.io/crates/azums)
[![Docs.rs](https://docs.rs/azums/badge.svg)](https://docs.rs/azums)
[![📊 Live Benchmarks](https://img.shields.io/badge/%F0%9F%93%8A_Live_Benchmarks-Dashboard-blue)](https://blockforge-dev.github.io/azums/)
[![Fastest Rust Job Queue](https://img.shields.io/badge/Performance-Fastest_Rust_Job_Queue-brightgreen)](docs/src/comparison.md)
[![CI Status](https://github.com/BlockForge-Dev/azums/actions/workflows/ci.yml/badge.svg)](https://github.com/BlockForge-Dev/azums/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/BlockForge-Dev/azums#license)

> A lightweight, high-performance job queue. The optional web dashboard is available as a separate package (`azums-dashboard`).

`azums` is an enterprise-grade, transactional background job queue and streaming framework designed for Rust web applications, CLI tools, AI agents, and microservices. Built on top of PostgreSQL, SQLite, Redis, and In-Memory storage backends with native extractors for **Axum**, **Actix Web**, **Poem**, and **Rocket**.

Performance claims are benchmark-derived and reproducible through `azums-perf` and Criterion. See benchmark artifacts for backend, workload, worker count, and machine conditions before comparing numbers.

---

## ⚡ Quickstart ("Hello, World!")

Add `azums` to your `Cargo.toml`:

```toml
[dependencies]
azums = "0.2"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

Enqueue and process background jobs in under 2 minutes:

```rust
use azums::{quickstart, Job};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Connect to database (Postgres, SQLite, or "memory")
    let client = quickstart("memory").await?;

    // 2. Enqueue background job
    client.enqueue(Job::new("greet", serde_json::json!({"name": "World"}))).await?;

    // 3. Register job processing handler
    client.register_handler("greet", |job| async move {
        println!("Hello, {}!", job.payload["name"]);
        Ok(())
    }).await;

    // 4. Execute workers until queue is empty
    client.run_until_empty().await?;
    Ok(())
}
```

---

## 🆚 Why azums over other job queues?

| Feature / Queue | **azums** | BullMQ (Node) | Celery (Python) | Sidekiq (Ruby) | Factotum (Rust) |
|---|---|---|---|---|---|
| **Language** | Rust 🦀 | Node.js | Python | Ruby | Rust |
| **Backend Portability** | ✅ Single API for Postgres, SQLite, Redis, In-Memory | ❌ Redis only | ✅ Redis, RabbitMQ, etc. | ❌ Redis only | ❌ Postgres only |
| **Instant Wake‑up** | ✅ `LISTEN/NOTIFY`, Redis PubSub, zero polling | ✅ Redis PubSub | ✅ Broker‑dependent | ✅ Redis PubSub | ❌ Polling only |
| **Strict FIFO Ordering** | ✅ Per-queue configurable (default FIFO) | ⚠️ FIFO per queue | ❌ Best-effort | ⚠️ FIFO per queue | ❌ Best-effort |
| **Framework Integrations** | ✅ Axum, Actix, Poem, Rocket (native extractors) | ❌ None built‑in | ✅ Flask, Django, FastAPI | ❌ None | ❌ None |
| **Event Streams (Redis‑style)** | ✅ Durable stream logs with consumer groups & offsets | ✅ Native Redis Streams | ❌ Requires Celery Beat | ❌ No native streams | ❌ None |
| **Dead‑Letter Queue (DLQ)** | ✅ Automatic with retry exhaustion, reason codes, and replay | ✅ | ✅ | ✅ | ✅ |
| **Transactional Enqueue** | ✅ Backend-dependent; native for SQL backends | ❌ Separate store | ❌ | ❌ | ❌ (to some extent) |
| **Embedded / Edge Support** | ✅ SQLite & in‑memory backends for single‑binary deployment | ❌ Requires Redis process | ❌ | ❌ | ❌ |
| **Idle CPU Usage** | **0.0%** | Low | Medium | Low | High (polling) |
| **Max Throughput (enqueue)** | Benchmark-derived; see M14 report / dashboard | ~5,000–10,000/sec | ~2,000–5,000/sec | ~5,000–10,000/sec | ~8,500/sec |
| **Licensing** | MIT / Apache 2.0 | MIT | BSD | LGPL / Pro | MIT |
| **Open Source Dashboard** | 🚧 Coming as separate crate | ✅ Built‑in | ✅ Flower | ✅ Sidekiq UI | ❌ None |
| **Managed Cloud Offering** | 🚧 Beta planned | ✅ (via Redis Enterprise) | ✅ (Celery Cloud) | ✅ (Sidekiq Pro) | ❌ |

### 🔑 Key Differentiators

- **One library, any database.** Your code looks identical whether you use Postgres, SQLite, or Redis. No lock‑in.
- **Zero‑cost idle.** Unlike polling‑based Rust queues, `azums` workers consume no CPU when the queue is empty—saving battery and server costs.
- **Natively embedded.** Run a full job queue inside your CLI tool, desktop app, or IoT device with SQLite. No external services required.
- **Streams without a broker.** Durable, replayable event streams are built into the database backend—just like Redis Streams, but your existing database handles it.
- **First‑class Rust web framework support.** Drop `azums-axum` into your project and inject a `JobQueue` directly into route handlers—something no other Rust queue offers.

---

**🔗 See the [Live Benchmarks Dashboard](https://blockforge-dev.github.io/azums/) for reproducible, up‑to‑date performance numbers.**

---

## 🏗️ Storage Backend Compatibility

Swap storage backends effortlessly with zero application code changes:

| Backend | Connection URL | Feature Flag | Ideal Use Case |
|---|---|---|---|
| **PostgreSQL** | `postgres://user:pass@localhost/db` | `postgres` (default) | Multi-node Kubernetes microservices & production DBs |
| **SQLite** | `sqlite://jobs.db?mode=rwc` | `sqlite` (default) | Single-binary web apps, desktop tools, IoT edge devices |
| **Redis** | `redis://127.0.0.1:6379` | `redis` (default) | Ultra-low latency memory queue & native streams |
| **In-Memory** | `memory` | Core | Fast unit tests, CI test pipelines, zero disk I/O |

---

## 🌐 Native Web Framework Extractors

`azums` provides native request extractors for every popular Rust web framework:

```rust
// Axum route handler with JobQueue extractor
async fn create_user(queue: JobQueue, Json(payload): Json<UserPayload>) -> impl IntoResponse {
    let job_id = queue.enqueue_now("default", "welcome_email", json!(payload)).await?;
    Json(json!({ "status": "queued", "id": job_id }))
}
```

- [`azums-axum`](https://crates.io/crates/azums-axum) (Axum 0.7)
- [`azums-actix`](https://crates.io/crates/azums-actix) (Actix Web 4)
- [`azums-poem`](https://crates.io/crates/azums-poem) (Poem 3)
- [`azums-rocket`](https://crates.io/crates/azums-rocket) (Rocket 0.5)

---

## ⚡ Performance & Micro-Benchmarks

`azums` is built for maximum throughput with minimal overhead. Run the reproducible M14 harness and Criterion micro-benchmarks locally:

```bash
# Run the M14 matrix harness
cargo run -p azums --release --bin azums-perf

# Run Criterion micro-benchmarks
cargo bench -p azums
```

| Benchmark Target | Operation | Throughput / Speed |
|---|---|---|
| `enqueue_single_job` | Atomic In-Memory Enqueue | **> 100,000 ops/sec** |
| `worker_process_batch_100` | Lease, Attempt, Complete Batch | **< 1.5 ms / 100 jobs** |

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
- **[Azums Architecture Book](https://blockforge-dev.github.io/azums/)**: FOR UPDATE SKIP LOCKED leasing, DLQ sequence diagrams, and table partitioning.

---

## 💬 Community & Support

- **[GitHub Discussions](https://github.com/BlockForge-Dev/azums/discussions)**: Have questions, feature requests, or architecture ideas? Join our GitHub Discussions.
- **[Issue Tracker](https://github.com/BlockForge-Dev/azums/issues)**: Found a bug or issue? Report it on our GitHub Issues tracker.

---

## 🤝 Contributing & License

Contributions are welcome! Please read [`CONTRIBUTING.md`](./CONTRIBUTING.md) and [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md).

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT License](./LICENSE-MIT) at your option.
