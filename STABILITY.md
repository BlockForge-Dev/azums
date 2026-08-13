# API Stability Policy

Azums follows [Semantic Versioning](https://semver.org/). Until version 1.0, minor releases **may**
contain breaking changes to **unstable** APIs. Stable APIs will follow semver strictly even before
1.0.

1.0 is allowed only after the [M21 Stable Release Gate](docs/src/stable_release.md) passes for the
exact release commit. Azums treats 1.0 as a stable-semantics declaration, not as a feature-count
milestone.

## Stable APIs

These types and functions are considered stable. Breaking changes to these will require a minor
version bump (pre-1.0) or a major version bump (post-1.0), with a deprecation period where feasible.

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

## Unstable APIs

These modules are marked with `Unstable API` in their doc comments. Their interfaces may change in
any minor version before 1.0 without a deprecation period.

| Module | Reason |
|--------|--------|
| `azums::jobs::timeline` | Internal timeline format under active iteration |
| `azums::jobs::debug_view` | Debug tooling, format may change |
| `azums::jobs::metrics` | Metrics schema and aggregation strategy evolving |
| `azums::jobs::maintenance` | Retention and archival strategy not finalized |
| `azums-dashboard` | Web dashboard UI and admin endpoints package |

## How to Identify Unstable APIs

1. **Doc comments:** Unstable modules carry an `Unstable API` warning in their documentation.
2. **This document:** The tables above are the canonical reference.

## Reporting Breakage

If a stable API changes without a version bump, please file an issue. We treat this as a bug.
