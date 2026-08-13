# M20 Release Candidate Evidence

M20 freezes features and proves documented guarantees against the available test and audit gates.

Result: no known violation of a documented Azums guarantee was found.

RC caveats:

- `cargo audit` passes with a scoped ignore for `RUSTSEC-2023-0071`: the vulnerable `rsa` crate is
  present only through SQLx's optional MySQL backend in `Cargo.lock`; Azums enables PostgreSQL and
  SQLite SQLx features, and `cargo tree --workspace --all-features --target all` shows no active
  path to `rsa` or `sqlx-mysql`.
- Redis Criterion benchmarks are opt-in behind `AZUMS_BENCH_REDIS=1` and require `REDIS_URL`. The
  default full Criterion sweep completes without trying to benchmark a local Redis instance.
- `cargo semver-checks --workspace --baseline-rev HEAD~1` exceeded the command budget. The primary
  public crate check, `cargo semver-checks -p azums --baseline-rev HEAD~1`, passed.

## RC Fixes Found By The Proof Run

The first RC pass found two API compatibility issues in examples/docs:

- `crates/azums/examples/full_features.rs` constructed `NewJob` directly with stale fields.
- `crates/azums/src/jobs/repo.rs` had a doctest constructing `NewJob` directly with stale fields.

Both were changed to use the stable `Job` builder path.

The blocker cleanup pass also:

- upgraded SQLx from `0.7` to `0.8.6`
- made the Redis throughput benchmark opt-in by default
- removed timing assertions from Criterion benchmark loops
- installed and ran `cargo-audit`
- installed and ran `cargo-semver-checks` for the primary `azums` crate

## Command Evidence

| Gate | Command | Result |
|---|---|---|
| Full test suite | `cargo test --workspace` | PASS |
| Full integration suite | Included in `cargo test --workspace`; PostgreSQL/Redis tests ran where services were reachable | PASS |
| Full chaos suite | `cargo test -p azums --test chaos` plus `AZUMS_CHAOS_SCENARIOS=10000 cargo test -p azums --test chaos m11_memory_randomized_chaos_10000_plus -- --ignored` | PASS |
| Full fuzz suite | `cargo test -p azums --test m13_fuzz_hardening` | PASS |
| Full property suite | `cargo test -p azums --test m12_property_based`; `cargo test -p azums-core --test proptest_queue` | PASS |
| Full benchmark suite | `cargo bench -p azums --benches` | PASS; Redis benchmark skipped unless `AZUMS_BENCH_REDIS=1` |
| Reproducible benchmark harness | `cargo test -p azums --test m14_performance_harness` | PASS |
| Performance regression guard | `cargo test -p azums --test m15_performance_regression_guard` | PASS |
| Documentation build | `mdbook build docs` | PASS with pre-existing benchmark HTML warnings |
| Dependency audit | `cargo audit` | PASS with documented inactive optional-dependency ignore |
| API compatibility checks | `cargo test -p azums --test api_audit`; `cargo test -p azums --test matrix_guard`; doctests in `cargo test --workspace`; `cargo semver-checks -p azums --baseline-rev HEAD~1` | PASS |
| Future incompatibility check | `cargo check --workspace` after SQLx 0.8.6 upgrade | PASS; old `sqlx-postgres 0.7.4` warning removed from current build |

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
| Performance claims are reproducible and regression-guarded. | `azums/tests/m14_performance_harness.rs`; `azums/tests/m15_performance_regression_guard.rs`; `cargo bench -p azums --benches` | PASS |

## Follow-Up Items

These are not release blockers:

- Run Redis-specific Criterion benchmarks explicitly with `AZUMS_BENCH_REDIS=1` and `REDIS_URL` in
  infrastructure that owns the Redis service and timing budget.
- Run `cargo semver-checks --workspace --baseline-rev HEAD~1` in CI with a longer wall-clock budget
  if every workspace support crate needs automated semver checks.
- Periodically revisit `.cargo/audit.toml` and remove the `RUSTSEC-2023-0071` ignore if SQLx stops
  locking the optional MySQL `rsa` dependency or Azums adds MySQL support.

Release candidate evidence says:

> No known violation of a documented guarantee.
