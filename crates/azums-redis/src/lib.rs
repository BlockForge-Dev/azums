//! # Azums Redis
//!
//! Production-grade Redis storage and streaming backend implementation for `azums`.

pub mod backend;

pub use backend::RedisBackend;
