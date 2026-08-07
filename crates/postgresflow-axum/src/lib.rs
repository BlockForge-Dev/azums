//! # PostgresFlow Axum
//!
//! Native Axum extractors (`JobQueue`) and state service integration (`BackgroundJobs`) for `postgresflow`.

pub mod extractor;
pub mod service;

pub use extractor::JobQueue;
pub use service::BackgroundJobs;

pub use postgresflow_core::{Job, JobListItem, JobStatus, NewJob, StorageBackend};
