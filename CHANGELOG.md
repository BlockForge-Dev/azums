# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-08-07

### Added

- **Initial crates.io release** — `postgresflow` is now available as a library dependency.
- **Feature flags:** `api` (Axum-based admin HTTP API) and `cli` (`pgflowctl` binary), both enabled by default.
- **Public API re-exports** at crate root: `Config`, `JobsRepo`, `Job`, `NewJob`, `JobStatus`, `make_pool`, `run_migrations`, `JobRunner`, `RetryConfig`, `EnqueueGuard`, `EnqueueGuardConfig`, `AttemptsRepo`, `MaintenanceRepo`, `MetricsRepo`, `PoliciesRepo`, `QueuePolicy`, `PolicyDecisionsRepo`, `IngestDecisionsRepo`.
- **Dual MIT/Apache-2.0 licensing.**
- **API stability policy** (`STABILITY.md`) with stable/unstable module classification.
- **Automated release pipeline** via `release-plz` GitHub Actions workflow.
- **Crate-level documentation** with usage examples and feature flag table.

### Core Features (existing, now publicly documented)

- Transactional job leasing with `SELECT ... FOR UPDATE SKIP LOCKED`.
- Dead-letter queue (DLQ) with reason codes and queryable history.
- Exponential backoff retries with jitter and error classification.
- Batch dequeue/lease for high-throughput processing.
- Dataset partitioning on `jobs.dataset_id` for index health.
- Enqueue guardrails: payload size limits and rate limiting with audit trail.
- Queue-level storm control policies (max in-flight, max attempts/minute).
- Job replay with lineage tracking (`replay_of_job_id`).
- Admin HTTP API with metrics, timeline, explain, and DLQ endpoints.
- Prometheus-compatible metrics endpoint (`/metrics/prom`).
- Maintenance operations: archival and history pruning.
- `pgflowctl` CLI for database reset, seeding, and timeline inspection.

### Changed

- `axum` is now an optional dependency (enabled via the `api` feature).
- Worker crate (`crates/worker`) marked `publish = false`.
