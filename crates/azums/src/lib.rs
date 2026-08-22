#![cfg_attr(docsrs, feature(doc_cfg, rustdoc_missing_doc_code_examples))]
#![cfg_attr(docsrs, deny(rustdoc::missing_doc_code_examples))]
#![allow(clippy::double_must_use)]
#![deny(missing_docs)]
//! # Azums
//!
//! **The durable execution layer for Rust.**
//!
//! `azums` turns ordinary async Rust handlers into recoverable, retryable, observable at-least-once
//! execution across Memory, SQLite, PostgreSQL, and Redis. It manages persistence, scheduling,
//! leases, heartbeats, attempts, retries, dead-letter handling, replay, and durable event streams
//! without requiring a separate message broker.
//!
//! *Performance claims are benchmark-derived and reproducible through `azums-perf` and Criterion.*
//! See [Live Benchmark Dashboard](https://blockforge-dev.github.io/azums/) for current measured results and conditions.
//!
//! ---
//!
//! ## Quickstart
//!
//! Add `azums` to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! azums = "1.0"
//! tokio = { version = "1", features = ["full"] }
//! serde = { version = "1", features = ["derive"] }
//! ```
//!
//! Run zero-config background job processing:
//!
//! ```rust,no_run
//! use azums::{quickstart, Job};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct GreetPayload {
//!     name: String,
//! }
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let client = quickstart("memory").await?;
//!
//!     client.enqueue(Job::new("greet", serde_json::json!({"name": "World"}))).await?;
//!
//!     client.register_handler("greet", |job| async move {
//!         let payload: GreetPayload = job.payload_typed()?;
//!         println!("Hello, {}!", payload.name);
//!         Ok(())
//!     }).await;
//!
//!     client.run_until_empty().await?;
//!     Ok(())
//! }
//! ```
//!
//! ---
//!
//! ## Storage Backend Compatibility
//!
//! `azums` supports four storage backends under a unified [`StorageBackend`] interface:
//!
//! | Backend | Connection URL | Feature Flag | Ideal Use Case |
//! |---|---|---|---|
//! | **PostgreSQL** | `postgres://user:pass@localhost/db` | `postgres` (default) | Multi-node Kubernetes microservices & production DBs |
//! | **SQLite** | `sqlite://jobs.db?mode=rwc` | `sqlite` (default) | Single-binary web apps, desktop tools, IoT edge devices |
//! | **Redis** | `redis://127.0.0.1:6379` | `redis` (default) | Ultra-low latency memory queue & native streams |
//! | **In-Memory** | `memory` | Core | Fast unit tests, CI test pipelines, zero disk I/O |
//!
//! ---
//!
//! ## Error Handling
//!
//! All queue operations return [`Error`] (aliased as [`QueueError`]):
//!
//! - Use [`job.payload_typed::<T>()`](azums_core::Job::payload_typed) to automatically parse JSON payloads into strongly-typed structs.
//! - Unhandled failures automatically trigger retries up to `job.max_attempts` before moving to the Dead-Letter Queue (`status = "dlq"`).
//!
//! ---
//!
//! ## Deployment
//!
//! - **Single-Binary Service**: Use `azums` inside your Axum, Actix, Poem, or Rocket application binary.
//! - **Separate Worker Nodes**: Run background workers independently using the [`worker`](https://crates.io/crates/worker) crate or `azumsctl`.
//! - **Monitoring Dashboard**: The optional web dashboard is available as a separate package (`azums-dashboard`).

/// Storage backend adapters and backend-specific constructors.
pub mod backend;
/// Environment-driven runtime configuration.
pub mod config;
/// PostgreSQL pool and migration helpers.
pub mod db;
/// Job repositories, execution policies, attempts, and operational views.
pub mod jobs;
/// High-level client, handler registry, and Tokio worker runtime.
pub mod quickstart;
/// High-level durable event stream handle.
pub mod stream_handle;

// Convenience re-exports forming the stable public API.

pub use azums_core::{
    semantic_contract, BackendCapabilities, BackendSemanticCapabilities, BackpressureCapability,
    CallRecord, ConsumerGroupCapability, ConsumerGroupStatus, DurabilityCapability, Error, Event,
    Job, JobExecution, JobHandler, JobLifecycleState, JobListItem, JobProcessor, JobStatus,
    MemoryBackend, MockBackend, NewEvent, NewJob, NotificationCapability, NotificationStream,
    OrderingCapability, Queue, QueueConfig, QueueError, QueueOrdering, RetentionCapability,
    SemanticBehavior, SemanticClassification, SemanticContract, StorageBackend, StreamBackend,
    TransactionalEnqueueCapability, Worker,
};
#[cfg(feature = "postgres")]
pub use backend::PostgresBackend;
#[cfg(feature = "redis")]
pub use backend::RedisBackend;
#[cfg(feature = "sqlite")]
pub use backend::{make_sqlite_pool, SqliteBackend};
pub use config::Config;
pub use db::{make_pool, run_migrations};
pub use jobs::attempts::{AttemptsRepo, JobAttempt};
pub use jobs::enqueue_guard::{EnqueueGuard, EnqueueGuardConfig};
pub use jobs::ingest_decisions::IngestDecisionsRepo;
pub use jobs::maintenance::{MaintenanceRepo, TableMaintenanceInfo};
pub use jobs::metrics::MetricsRepo;
pub use jobs::policies::{PoliciesRepo, QueuePolicy};
pub use jobs::policy_decisions::{PolicyDecisionRow, PolicyDecisionsRepo};
pub use jobs::repo::JobsRepo;
pub use jobs::retry::RetryConfig;
pub use jobs::runner::JobRunner;
pub use quickstart::{quickstart, Client, QuickstartFlow};
pub use stream_handle::StreamHandle;
