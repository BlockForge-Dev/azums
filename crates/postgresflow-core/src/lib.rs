//! # PostgresFlow Core
//!
//! Core data types and the backend-agnostic [`StorageBackend`] trait for `postgresflow`.

pub mod backend;
pub mod model;

pub use backend::StorageBackend;
pub use model::{Job, JobListItem, JobStatus, NewJob};
