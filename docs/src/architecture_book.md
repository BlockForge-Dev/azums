# The Azums Architecture Book

This book explains Azums as a system: what it guarantees, where guarantees are backend-dependent,
how jobs move, how workers coordinate, how failures are made inspectable, and how the code is
organized.

Beginners should start with [Zero-Config Quickstart](quickstart.md). The architecture book is for
engineers who need to reason about correctness, operate Azums in production, write a backend adapter,
or audit reliability claims.

## Reading Map

Part I, Philosophy, defines the product promise: one application API over multiple storage
environments without pretending those storage environments are identical.

Part II, Execution Model, defines the canonical job state machine and what Azums does not guarantee.

Part III, Core Primitives, covers the indivisible objects: job, attempt, execution, worker, queue,
event, payload, metadata, priority, scheduling, and identity.

Part IV, Reliability, covers transactional enqueue, lease recovery, retries, DLQ, idempotency, and
duplicate delivery.

Part V, Storage Backends, documents the capability matrix for Memory, SQLite, PostgreSQL, and Redis.

Part VI, Coordination, covers concurrency, ordering, queue isolation, partitioning, and backpressure.

Part VII, Event Streaming, treats streams as a first-class primitive with offsets, consumer groups,
acknowledgement, and replay.

Part VIII, Performance, explains how benchmark claims are produced and guarded against regression.

Part IX, Failure Engineering, covers chaos tests, property tests, fuzzing, and input hardening.

Part X, Integrations, shows how to adopt Azums from plain Rust, web frameworks, CLI tools, Tokio, and
embedded SQLite applications.

Part XI, Operations, covers observability, metrics, structured logs, tracing propagation, and admin
inspection.

Part XII, Internals, is the source map for engineers extending Azums itself.

## System Contract

Azums provides at-least-once execution. It does not guarantee exactly-once external side effects.

Azums makes every committed job observable while retained by the backend. It does not guarantee
visibility for job history that an operator has explicitly pruned.

Azums exposes one portable application API. It does not claim that Memory, SQLite, PostgreSQL, and
Redis have identical durability or coordination properties.

Azums can deduplicate enqueue attempts by idempotency key. It does not deduplicate arbitrary
side-effect calls inside application handlers.

Azums schedules by documented eligibility time. It does not guarantee execution at exactly that
timestamp; workers, clocks, queue ordering, and backpressure affect actual start time.

## Evidence Ledger

Every important architecture claim below points to implementation and tests. If a row has
backend-dependent behavior, the linked docs define the boundary.

