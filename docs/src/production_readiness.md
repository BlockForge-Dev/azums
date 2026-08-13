# Production Readiness

This page is the M19 production audit checkpoint. It states what was audited, what Azums provides,
what remains operator responsibility, and where claims are backed by implementation and tests.

## Security Audit

| Area | Production stance | Implementation | Tests / verification |
|---|---|---|---|
| Dependency audit | Run `cargo audit` or equivalent in CI before release. This local environment did not have `cargo-audit` installed when M19 was written. | [Cargo.toml](../../Cargo.toml), [Cargo.lock](../../Cargo.lock) | Operator command: `cargo audit`; local visibility: `cargo tree -d -p azums` |
| Unsafe code | `azums-core` denies unsafe code. M19 local grep found no `unsafe` token under `crates/`. | [lib.rs](../../crates/azums-core/src/lib.rs) | Operator command: `rg "\bunsafe\b" crates` |
| Serialization safety | Public JSON boundaries deserialize through `serde_json`; malformed input must reject cleanly. | [model.rs](../../crates/azums-core/src/model.rs) | [m13_fuzz_hardening.rs](../../crates/azums/tests/m13_fuzz_hardening.rs) |
| Secrets handling | `DATABASE_URL`, `REDIS_URL`, and `AZUMS_API_TOKEN` are environment inputs. Azums should not log raw connection strings or API tokens. | [config.rs](../../crates/azums/src/config.rs), [quickstart.rs](../../crates/azums/src/quickstart.rs) | Manual log review before production release |
| Authorization boundaries | Admin endpoints are protected when `AZUMS_API_TOKEN` is set. Health and the HTML shell remain unauthenticated. | [api/mod.rs](../../crates/azums-dashboard/src/api/mod.rs) | [admin_api.md](admin_api.md) |
| Payload limits | PostgreSQL admin/API enqueue path supports size and rate guards. Application code should also validate schemas before enqueue. | [enqueue_guard.rs](../../crates/azums/src/jobs/enqueue_guard.rs) | [storm_control.rs](../../crates/azums/tests/storm_control.rs), [m13_fuzz_hardening.rs](../../crates/azums/tests/m13_fuzz_hardening.rs) |
| Resource exhaustion | Query limits are clamped; stream reads and job lists are bounded; overload behavior is documented as backlog or backend throttling. | [repo.rs](../../crates/azums/src/jobs/repo.rs), [stream_handle.rs](../../crates/azums/src/stream_handle.rs), [backend.rs](../../crates/azums-redis/src/backend.rs) | [m8_concurrency_backpressure.rs](../../crates/azums/tests/m8_concurrency_backpressure.rs) |

Production requirement:

- Do not expose the admin API without `AZUMS_API_TOKEN` and network-level access control.
- Do not put secrets inside job payloads unless the backend storage, backups, logs, and DLQ access are treated as secret-bearing systems.
- Validate payload schemas at producer boundaries; Azums stores JSON but does not understand application-specific meaning.
- Run dependency advisory scanning in CI. M19 documents the command but does not vendor a scanner.

## Reliability Audit

| Area | Production stance | Implementation | Tests |
|---|---|---|---|
| Recovery | Abandoned running work becomes recoverable after lease expiry and reaping. | [quickstart.rs](../../crates/azums/src/quickstart.rs), [repo.rs](../../crates/azums/src/jobs/repo.rs), [memory.rs](../../crates/azums-core/src/backend/memory.rs) | [lease_recovery.rs](../../crates/azums/tests/lease_recovery.rs), [reliability_worker_crash.rs](../../crates/azums/tests/reliability_worker_crash.rs) |
| Graceful shutdown | `run_with_shutdown` accepts a `CancellationToken` and exits the poll loop. In-flight handler side effects remain application responsibility. | [quickstart.rs](../../crates/azums/src/quickstart.rs) | [phantom_recovery.rs](../../crates/azums/tests/phantom_recovery.rs) |
| Database failures | SQL transaction boundaries and lease recovery are tested. Live database restart and network partitions require environment-controlled testing. | [transactional_integrity.md](transactional_integrity.md), [chaos_engineering.md](chaos_engineering.md) | [transactional_enqueue.rs](../../crates/azums/tests/transactional_enqueue.rs), [chaos.rs](../../crates/azums/tests/chaos.rs) |
| Worker failures | Crash-after-claim and crash-before-ACK recover by lease expiry, producing at-least-once delivery. | [leasing.md](leasing.md), [semantics.md](semantics.md) | [lease_recovery.rs](../../crates/azums/tests/lease_recovery.rs), [reliability_worker_crash.rs](../../crates/azums/tests/reliability_worker_crash.rs) |
| Network failures | Redis/PostgreSQL network failures are backend and deployment dependent. Operators must test their runtime environment. | [backend_equivalence.md](backend_equivalence.md) | [redis_backend.rs](../../crates/azums/tests/redis_backend.rs), environment-dependent chaos profiles |

Production requirement:

- Set handler timeouts for jobs that call external services.
- Choose `AZUMS_LEASE_SECONDS` longer than normal handler latency but short enough for recovery objectives.
- Make handlers idempotent around external side effects.
- Alert on rising retry, timeout, lease-expired, and DLQ counts.

## Operations Audit

| Area | Production stance | Implementation | Tests / docs |
|---|---|---|---|
| Migrations | SQL migrations live in the `azums` crate and can run on startup or as a separate release step. | [migrations](../../crates/azums/migrations), [db.rs](../../crates/azums/src/db.rs), [quickstart.rs](../../crates/azums/src/quickstart.rs) | [transactional_enqueue.rs](../../crates/azums/tests/transactional_enqueue.rs), [sqlite.rs](../../crates/azums/tests/sqlite.rs) |
| Upgrade paths | Use semantic versioning and release-plz. Review migrations before deploy. | [RELEASE.md](../RELEASE.md) | Release checklist |
| Rollback | Roll back application image first. Database rollback requires an explicit migration-specific repair plan. | [RELEASE.md](../RELEASE.md) | Manual release procedure |
| Compatibility | Backend capabilities define supported semantics. | [backend_equivalence.md](backend_equivalence.md), [model.rs](../../crates/azums-core/src/model.rs) | [capabilities.rs](../../crates/azums/tests/capabilities.rs), [matrix_guard.rs](../../crates/azums/tests/matrix_guard.rs) |
| Configuration validation | Runtime config parses and clamps batch size, reap interval, maintenance interval, and SQLite vacuum settings. | [config.rs](../../crates/azums/src/config.rs) | Add deployment smoke tests for each environment |

## Release Gate

Before handing Azums to another team, run:

```powershell
cargo check --workspace
cargo test --workspace --no-run
cargo test -p azums --test m17_observability
cargo test -p azums --test chaos
mdbook build docs
```

When security tooling is installed, also run:

```powershell
cargo audit
cargo deny check
```

The audit result must explicitly classify any finding as:

- fixed before release
- accepted with mitigation and owner
- false positive with evidence
- environment-dependent and covered by deployment tests

No production release should ship with an unexplained security, migration, or data-loss risk.
