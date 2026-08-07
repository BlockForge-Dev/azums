# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-08-07

### Added
- **Multi-Crate Workspace Architecture**:
  - `postgresflow-core`: Zero-dependency, `no_std` + `alloc` compatible core contract.
  - `postgresflow-postgres`: Dedicated PostgreSQL storage backend driver using SQLx & Tokio.
  - `postgresflow`: Meta-crate & `pgflowctl` administration binary.
- **Storage Backends**:
  - SQLite embedded storage backend (`SqliteBackend`).
  - In-Memory test backend (`MemoryBackend` & `MockBackend`).
  - Connection URL auto-detection in `quickstart(url)`.
- **Web Framework Integrations**:
  - `postgresflow-axum`: Native Axum 0.7 `JobQueue` extractor and `BackgroundJobs` state service.
  - `postgresflow-actix`: Native Actix Web 4 `JobQueue` extractor.
  - `postgresflow-poem`: Native Poem 3 `JobQueue` extractor.
  - `postgresflow-rocket`: Native Rocket 0.5 `JobQueue` request guard.
- **Ergonomics & API Polish**:
  - `Job::payload_typed<T>()` strongly-typed JSON payload deserialization.
  - `Client` top-level entry point alias.
  - `JobProcessor` trait for structured, trait-based worker registration.
  - Unified `Error` enum.
