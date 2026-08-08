//! # Azums Core
//!
//! Zero-dependency core traits, models, and [`QueueError`] for `azums`.

#![deny(unsafe_code)]
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
    NewJob,
};
