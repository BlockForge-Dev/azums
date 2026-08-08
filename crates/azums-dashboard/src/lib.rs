//! # Azums Dashboard
//!
//! Optional web administration UI dashboard console for `azums`.

pub mod admin;
pub mod api;

pub use api::{router, ApiState};
