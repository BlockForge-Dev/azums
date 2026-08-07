#![cfg_attr(docsrs, feature(doc_auto_cfg))]
//! # PostgresFlow
//!
//! **The lightweight, Postgres-backed job queue for Rust — from embedded to cloud.**
//!
//! `postgresflow` delivers enterprise background job processing with ACID guarantees,
//! row-level FOR UPDATE SKIP LOCKED leasing, dead-letter queues (DLQ), exponential backoff retries,
//! and time-partitioned storage tables.
//!
//! ---
//!
//! ## Quickstart
//!
//! Add `postgresflow` to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! postgresflow = "0.2"
//! tokio = { version = "1", features = ["full"] }
//! serde = { version = "1", features = ["derive"] }
//! ```
//!
//! Run zero-config background job processing:
//!
//! ```rust,no_run
//! use postgresflow::{quickstart, Job};
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
//! ## Choosing a Backend
//!
//! `postgresflow` supports three storage backends under a unified [`StorageBackend`] interface:
//!
//! 1. **PostgreSQL** (`postgres://...`):
//!    - Multi-node Kubernetes clusters, production web applications, distributed workers.
//!    - Built on `sqlx` and `tokio-postgres` using `FOR UPDATE SKIP LOCKED`.
//! 2. **SQLite** (`sqlite://jobs.db?mode=rwc`):
//!    - Embedded CLI applications, desktop apps, single-server web deployments, IoT edge devices.
//!    - Runs in WAL mode for single-writer concurrency with zero network overhead.
//! 3. **In-Memory** (`memory`):
//!    - Ephemeral unit testing, local development, zero disk I/O test pipelines.
//!
//! ---
//!
//! ## Error Handling
//!
//! All queue operations return [`Error`] (aliased as [`QueueError`]):
//!
//! - Use [`job.payload_typed::<T>()`](postgresflow_core::Job::payload_typed) to automatically parse JSON payloads into strongly-typed structs.
//! - Unhandled failures automatically trigger retries up to `job.max_attempts` before moving to the Dead-Letter Queue (`status = "dlq"`).
//!
//! ---
//!
//! ## Deployment
//!
//! - **Single-Binary Service**: Use `postgresflow` inside your Axum, Actix, Poem, or Rocket application binary.
//! - **Separate Worker Nodes**: Run background workers independently using the [`worker`](https://crates.io/crates/worker) crate or `pgflowctl`.
//! - **Monitoring Dashboard**: Enable the `api` feature to expose an Axum-based web UI console and Prometheus `/metrics` endpoint.

#[cfg(feature = "api")]
pub mod admin;
#[cfg(feature = "api")]
pub mod api;

pub mod backend;
pub mod config;
pub mod db;
pub mod jobs;
pub mod quickstart;

// ── Convenience re-exports (stable public API) ──

#[cfg(feature = "postgres")]
pub use backend::PostgresBackend;
#[cfg(feature = "sqlite")]
pub use backend::{make_sqlite_pool, SqliteBackend};
pub use config::Config;
pub use db::{make_pool, run_migrations};
pub use jobs::attempts::AttemptsRepo;
pub use jobs::enqueue_guard::{EnqueueGuard, EnqueueGuardConfig};
pub use jobs::ingest_decisions::IngestDecisionsRepo;
pub use jobs::maintenance::MaintenanceRepo;
pub use jobs::metrics::MetricsRepo;
pub use jobs::policies::{PoliciesRepo, QueuePolicy};
pub use jobs::policy_decisions::{PolicyDecisionRow, PolicyDecisionsRepo};
pub use jobs::repo::JobsRepo;
pub use jobs::retry::RetryConfig;
pub use jobs::runner::JobRunner;
pub use postgresflow_core::{
    CallRecord, Error, Job, JobHandler, JobListItem, JobProcessor, JobStatus, MemoryBackend,
    MockBackend, NewJob, QueueError, StorageBackend,
};
pub use quickstart::{quickstart, Client, QuickstartFlow};
