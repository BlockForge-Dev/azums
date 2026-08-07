//! # PostgresFlow Core
//!
//! Zero-dependency core traits, models, and [`QueueError`] for `postgresflow`.

#![deny(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod backend;
pub mod error;
pub mod model;

pub use backend::{CallRecord, MemoryAttempt, MemoryBackend, MockBackend, StorageBackend};
pub use error::{Error, QueueError};
pub use model::{Job, JobHandler, JobListItem, JobProcessor, JobStatus, NewJob};
