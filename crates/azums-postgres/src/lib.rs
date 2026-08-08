//! # Azums Postgres
//!
//! Production-grade PostgreSQL storage backend implementation for `azums`.

pub use azums_core::{
    CallRecord, Job, JobHandler, JobListItem, JobStatus, MemoryAttempt, MemoryBackend, MockBackend,
    NewJob, QueueError, StorageBackend,
};

// Re-export backend and database utilities from azums
pub use azums::backend::PostgresBackend;
pub use azums::db::{make_pool, run_migrations};
pub use azums::jobs::attempts::AttemptsRepo;
pub use azums::jobs::ingest_decisions::IngestDecisionsRepo;
pub use azums::jobs::maintenance::MaintenanceRepo;
pub use azums::jobs::metrics::MetricsRepo;
pub use azums::jobs::policies::{PoliciesRepo, QueuePolicy};
pub use azums::jobs::policy_decisions::{PolicyDecisionRow, PolicyDecisionsRepo};
pub use azums::jobs::repo::JobsRepo;
