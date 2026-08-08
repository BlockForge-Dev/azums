//! # Azums Axum
//!
//! Native Axum extractors (`JobQueue`) and state service integration (`BackgroundJobs`) for `azums`.

pub mod extractor;
pub mod service;

pub use extractor::JobQueue;
pub use service::BackgroundJobs;

pub use azums_core::{Job, JobListItem, JobStatus, NewJob, StorageBackend};
