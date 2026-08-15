# API Stability Policy

Azums follows [Semantic Versioning](https://semver.org/). Starting with version 1.0.0, all public
APIs follow semver unless they are explicitly listed as unstable below.

1.0 is allowed only after the [M21 Stable Release Gate](docs/src/stable_release.md) passes for the
exact release commit. Azums treats 1.0 as a stable-semantics declaration, not as a feature-count
milestone.

## Stable APIs

All public types, traits, functions, methods, and documented Guaranteed semantics are stable unless
their module appears in the Unstable APIs section. Breaking changes require a major version bump,
with a deprecation period where feasible.

| Item | Module |
|------|--------|
| `Config` | `azums::config` |
| `Config::from_env()` | `azums::config` |
| `make_pool()` | `azums::db` |
| `run_migrations()` | `azums::db` |
| `Job` | `azums::jobs::model` |
| `NewJob` | `azums::jobs::model` |
| `JobStatus` | `azums::jobs::model` |
| `JobsRepo` | `azums::jobs::repo` |
| `JobsRepo::enqueue()` | `azums::jobs::repo` |
| `JobsRepo::enqueue_now()` | `azums::jobs::repo` |
| `JobsRepo::enqueue_at()` | `azums::jobs::repo` |
| `JobsRepo::enqueue_in()` | `azums::jobs::repo` |
| `JobsRepo::get_job()` | `azums::jobs::repo` |
| `JobsRepo::lease_jobs_batch()` | `azums::jobs::repo` |
| `AttemptsRepo` | `azums::jobs::attempts` |
| `JobRunner` | `azums::jobs::retry` |
| `RetryConfig` | `azums::jobs::retry` |
| `EnqueueGuard` | `azums::jobs::enqueue_guard` |
| `EnqueueGuardConfig` | `azums::jobs::enqueue_guard` |
| `PoliciesRepo` | `azums::jobs::policies` |
| `QueuePolicy` | `azums::jobs::policies` |
| `PolicyDecisionsRepo` | `azums::jobs::policy_decisions` |
| `IngestDecisionsRepo` | `azums::jobs::ingest_decisions` |
| `StorageBackend` and `StreamBackend` | `azums_core::backend` |
| `BackendCapabilities` and `BackendSemanticCapabilities` | `azums_core::model` |
| `SemanticBehavior`, `SemanticClassification`, `SemanticContract` | `azums_core::semantics` |
| `semantic_contract()` | `azums_core::semantics` |
| `QuickstartFlow` / `Client` | `azums::quickstart` |
| `MemoryBackend`, `PostgresBackend`, `SqliteBackend`, `RedisBackend` | backend modules |

## Unstable APIs

These modules are marked with `Unstable API` in their doc comments. Their interfaces may change in
any minor version before 1.0 without a deprecation period.

| Module | Reason |
|--------|--------|
| `azums::jobs::timeline` | Internal timeline format under active iteration |
| `azums::jobs::debug_view` | Debug tooling, format may change |
| `azums-dashboard` | Web dashboard UI and admin endpoints package |

## How to Identify Unstable APIs

1. **Doc comments:** Unstable modules carry an `Unstable API` warning in their documentation.
2. **This document:** The tables above are the canonical reference.

## Reporting Breakage

If a stable API changes without a version bump, please file an issue. We treat this as a bug.
