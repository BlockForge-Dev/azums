#![cfg_attr(docsrs, feature(doc_auto_cfg))]
//! # PostgresFlow
//!
//! A Postgres-backed job queue with transactional leasing, dead-letter queues,
//! automatic retries with exponential backoff, time-partitioned tables, and an optional admin HTTP API.
//!
//! ## Quick Start
//!
//! Add `postgresflow` to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! postgresflow = "0.2"
//! ```
//!
//! Then run zero-config quickstart:
//!
//! ```rust,no_run
//! use postgresflow::{quickstart, Job};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let flow = quickstart("postgres://localhost/flow").await?;
//!     flow.enqueue(Job::new("greet", serde_json::json!({"name": "World"}))).await?;
//!     flow.register_handler("greet", |job| async move {
//!         println!("Hello, {}!", job.payload["name"]);
//!         Ok(())
//!     }).await;
//!     flow.run().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Core Capabilities
//!
//! - **ACID Enqueue**: Enqueue background jobs inside your application's SQL transactions with zero external message broker.
//! - **Transactional Leasing**: Safe multi-worker concurrency using Postgres `FOR UPDATE SKIP LOCKED`.
//! - **Time Partitioning**: Bounded table size via automatic monthly dataset partition routing and archiving.
//! - **Dead-Letter Queue (DLQ)**: Automatic retries with exponential backoff and explicit DLQ routing.
//! - **Admin Console & REST API**: Built-in visual management dashboard and Prometheus metrics endpoint.
//!
//! ## Documentation & Book
//!
//! For complete architecture deep-dives, lifecycle sequence diagrams, and operational guides,
//! visit the [PostgresFlow Book](https://blockforge-dev.github.io/postgresflow/).
//!
//! ## Feature Flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `api`   | ✅      | Axum-based admin HTTP API and web UI router |
//! | `cli`   | ✅      | `pgflowctl` CLI binary |
//!
//! To use postgresflow as a lightweight library without HTTP server components:
//!
//! ```toml
//! [dependencies]
//! postgresflow = { version = "0.2", default-features = false }
//! ```

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

pub use backend::PostgresBackend;
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
pub use postgresflow_core::{Job, JobListItem, JobStatus, NewJob, StorageBackend};
pub use quickstart::{quickstart, QuickstartFlow};
