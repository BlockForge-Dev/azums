# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-08-08

### Added
- **Strict FIFO Ordering (Per-Queue)**:
  - Per-queue `QueueOrdering::Fifo` (default) vs `QueueOrdering::Fastest`.
  - Postgres `ORDER BY priority DESC, run_at ASC, created_at ASC, id ASC FOR UPDATE SKIP LOCKED` with composite index `jobs_fifo_queue_created_idx`.
  - SQLite `idx_jobs_fifo` index and `id ASC` tie-breaking.
  - Redis `LMOVE queue processing LEFT RIGHT` for true FIFO dequeuing.
- **Database Bloat & Maintenance Automation**:
  - `StorageBackend::perform_maintenance` and `client.perform_maintenance()`.
  - PostgreSQL automatic `VACUUM ANALYZE` on core tables every 5 minutes.
  - SQLite `PRAGMA auto_vacuum = INCREMENTAL;` and periodic `PRAGMA incremental_vacuum` after every N dequeues.
  - `/maintenance/status` REST endpoint displaying dead tuple counts (`n_dead_tup`) and last vacuum timestamps.
  - Partitioning strategy guide in `docs/src/partitioning.md`.
- **Phantom Job Recovery & Heartbeat**:
  - `StorageBackend::extend_lease` heartbeat method extending active long-running job locks.
  - Automatic in-flight job heartbeat tasks spawning every `lease_seconds / 2` seconds.
  - Background reclaimer sweeper (`reap_expired_locks`) returning stranded expired jobs back to `queued`.
  - `QuickstartFlow::run_with_shutdown` with `CancellationToken` for graceful worker shutdown.
- **Connection Pool Isolation**:
  - Unpooled `LISTEN` socket connections in PostgreSQL (`PgListener::connect`) and unpooled PubSub sockets in Redis (`client.get_async_pubsub()`), eliminating query pool starvation.
- **Panic Isolation & DLQ Handling**:
  - Panic-safe handler execution boundaries catching unwinding panics in worker tasks.
  - Automatic `format_panic_message` payload extraction.
  - Immediate routing of panicked jobs to Dead-Letter Queue (DLQ) with reason code `"PANIC"`, preserving worker runtime stability for subsequent jobs.

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
