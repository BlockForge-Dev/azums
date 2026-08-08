//! # Azums Core
//!
//! Zero-dependency core traits, models, and [`QueueError`] for `azums`.

#![deny(unsafe_code)]
#![allow(clippy::double_must_use)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod backend;
pub mod error;
pub mod model;

pub use backend::{
    CallRecord, MemoryAttempt, MemoryBackend, MockBackend, NotificationStream, StorageBackend,
    StreamBackend,
};
pub use error::{Error, QueueError};
pub use model::{
    ConsumerGroupStatus, Event, Job, JobHandler, JobListItem, JobProcessor, JobStatus, NewEvent,
    NewJob, QueueConfig, QueueOrdering,
};

/// Helper function to extract a human-readable panic message string from a panic payload.
pub fn format_panic_message(err: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = err.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else {
        "job handler panicked with unknown payload".to_string()
    }
}
