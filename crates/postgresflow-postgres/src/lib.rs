//! # PostgresFlow Postgres
//!
//! Production-grade PostgreSQL storage backend implementation for `postgresflow`.

pub use postgresflow_core::{
    CallRecord, Job, JobHandler, JobListItem, JobStatus, MemoryAttempt, MemoryBackend,
    MockBackend, NewJob, QueueError, StorageBackend,
};

// Re-export backend and database utilities from postgresflow
pub use postgresflow::backend::PostgresBackend;
pub use postgresflow::db::{make_pool, run_migrations};
pub use postgresflow::jobs::attempts::AttemptsRepo;
pub use postgresflow::jobs::ingest_decisions::IngestDecisionsRepo;
pub use postgresflow::jobs::maintenance::MaintenanceRepo;
pub use postgresflow::jobs::metrics::MetricsRepo;
pub use postgresflow::jobs::policies::{PoliciesRepo, QueuePolicy};
pub use postgresflow::jobs::policy_decisions::{PolicyDecisionRow, PolicyDecisionsRepo};
pub use postgresflow::jobs::repo::JobsRepo;
