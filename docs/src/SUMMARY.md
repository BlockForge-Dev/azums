# Summary

[Introduction](introduction.md)
[Zero-Config Quickstart](quickstart.md)
[The Azums Architecture Book](architecture_book.md)

# Part I - Philosophy

- [Architecture Overview](architecture.md)
- [Execution Semantics](semantics.md)
- [Primitive Correctness Audit](primitive_correctness.md)

# Part II - Execution Model

- [Job Lifecycle](job_lifecycle.md)
- [Scheduling & Time Semantics](time_semantics.md)
- [Replay Semantics](replay_semantics.md)

# Part III - Core Primitives

- [Storage Backend Equivalence](backend_equivalence.md)
- [Transactional Integrity](transactional_integrity.md)
- [Idempotency & Duplicate Execution](idempotency.md)

# Part IV - Reliability

- [Lease Recovery](leasing.md)
- [Chaos Engineering](chaos_engineering.md)
- [Property-Based Testing](property_testing.md)
- [Fuzzing & Input Hardening](fuzzing_input_hardening.md)

# Part V - Storage Backends

- [Redis Storage Backend](redis_backend.md)
- [Dataset Partitioning Strategy](partitioning.md)

# Part VI - Coordination

- [Concurrency, Ordering & Backpressure](concurrency_backpressure.md)
- [Ordering Guarantees](ordering.md)
- [Event-Driven Instant Wake-Up](instant_wakeup.md)

# Part VII - Event Streaming

- [Redis-Style Event Streams](streams.md)
- [Durable Event Streaming](event_streaming.md)

# Part VIII - Performance

- [Performance Engineering](performance_engineering.md)
- [Performance Regression Protection](performance_regression_protection.md)
- [Performance Tuning Guide](../PERFORMANCE_TUNING.md)
- [Interactive Benchmark Dashboard](../benchmarks/index.html)

# Part IX - Failure Engineering

- [Retry, Failure Classification & DLQ](failure_handling.md)

# Part X - Integrations

- [Developer Experience & Integration](developer_experience.md)
- [Admin API & Web UI](admin_api.md)

# Part XI - Operations

- [Production Readiness](production_readiness.md)
- [Production Deployment Guide](production_deployment.md)
- [Failure And Recovery Runbook](failure_recovery_runbook.md)
- [Observability](observability.md)
- [Feature Comparison Matrix](comparison.md)

# Part XII - Internals

- [Low-Level Design & DSA](LLD.md)
