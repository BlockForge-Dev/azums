# Azums Test Matrix (must stay green)

This file proves the test suite covers the product’s invariants.

---

## Constitution (5 Laws)

### Law 1 — Every failure has a recorded reason code

Covered by:

- tests/attempts.rs::finish_failed_sets_error_fields
- tests/dlq.rs::non_retryable_goes_to_dlq_immediately

### Law 2 — Every job is replayable

Covered by:

- tests/replay.rs

### Law 3 — Retries are bounded and budgeted

Covered by:

- tests/retries.rs
- tests/dlq.rs::exhausted_retries_moves_job_to_dlq_and_preserves_attempts
- tests/storm_control.rs
- tests/policy_decisions.rs

### Law 4 — System protects itself from abuse

Covered by:

- tests/storm_control.rs
- tests/policy_decisions.rs

### Law 5 — Debug without logs

Covered by:

- tests/timeline.rs
- tests/policy_timeline.rs
- tests/error_classification.rs

---

## Reliability (chaos-ish)

Covered by:

- tests/chaos.rs::m11_memory_randomized_chaos_ci_matrix
- tests/chaos.rs::m11_sqlite_contention_chaos_ci_matrix
- tests/chaos.rs::m11_memory_randomized_chaos_10000_plus (ignored long-run profile)
- tests/leasing.rs::lease_expires_then_other_worker_can_claim
- tests/leasing.rs::leasing_two_workers_never_claim_same_job
- tests/reliability_worker_crash.rs

Notes:

- The default chaos test suite covers randomized in-process failures and SQLite contention.
- Live PostgreSQL restart, Redis restart, and network partition tests are environment-dependent and must not be reported as guaranteed unless the runner controls those services.

---

## Property-Based Testing

Covered by:

- tests/m12_property_based.rs::m12_generated_lifecycle_programs_preserve_core_invariants
- tests/m12_property_based.rs::m12_generated_lifecycle_state_transitions_are_exact
- tests/m12_property_based.rs::m12_sqlite_generated_rollbacks_leave_no_durable_job

Properties:

- No illegal state transition.
- No duplicate valid lease.
- Attempts never decrease.
- Terminal jobs remain terminal.
- Rollback produces no durable job.
- Duplicate enqueue operations with an idempotency key produce one logical job.

---

## Fuzzing & Input Hardening

Covered by:

- tests/m13_fuzz_hardening.rs::m13_public_input_boundaries_survive_generated_garbage
- tests/m13_fuzz_hardening.rs::m13_malformed_serialized_data_rejects_without_panic

Boundaries:

- Job payloads, job types, queues, idempotency metadata, streams, event types, malformed serialized jobs/events, status parsing, typed payload decoding, lease APIs, stream APIs.

Properties:

- No panic.
- No unbounded allocation from generated input.
- No infinite loop.
- Committed jobs remain readable.
- Storage never produces unknown job statuses.
- Lease batches do not contain duplicate jobs.
- Terminal jobs do not retain live leases.
- Malformed serialized data rejects cleanly.

---

## Load & Cost

Status:

- Reproducible benchmark harness is available as `azums-perf`.
- Criterion microbenchmarks remain available under `crates/azums/benches`.

Artifacts:

- `cargo run -p azums --release --bin azums-perf`
- `cargo bench -p azums --benches`
- `target/azums-perf/m14_report.json`
- `target/azums-perf/m14_report.md`
- tests/m14_performance_harness.rs::m14_perf_binary_emits_reproducible_reports
- tests/m15_performance_regression_guard.rs::m15_perf_guard_passes_matching_reports_and_fails_meaningful_regressions

Regression guard:

- `azums-perf-guard <baseline m14_report.json> <current m14_report.json>`
- Fails on >5% throughput regression by default.
- Fails on >5% p50/p99 latency regression by default.
- Fails on >10% allocation or memory increase when those counters are measured.
- Emits explicit `PERF_GUARD_SKIP` lines for nullable/unmeasured CPU, allocation, or memory counters.

---

## Developer Experience

Covered by:

- tests/m16_developer_experience.rs::m16_install_enqueue_process_retry_inspect_path_is_one_client
- crates/azums/examples/install_enqueue_process_retry_inspect.rs

Properties:

- New users can install, enqueue, process, retry, inspect, replay, and use stream consumer groups through one client.
- Advanced users can progressively access backend capabilities and storage-specific APIs without rewriting business handlers.
