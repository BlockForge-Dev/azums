# M21 Stable Release Gate

M21 answers one question:

> Can Azums tell Rust developers that its documented semantics will remain stable?

Azums 1.0 is not a feature-count release. It is a guarantee-stability release.

Current status: **1.0 is not declared by this document.** A 1.0 release is eligible only when every
gate below passes for the exact release commit.

## Stable Semantics

These contracts are eligible to become 1.0-stable because they are documented in
[Execution Semantics](semantics.md), backed by M20 evidence, and expressed through stable public API
surfaces:

| Area | Stable 1.0 contract |
|---|---|
| Delivery | Azums provides at-least-once job execution for successfully enqueued runnable jobs. |
| External effects | Azums does not guarantee exactly-once external side effects. |
| State machine | Invalid lifecycle transitions are rejected and terminal states remain terminal. |
| Attempts | Attempt history is durable while retained by a durable backend. |
| Leasing | A job has at most one active lease; expired leases can be recovered. |
| Retry and DLQ | Classified failures follow deterministic retry, cancellation, timeout, panic, and DLQ rules. |
| Idempotent enqueue | A non-null `idempotency_key` identifies one logical job where the backend supports the documented API. |
| Transactional enqueue | SQL transactional enqueue is guaranteed only inside supported backend transaction boundaries. |
| Scheduling | Jobs do not intentionally execute before documented eligibility time. Exact wall-clock execution time is not guaranteed. |
| Ordering | Priority and FIFO affect lease order where documented. Completion order and worker fairness are not guaranteed. |
| Backpressure | Overload behavior is explicit: backlog-only or backend-declared execution rate limiting. |
| Streams | Stream append, offset reads, monotonic ACK, consumer-group offsets, and replay are at-least-once contracts. |
| Replay | Replay creates new work with lineage; it does not erase history or deduplicate external side effects. |
| Cancellation | Queued/scheduled cancellation and owning-worker running cancellation follow the documented state machine. |
| Backend equivalence | One application job API spans Memory, SQLite, PostgreSQL, and Redis without pretending their operational guarantees are identical. |

## Stable API Surface

For 1.0, the stable public API list in [API Stability Policy](../../STABILITY.md) becomes a semver
major-version contract.

Before 1.0:

- breaking changes to stable APIs require the documented pre-1.0 policy
- unstable modules may still change in minor versions
- every unstable module must remain labeled as unstable

At and after 1.0:

- breaking changes to stable APIs require a major version bump
- behavior classified as **Guaranteed** in [Execution Semantics](semantics.md) must not regress in a
  compatible release
- behavior classified as **Backend-dependent** must stay tied to explicit `BackendCapabilities`
- behavior classified as **Unspecified** must not be marketed as guaranteed

## Backend Boundary

Azums 1.0 may guarantee a portable API without guaranteeing identical storage behavior.

The compatibility matrix in [Storage Backend Equivalence](backend_equivalence.md) is part of the
release contract. A backend may ship only if its declared capabilities match tested behavior:

- Memory remains process-local and non-durable.
- SQLite remains durable for file-backed embedded use, with single-process coordination semantics.
- PostgreSQL remains the strongest transactional and distributed-worker backend.
- Redis remains atomic inside Redis, distributed-worker capable, and non-transactional with a
  separate SQL application database.

Any backend whose implementation diverges from its declared capabilities blocks 1.0.

## Required Release Gates

The exact release commit must pass:

| Gate | Required command or evidence |
|---|---|
| Release candidate evidence | [M20 Release Candidate Evidence](release_candidate.md) says no known documented guarantee is violated. |
| Unit and integration tests | `cargo test --workspace` |
| Documentation build | `mdbook build docs` |
| Dependency audit | `cargo audit` with only documented non-blocking warnings or scoped inactive-dependency ignores |
| Public API compatibility | `cargo semver-checks -p azums --baseline-rev <previous-release>` |
| Future incompatibility check | `cargo check --workspace` without active future-incompat warnings from Azums-owned code |
| Guarantee inventory | Every Guaranteed, Backend-dependent, and Unspecified behavior remains classified in [Execution Semantics](semantics.md). |
| Backend matrix | Memory, SQLite, PostgreSQL, and Redis capabilities remain documented and tested. |
| Operations docs | Production deployment and failure/recovery runbooks build with the book. |

Optional-but-recommended gates before announcing a production 1.0:

- Redis-specific Criterion sweep with `AZUMS_BENCH_REDIS=1` and `REDIS_URL`
- full workspace semver checks with a CI budget long enough for support crates
- long chaos profile in infrastructure that controls PostgreSQL, Redis, process death, and network faults

## Release Blockers

Any of these blocks 1.0:

- undocumented behavior being relied on by public examples
- a documented guarantee without implementation and test evidence
- a backend capability that overstates the backend's real behavior
- an active vulnerability in a shipped dependency path
- an active future-incompat warning from Azums-owned code
- an unclassified behavior in scheduling, DLQ, idempotency, transactional enqueue, streams,
  consumer groups, replay, cancellation, or backend compatibility
- a stable API break that is not handled according to the stability policy
- release notes that imply exactly-once external side effects, strict completion ordering, global
  ordering, unlimited retention, automatic scaling, or cross-backend transactionality

## Stable Release Declaration

A release manager may declare Azums 1.0 only by recording the release commit, previous-release
baseline, command evidence, and remaining documented non-blocking caveats.

The declaration must use this form:

```text
Azums 1.0 declares the documented stable API and Guaranteed execution semantics as stable.
Backend-dependent behavior remains governed by BackendCapabilities.
Unspecified behavior remains outside the compatibility contract.
```

Until that declaration exists for a release commit, Azums may be release-candidate ready, but it is
not a stable 1.0 release.
