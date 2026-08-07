//! Job queue core: models, repository, retry logic, and execution runner.
//!
//! This module contains both stable and unstable sub-modules.
//! See [`STABILITY.md`](https://github.com/BlockForge-Dev/postgresflow/blob/main/STABILITY.md)
//! for the full API stability policy.

pub mod attempts;
pub mod error_codes;
pub mod model;
pub mod policies;
pub mod repo;
pub mod retry;
pub mod runner;
pub mod enqueue_guard;
pub mod ingest_decisions;
pub mod policy_decisions;

pub use policies::{PoliciesRepo, QueuePolicy};
pub use policy_decisions::{PolicyDecisionRow, PolicyDecisionsRepo};
pub use attempts::AttemptsRepo;
pub use model::{Job, JobStatus, NewJob};
pub use repo::JobsRepo;

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
