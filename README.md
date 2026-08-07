# PostgresFlow 🦀⚡

[![Crates.io](https://img.shields.io/crates/v/postgresflow.svg)](https://crates.io/crates/postgresflow)
[![Docs.rs](https://docs.rs/postgresflow/badge.svg)](https://docs.rs/postgresflow)
[![CI Status](https://github.com/BlockForge-Dev/postgresflow/workflows/CI/badge.svg)](https://github.com/BlockForge-Dev/postgresflow/actions)
[![License](https://img.shields.io/crates/l/postgresflow.svg)](https://github.com/BlockForge-Dev/postgresflow#license)

> **The lightweight, Postgres-backed job queue for Rust — from embedded to cloud.**

`postgresflow` is an enterprise-grade, transactional background job queue designed for Rust web applications, CLI tools, and microservices. Built on top of PostgreSQL, SQLite, and In-Memory storage backends with native extractors for **Axum**, **Actix Web**, **Poem**, and **Rocket**.

---

## ⚡ Quickstart ("Hello, World!")

Add `postgresflow` to your `Cargo.toml`:

```toml
[dependencies]
postgresflow = "0.2"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

Enqueue and process background jobs in under 2 minutes:

```rust
use postgresflow::{quickstart, Job};

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

## 🏗️ Storage Backend Flexibility

Swap storage backends effortlessly with zero application code changes:

| Backend | Connection URL | Ideal Use Case |
|---|---|---|
| **PostgreSQL** | `postgres://user:pass@localhost/db` | Multi-node Kubernetes microservices & production DBs |
| **SQLite** | `sqlite://jobs.db?mode=rwc` | Single-binary web apps, desktop tools, IoT edge devices |
| **In-Memory** | `memory` | Fast unit tests, CI test pipelines, zero disk I/O |

---

## 🌐 Native Web Framework Extractors

`postgresflow` provides native request extractors for every popular Rust web framework:

```rust
// Axum route handler with JobQueue extractor
async fn create_user(queue: JobQueue, Json(payload): Json<UserPayload>) -> impl IntoResponse {
    let job_id = queue.enqueue_now("default", "welcome_email", json!(payload)).await?;
    Json(json!({ "status": "queued", "id": job_id }))
}
```

- [`postgresflow-axum`](https://crates.io/crates/postgresflow-axum) (Axum 0.7)
- [`postgresflow-actix`](https://crates.io/crates/postgresflow-actix) (Actix Web 4)
- [`postgresflow-poem`](https://crates.io/crates/postgresflow-poem) (Poem 3)
- [`postgresflow-rocket`](https://crates.io/crates/postgresflow-rocket) (Rocket 0.5)

---

## ⚡ Performance & Micro-Benchmarks

`postgresflow` is built for maximum throughput with minimal overhead. Run Criterion benchmarks locally:

```bash
# Run Criterion micro-benchmarks
cargo bench -p postgresflow
```

| Benchmark Target | Operation | Throughput / Speed |
|---|---|---|
| `enqueue_single_job` | Atomic In-Memory Enqueue | **> 100,000 ops/sec** |
| `worker_process_batch_100` | Lease, Attempt, Complete Batch | **< 1.5 ms / 100 jobs** |

---

## 📚 Documentation & Book

- **[Docs.rs API Guide](https://docs.rs/postgresflow)**: Comprehensive module documentation & inline examples.
- **[PostgresFlow Architecture Book](https://blockforge-dev.github.io/postgresflow/)**: Architecture deep-dives, FOR UPDATE SKIP LOCKED leasing, DLQ sequence diagrams, and table partitioning.

---

## 🤝 Contributing & License

Contributions are welcome! Please read [`CONTRIBUTING.md`](./CONTRIBUTING.md) and [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md).

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT License](./LICENSE-MIT) at your option.