| Claim | Implementation | Tests |
|---|---|---|
| The portable storage boundary is `StorageBackend`. | [backend/mod.rs](../../crates/azums-core/src/backend/mod.rs) | [api_audit.rs](../../crates/azums/tests/api_audit.rs), [matrix_guard.rs](../../crates/azums/tests/matrix_guard.rs) |
| Backends declare capabilities instead of faking equivalence. | [model.rs](../../crates/azums-core/src/model.rs), [memory.rs](../../crates/azums-core/src/backend/memory.rs), [postgres.rs](../../crates/azums/src/backend/postgres.rs), [sqlite.rs](../../crates/azums/src/backend/sqlite.rs), [backend.rs](../../crates/azums-redis/src/backend.rs) | [capabilities.rs](../../crates/azums/tests/capabilities.rs), [matrix_guard.rs](../../crates/azums/tests/matrix_guard.rs) |
| Job lifecycle states have legal successors and terminal states. | [model.rs](../../crates/azums-core/src/model.rs) | [core_unit.rs](../../crates/azums-core/tests/core_unit.rs), [m12_property_based.rs](../../crates/azums/tests/m12_property_based.rs) |
| Workers must lease jobs before execution. | [quickstart.rs](../../crates/azums/src/quickstart.rs), [memory.rs](../../crates/azums-core/src/backend/memory.rs), [repo.rs](../../crates/azums/src/jobs/repo.rs) | [leasing.rs](../../crates/azums/tests/leasing.rs), [concurrency.rs](../../crates/azums/tests/concurrency.rs) |
| Attempts are durable and reconstructable. | [attempts.rs](../../crates/azums/src/jobs/attempts.rs), [memory.rs](../../crates/azums-core/src/backend/memory.rs), [timeline.rs](../../crates/azums/src/jobs/timeline.rs) | [attempts.rs](../../crates/azums/tests/attempts.rs), [timeline.rs](../../crates/azums/tests/timeline.rs), [m17_observability.rs](../../crates/azums/tests/m17_observability.rs) |
| Retry and DLQ are deterministic for classified failures. | [retry.rs](../../crates/azums/src/jobs/retry.rs), [quickstart.rs](../../crates/azums/src/quickstart.rs) | [retries.rs](../../crates/azums/tests/retries.rs), [dlq.rs](../../crates/azums/tests/dlq.rs), [failure_semantics.rs](../../crates/azums/tests/failure_semantics.rs) |
| Idempotency keys collapse duplicate enqueue attempts into one logical job where supported. | [model.rs](../../crates/azums-core/src/model.rs), [memory.rs](../../crates/azums-core/src/backend/memory.rs), [repo.rs](../../crates/azums/src/jobs/repo.rs) | [idempotency.rs](../../crates/azums/tests/idempotency.rs), [m12_property_based.rs](../../crates/azums/tests/m12_property_based.rs) |
| Transactional enqueue is supported by transactional backends and documented as backend-dependent. | [backend_equivalence.md](backend_equivalence.md), [transactional_integrity.md](transactional_integrity.md), [repo.rs](../../crates/azums/src/jobs/repo.rs) | [transactional_enqueue.rs](../../crates/azums/tests/transactional_enqueue.rs), [m12_property_based.rs](../../crates/azums/tests/m12_property_based.rs) |
| Lease expiry makes abandoned work recoverable. | [memory.rs](../../crates/azums-core/src/backend/memory.rs), [repo.rs](../../crates/azums/src/jobs/repo.rs), [quickstart.rs](../../crates/azums/src/quickstart.rs) | [lease_recovery.rs](../../crates/azums/tests/lease_recovery.rs), [reliability_worker_crash.rs](../../crates/azums/tests/reliability_worker_crash.rs), [phantom_recovery.rs](../../crates/azums/tests/phantom_recovery.rs) |
| Scheduling never makes a job runnable before its eligibility time. | [model.rs](../../crates/azums-core/src/model.rs), [quickstart.rs](../../crates/azums/src/quickstart.rs) | [scheduling.rs](../../crates/azums/tests/scheduling.rs), [m9_time_semantics.rs](../../crates/azums/tests/m9_time_semantics.rs) |
| Concurrency behavior is explicit under workers, queues, ordering, and overload. | [model.rs](../../crates/azums-core/src/model.rs), [quickstart.rs](../../crates/azums/src/quickstart.rs), [enqueue_guard.rs](../../crates/azums/src/jobs/enqueue_guard.rs) | [m8_concurrency_backpressure.rs](../../crates/azums/tests/m8_concurrency_backpressure.rs), [fifo_ordering.rs](../../crates/azums/tests/fifo_ordering.rs), [storm_control.rs](../../crates/azums/tests/storm_control.rs) |
| Event streams have offsets, consumer groups, acknowledgement, and replay. | [stream.rs](../../crates/azums-core/src/backend/stream.rs), [stream_handle.rs](../../crates/azums/src/stream_handle.rs), [stream_repo.rs](../../crates/azums/src/jobs/stream_repo.rs) | [streams.rs](../../crates/azums/tests/streams.rs), [m10_streaming.rs](../../crates/azums/tests/m10_streaming.rs) |
| Chaos, property, and fuzz testing protect invariants beyond hand-picked examples. | [tests/chaos](../../crates/azums/tests/chaos), [m12_property_based.rs](../../crates/azums/tests/m12_property_based.rs), [m13_fuzz_hardening.rs](../../crates/azums/tests/m13_fuzz_hardening.rs) | [chaos.rs](../../crates/azums/tests/chaos.rs), [m12_property_based.rs](../../crates/azums/tests/m12_property_based.rs), [m13_fuzz_hardening.rs](../../crates/azums/tests/m13_fuzz_hardening.rs) |
| Performance claims are reproducible and guarded. | [azums-perf.rs](../../crates/azums/src/bin/azums-perf.rs), [azums-perf-guard.rs](../../crates/azums/src/bin/azums-perf-guard.rs) | [m14_performance_harness.rs](../../crates/azums/tests/m14_performance_harness.rs), [m15_performance_regression_guard.rs](../../crates/azums/tests/m15_performance_regression_guard.rs) |
| The beginner path is install, enqueue, process, retry, inspect. | [quickstart.rs](../../crates/azums/src/quickstart.rs), [install_enqueue_process_retry_inspect.rs](../../crates/azums/examples/install_enqueue_process_retry_inspect.rs) | [m16_developer_experience.rs](../../crates/azums/tests/m16_developer_experience.rs) |
| Production failures are explainable through structured observations and metrics. | [observability.rs](../../crates/azums-core/src/backend/observability.rs), [quickstart.rs](../../crates/azums/src/quickstart.rs) | [m17_observability.rs](../../crates/azums/tests/m17_observability.rs) |

## How To Read The Source After The Book

Start with [azums-core/src/model.rs](../../crates/azums-core/src/model.rs). That file contains the
data types and lifecycle rules that every backend must respect.

Then read [azums-core/src/backend/mod.rs](../../crates/azums-core/src/backend/mod.rs). That trait is
the adapter contract.

Then read [azums/src/quickstart.rs](../../crates/azums/src/quickstart.rs). That file shows how the
portable backend contract becomes an application-facing worker runtime.

Only after that should you inspect individual backends. Backend files are allowed to differ because
their storage engines differ. The compatibility matrix documents which differences are guarantees,
which are backend-dependent, and which are unspecified.
