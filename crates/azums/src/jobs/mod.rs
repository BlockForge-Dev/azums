//! Job queue core: models, repository, retry logic, and execution runner.
//!
//! This module contains both stable and unstable sub-modules.
//! See [`STABILITY.md`](https://github.com/BlockForge-Dev/azums/blob/main/STABILITY.md)
//! for the full API stability policy.

/// Durable execution-attempt records and repository operations.
pub mod attempts;
/// Producer-side payload and enqueue-rate admission controls.
pub mod enqueue_guard;
/// Canonical operational error codes and remediation guidance.
pub mod error_codes;
/// Audit records for producer admission decisions.
pub mod ingest_decisions;
/// Compatibility re-exports for core job models.
pub mod model;
/// PostgreSQL queue execution policies.
pub mod policies;
/// Durable audit records for worker policy decisions.
pub mod policy_decisions;
/// PostgreSQL job persistence and lifecycle mutations.
pub mod repo;
/// Retry configuration, failure classification, and backoff calculations.
pub mod retry;
/// Repository-oriented job completion and failure coordinator.
pub mod runner;
/// PostgreSQL durable event stream repository.
pub mod stream_repo;

pub use attempts::AttemptsRepo;
pub use model::{Job, JobStatus, NewJob};
pub use policies::{PoliciesRepo, QueuePolicy};
pub use policy_decisions::{PolicyDecisionRow, PolicyDecisionsRepo};
pub use repo::JobsRepo;
pub use stream_repo::StreamRepo;

// ── Unstable modules ──
// These modules may change in minor versions before 1.0.

/// ⚠️ **Unstable API** — This module's interface may change in minor versions before 1.0.
///
/// Job execution timeline reconstruction for audit and debugging.
pub mod timeline;

/// ⚠️ **Unstable API** — This module's interface may change in minor versions before 1.0.
///
/// Debug view for inspecting job state, attempts, and policy decisions.
pub mod debug_view;

/// ⚠️ **Unstable API** — This module's interface may change in minor versions before 1.0.
///
/// Maintenance operations: archival, pruning, and retention management.
pub mod maintenance;
pub use maintenance::{cutoff_days, MaintenanceRepo};

/// ⚠️ **Unstable API** — This module's interface may change in minor versions before 1.0.
///
/// Queue-level metrics aggregation.
pub mod metrics;
pub use metrics::{Metrics, MetricsRepo};
