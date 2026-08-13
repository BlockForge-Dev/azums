# M20 Release Candidate Evidence

M20 freezes features and proves documented guarantees against the available test and audit gates.

Result: no known violation of a documented Azums guarantee was found in the gates that completed.

RC caveats:

- `cargo audit` is not installed in this environment, so dependency advisory scanning is blocked.
- `cargo semver-checks` is not installed in this environment, so automated semver compatibility
  scanning is blocked.
- Full Criterion benchmark sweep timed out after 20 minutes while running `redis_throughput`; the
  benchmark estimated a long Redis sample collection. The M14 reproducible benchmark smoke and M15
  performance regression guard both passed.
- Build output reports a future-incompatibility warning in `sqlx-postgres 0.7.4`. The warning is in
  an upstream dependency and should be resolved by upgrading SQLx before a Rust edition/toolchain
  change makes it hard error.

## RC Fixes Found By The Proof Run

The first RC pass found two API compatibility issues in examples/docs:

- `crates/azums/examples/full_features.rs` constructed `NewJob` directly with stale fields.
- `crates/azums/src/jobs/repo.rs` had a doctest constructing `NewJob` directly with stale fields.

Both were changed to use the stable `Job` builder path.

## Command Evidence

| Gate | Command | Result |
|---|---|---|
| Full test suite | `cargo test --workspace` | PASS |
| Full integration suite | Included in `cargo test --workspace`; PostgreSQL/Redis tests ran where services were reachable | PASS |
| Full chaos suite | `cargo test -p azums --test chaos` plus `AZUMS_CHAOS_SCENARIOS=10000 cargo test -p azums --test chaos m11_memory_randomized_chaos_10000_plus -- --ignored` | PASS |
| Full fuzz suite | `cargo test -p azums --test m13_fuzz_hardening` | PASS |
| Full property suite | `cargo test -p azums --test m12_property_based`; `cargo test -p azums-core --test proptest_queue` | PASS |
| Full benchmark suite | `cargo bench -p azums --benches` | BLOCKED: command timed out in `redis_throughput` |
| Reproducible benchmark harness | `cargo test -p azums --test m14_performance_harness` | PASS |
| Performance regression guard | `cargo test -p azums --test m15_performance_regression_guard` | PASS |
| Documentation build | `mdbook build docs` | PASS with pre-existing benchmark HTML warnings |
| Dependency audit | `cargo audit --version` / `cargo audit` | BLOCKED: `cargo-audit` not installed |
| API compatibility checks | `cargo test -p azums --test api_audit`; `cargo test -p azums --test matrix_guard`; doctests in `cargo test --workspace` | PASS |
| Future incompatibility report | `cargo report future-incompatibilities --id 1` | WARN: `sqlx-postgres 0.7.4` |

## Guarantee To Test Matrix

| Guarantee | Test evidence | RC status |
|---|---|---|
| Invalid lifecycle transitions are rejected. | `azums-core/tests/core_unit.rs`; `azums/tests/m12_property_based.rs` | PASS |
| Terminal states remain terminal. | `azums-core/tests/core_unit.rs`; `azums/tests/m12_property_based.rs`; `azums/tests/chaos.rs` | PASS |
| Every durable attempt can be reconstructed while retained. | `azums/tests/attempts.rs`; `azums/tests/timeline.rs`; `azums/tests/m17_observability.rs` | PASS |
| At-least-once execution and lease recovery preserve committed jobs. | `azums/tests/lease_recovery.rs`; `azums/tests/reliability_worker_crash.rs`; long M11 chaos profile | PASS |
| Transactional enqueue keeps SQL app state and job state aligned across commit/rollback boundaries. | `azums/tests/transactional_enqueue.rs`; `azums/tests/m12_property_based.rs` | PASS |
| Retry and DLQ lifecycle is deterministic for classified failures. | `azums/tests/retries.rs`; `azums/tests/dlq.rs`; `azums/tests/failure_semantics.rs` | PASS |
| Idempotency keys collapse duplicate enqueue attempts into one logical job where supported. | `azums/tests/idempotency.rs`; `azums/tests/m12_property_based.rs` | PASS |
| Scheduling does not lease before documented eligibility time. | `azums/tests/scheduling.rs`; `azums/tests/m9_time_semantics.rs` | PASS |
| Queue isolation, ordering, priority, and backpressure behavior remain predictable under concurrency. | `azums/tests/m8_concurrency_backpressure.rs`; `azums/tests/fifo_ordering.rs`; `azums/tests/storm_control.rs` | PASS |
| Event stream offsets, ACK, consumer groups, replay, and retention are unambiguous. | `azums/tests/streams.rs`; `azums/tests/m10_streaming.rs` | PASS |
| Public input boundaries reject malformed data without panics or invalid state. | `azums/tests/m13_fuzz_hardening.rs` | PASS |
| Production failures are explainable without reading source code. | `azums/tests/m17_observability.rs`; `azums/tests/m19_production_readiness.rs` | PASS |
| Beginner install, enqueue, process, retry, inspect path works. | `azums/tests/m16_developer_experience.rs`; `azums/examples/install_enqueue_process_retry_inspect.rs` built in workspace tests | PASS |
| Performance claims are reproducible and regression-guarded. | `azums/tests/m14_performance_harness.rs`; `azums/tests/m15_performance_regression_guard.rs` | PASS for smoke/regression guard; full Criterion sweep blocked |

## Not Passed As Release Gates Yet

These are not guarantee violations, but they prevent claiming a complete RC release gate:

- Install and run `cargo-audit`.
- Install and run `cargo-semver-checks`.
- Run the full Criterion benchmark suite with enough wall-clock budget or split Redis benchmarks into
  an explicit long-running profile.
- Decide whether to upgrade `sqlx` from `0.7.4` to a version without the recorded future
  incompatibility warning.

Until those are resolved, the release candidate evidence says:

> No known violation of a documented guarantee in completed gates; some release gates are blocked by
> missing tooling or benchmark runtime.
