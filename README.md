# Azums 🦀⚡

[![Crates.io](https://img.shields.io/crates/v/azums.svg)](https://crates.io/crates/azums)
[![Docs.rs](https://docs.rs/azums/badge.svg)](https://docs.rs/azums)
[![Fastest Rust Job Queue](https://img.shields.io/badge/Performance-Fastest_Rust_Job_Queue-brightgreen)](docs/PERFORMANCE_TUNING.md)
[![CI Status](https://github.com/BlockForge-Dev/azums/workflows/CI/badge.svg)](https://github.com/BlockForge-Dev/azums/actions)
[![License](https://img.shields.io/crates/l/azums.svg)](https://github.com/BlockForge-Dev/azums#license)

> A lightweight, high-performance job queue. The optional web dashboard is available as a separate package (`azums-dashboard`) coming soon.

`azums` is an enterprise-grade, transactional background job queue and streaming framework designed for Rust web applications, CLI tools, AI agents, and microservices. Built on top of PostgreSQL, SQLite, Redis, and In-Memory storage backends with native extractors for **Axum**, **Actix Web**, **Poem**, and **Rocket**.

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

`azums` is built for maximum throughput with minimal overhead. Run Criterion benchmarks locally:

```bash
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
- **[Azums Low-Level Design (LLD + DSA)](./docs/architecture/LLD.md)**: Deep-dive architecture specs, data structures, and algorithm complexity.
- **[Azums Architecture Book](https://blockforge-dev.github.io/azums/)**: FOR UPDATE SKIP LOCKED leasing, DLQ sequence diagrams, and table partitioning.

---

## 💬 Community & Support

- **[GitHub Discussions](https://github.com/BlockForge-Dev/azums/discussions)**: Have questions, feature requests, or architecture ideas? Join our GitHub Discussions.
- **[Issue Tracker](https://github.com/BlockForge-Dev/azums/issues)**: Found a bug or issue? Report it on our GitHub Issues tracker.

---

## 🤝 Contributing & License

Contributions are welcome! Please read [`CONTRIBUTING.md`](./CONTRIBUTING.md) and [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md).

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT License](./LICENSE-MIT) at your option.

