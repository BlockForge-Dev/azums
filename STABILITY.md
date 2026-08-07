# API Stability Policy

PostgresFlow follows [Semantic Versioning](https://semver.org/). Until version 1.0, minor releases **may** contain breaking changes to **unstable** APIs. Stable APIs will follow semver strictly even before 1.0.

## Stable APIs

These types and functions are considered stable. Breaking changes to these will require a minor version bump (pre-1.0) or a major version bump (post-1.0), with a deprecation period where feasible.

| Item | Module |
|------|--------|
| `Config` | `postgresflow::config` |
| `Config::from_env()` | `postgresflow::config` |
| `make_pool()` | `postgresflow::db` |
| `run_migrations()` | `postgresflow::db` |
| `Job` | `postgresflow::jobs::model` |
| `NewJob` | `postgresflow::jobs::model` |
| `JobStatus` | `postgresflow::jobs::model` |
| `JobsRepo` | `postgresflow::jobs::repo` |
| `JobsRepo::enqueue()` | `postgresflow::jobs::repo` |
| `JobsRepo::enqueue_now()` | `postgresflow::jobs::repo` |
| `JobsRepo::enqueue_at()` | `postgresflow::jobs::repo` |
| `JobsRepo::enqueue_in()` | `postgresflow::jobs::repo` |
| `JobsRepo::get_job()` | `postgresflow::jobs::repo` |
| `JobsRepo::lease_jobs_batch()` | `postgresflow::jobs::repo` |
| `AttemptsRepo` | `postgresflow::jobs::attempts` |
| `JobRunner` | `postgresflow::jobs::runner` |
| `RetryConfig` | `postgresflow::jobs::retry` |
| `EnqueueGuard` | `postgresflow::jobs::enqueue_guard` |
| `EnqueueGuardConfig` | `postgresflow::jobs::enqueue_guard` |
| `PoliciesRepo` | `postgresflow::jobs::policies` |
| `QueuePolicy` | `postgresflow::jobs::policies` |
| `PolicyDecisionsRepo` | `postgresflow::jobs::policy_decisions` |
| `IngestDecisionsRepo` | `postgresflow::jobs::ingest_decisions` |

## Unstable APIs

These modules are marked with `⚠️ Unstable API` in their doc comments. Their interfaces may change in any minor version before 1.0 without a deprecation period.

| Module | Reason |
|--------|--------|
| `postgresflow::jobs::timeline` | Internal timeline format under active iteration |
| `postgresflow::jobs::debug_view` | Debug tooling, format may change |
| `postgresflow::jobs::metrics` | Metrics schema and aggregation strategy evolving |
| `postgresflow::jobs::maintenance` | Retention and archival strategy not finalized |
| `postgresflow::admin` (feature-gated) | Admin metrics endpoint format |
| `postgresflow::api` (feature-gated) | HTTP API router and handler signatures |

## How to Identify Unstable APIs

1. **Doc comments:** Unstable modules carry a `⚠️ Unstable API` warning in their documentation.
2. **Feature gates:** The `api` and `admin` modules require the `api` feature flag.
3. **This document:** The tables above are the canonical reference.

## Reporting Breakage

If a stable API changes without a version bump, please file an issue. We treat this as a bug.
