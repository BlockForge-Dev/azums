# pgflow Test Matrix (must stay green)

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

## Load & Cost

Status:

- Not automated in cargo test yet.
- Planned: benches/ and a script that prints throughput/latency.

Planned artifacts:

- benches/load.rs (criterion or custom harness)
- scripts/test/load.ps1 (runs workers + pushes N jobs)
