---
title: "Azums 1.0: A Rust Job Queue That Makes Its Guarantees Explicit"
published: false
description: "Azums is a Rust-native job queue, worker runtime, scheduler, crash-recovery system, and event-streaming engine for Memory, SQLite, PostgreSQL, and Redis."
tags: rust, opensource, backend, database
---

Building a queue is easy.

Building one that can answer what happens after a worker crashes, a transaction rolls back, a lease expires, or the same job is delivered twice is much harder.

That is the problem we built [Azums](https://github.com/BlockForge-Dev/azums) to solve.

Azums is a Rust-native background job queue, worker runtime, scheduler, failure-recovery system, and durable event-streaming engine. It supports Memory, SQLite, PostgreSQL, and Redis through one application-facing API.

Version 1.0 is not simply a collection of features. It is a declaration that the documented execution semantics are stable.

The central rule is simple:

> Every important behavior must be classified as guaranteed, backend-dependent, or unspecified.

That distinction matters more than a large benchmark number or a long feature list.

## The problem Azums solves

Rust applications frequently need to perform work outside the request or command that created it:

- send an email after creating an account
- process a payment webhook
- generate a report
- retry an unreliable third-party API
- run a long AI inference task
- index a document after a database update
- schedule work for later
- replay events for a new consumer

Putting that work directly inside an HTTP request increases latency and couples user-facing availability to every downstream dependency. Spawning a Tokio task is lightweight, but the task disappears if the process exits. A broker can provide durability, but now the application has to reconcile broker behavior, database transactions, retries, idempotency, and worker recovery.

Azums provides one execution model for that entire path:

```text
application mutation
        |
        v
     enqueue
        |
        v
 claim -> lease -> attempt -> handler
                         |
          +--------------+--------------+
          |              |              |
          v              v              v
      completed      retry wait        DLQ
```

The goal is not to hide failure. The goal is to make failure deterministic, recoverable, and observable.

## Your first Azums job

Install the crate and runtime dependencies:

```bash
cargo add azums@1.0.0 anyhow serde_json
cargo add tokio --features macros,rt-multi-thread
```

Then register a handler, enqueue a job, and process it:

```rust
use azums::{quickstart, Job};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let queue = quickstart("memory").await?.with_queue("emails");

    queue
        .register_handler("send_welcome_email", |job| async move {
            println!("Sending email to {}", job.payload["email"]);
            Ok(())
        })
        .await;

    let job_id = queue
        .enqueue(
            Job::new(
                "send_welcome_email",
                json!({ "email": "new@example.com" }),
            )
            .queue("emails")
            .priority(10)
            .max_attempts(5)
            .idempotency_key("welcome:new@example.com"),
        )
        .await?;

    queue.run_until_empty().await?;
    println!("Job: {:?}", queue.get_job(job_id).await?);

    Ok(())
}
```

The handler does not need to know whether the job came from Memory, SQLite, PostgreSQL, or Redis. Changing storage is a connection-level decision:

```rust
let memory = quickstart("memory").await?;
let sqlite = quickstart("sqlite://jobs.db?mode=rwc").await?;
let postgres = quickstart("postgres://user:pass@localhost/app").await?;
let redis = quickstart("redis://127.0.0.1:6379").await?;
```

The business logic remains the same, but the operational guarantees do not. Azums exposes those differences rather than pretending the backends are equivalent.

## A state machine you can reason about

Every job follows one canonical lifecycle:

```text
SCHEDULED -> QUEUED -> RUNNING -> COMPLETED
                           |----> RETRY_WAIT -> QUEUED
                           |----> CANCELLED
                           |----> DLQ
```

Every transition outside this graph is illegal. `COMPLETED`, `CANCELLED`, and `DLQ` are terminal.

Azums separates the durable job from its execution history:

- `Job` stores identity, queue, type, payload, priority, schedule, retry budget, status, lease, DLQ information, replay lineage, and timestamps.
- `JobAttempt` records each handler run, including attempt number, worker, result, timing, and error information.
- `JobExecution` represents the active relationship between a job, attempt, worker, and lease.
- `Event` represents an immutable entry in a stream.
- `ConsumerGroupStatus` records the highest acknowledged stream offset.

This separation means a job's lifecycle can be reconstructed from persisted state instead of inferred from logs.

## Crash recovery is based on leases

Azums workers follow a claim, lease, heartbeat, and ACK protocol:

```text
CLAIM -> LEASE -> HEARTBEAT -> ACK
```

A worker must hold the active lease before executing or completing a job. While the handler runs, heartbeats extend that lease.

If the worker is killed, loses its database connection, or disappears before ACK:

```text
heartbeat stops
      |
      v
lease expires
      |
      v
recovery records the abandoned attempt
      |
      v
job becomes executable again
```

This is why Azums provides **at-least-once execution**.

It also explains an important non-guarantee: if the handler performs an external side effect and crashes before ACK, that side effect may happen twice. No queue can atomically control an arbitrary email provider, payment service, or HTTP API.

Azums therefore separates delivery safety from side-effect safety. Use an `idempotency_key` to collapse duplicate enqueue operations, and design external effects with their own idempotency key or transactional outbox boundary.

## Transactional enqueue where it is actually possible

SQLite and PostgreSQL can enqueue a job inside the same database transaction as application data:

```text
BEGIN
  update application state
  enqueue job
COMMIT
```

If the transaction commits, both changes are preserved. If it rolls back, neither is preserved.

This is one of Azums' strongest guarantees, but its scope is precise: the application mutation and job must use the same SQLite or PostgreSQL transaction.

Redis operations can be atomic inside Redis, but Redis cannot participate in an unrelated SQL transaction. Azums reports that limitation through `BackendCapabilities` instead of claiming cross-system atomicity.

## Deterministic retries and DLQ

Failures are classified as retryable, permanent, timeout, panic, cancellation, or system failures.

Retryable failures follow the configured attempt budget, delay, backoff, and jitter. A policy might produce delays such as:

```text
1s -> 2s -> 4s -> 8s -> 16s
```

When attempts are exhausted, or when a failure is classified as permanent, the job moves to the dead-letter queue. The retained record includes the original job identity and data, attempt history, workers, errors, timestamps, reason code, and panic information where available.

Replaying a DLQ job creates new work with lineage back to the original job. Replay does not erase history and does not pretend the original side effects never happened.

## Scheduling and time semantics

Azums supports:

- `run_at` and relative delays
- execution deadlines
- per-attempt handler timeouts
- retry backoff
- fixed-interval recurring jobs

The guarantee is eligibility, not perfect timing. A job is not intentionally leased before its documented `run_at` according to the backend clock. Azums does not promise that it starts at that exact instant because worker availability, polling, notifications, clock behavior, and infrastructure pauses affect wake-up latency.

Recurring execution is fixed-interval rather than calendar-based. Calendar schedules, daylight-saving interpretation, and automatic generation of every missed occurrence are outside the 1.0 guarantee.

## Jobs and durable streams in one system

Azums also provides event streams with append, sequence offsets, subscriptions, consumer-group ACK, replay, and retention-aware pruning.

Events receive monotonically increasing sequence numbers within a stream. If a consumer group has acknowledged offset `120`, its next event is the first retained event with a sequence greater than `120`.

Offsets only move forward, but delivery remains at least once. Consumer groups persist progress; they do not automatically assign partitions or balance work between members.

That model works for audit logs, projections, AI workflow events, data pipelines, and consumers that need to rebuild state from history.

## One API, honest backend differences

| Backend | Best fit | Operational boundary |
|---|---|---|
| Memory | Unit tests and ephemeral work | Process-local and non-durable |
| SQLite | Embedded apps, CLIs, desktop, and edge | Durable file storage with single-process coordination |
| PostgreSQL | Transactional services and multi-host workers | Same-database transactions, distributed leasing, execution rate limits |
| Redis | Low-latency distributed jobs and streams | Persistence depends on Redis configuration; no cross-SQL transaction |

Applications can inspect capabilities at runtime:

```rust
let client = azums::quickstart("memory").await?;
let capabilities = client.capabilities();

if capabilities.distributed_workers {
    println!("This backend supports distributed worker coordination");
}
```

The capability model covers transactional enqueue, durability, notifications, streams, consumer groups, distributed workers, ordering, backpressure, retention, and transaction scope.

This allows the same job code to grow with the application:

- start with Memory for tests
- use SQLite for an embedded or single-binary deployment
- move to PostgreSQL for transactional multi-host workers
- choose Redis for a Redis-native distributed environment

## Where Azums fits in Rust

Azums is Tokio-native and ships integration crates for Axum, Actix Web, Poem, and Rocket. The published crate family includes `azums`, `azums-core`, `azums-postgres`, `azums-redis`, `azums-axum`, `azums-actix`, `azums-poem`, and `azums-rocket`.

That gives it a useful range across the Rust ecosystem:

- Web services can move email, webhook, billing, and indexing work outside request latency.
- Database-backed applications can commit domain changes and follow-up jobs atomically.
- CLI, desktop, and edge applications can use a durable SQLite queue without operating a broker.
- Microservices can coordinate workers through PostgreSQL or Redis.
- AI applications can make long-running inference and tool workflows retryable, timed, recoverable, and traceable.
- Event-driven systems can retain offsets and replay streams for new consumers.
- Tests can run the same handler logic entirely in memory.

## What stable 1.0 promises

Azums 1.0 makes the following portable promises:

- at-least-once execution for successfully enqueued, runnable, non-cancelled work while the backend retains it and workers are available
- rejection of illegal lifecycle transitions
- terminal states remain terminal
- at most one valid active lease for a job
- recovery of expired leases
- deterministic failure classification, retry, timeout, cancellation, and DLQ behavior
- one logical job for a non-null idempotency key
- no intentional leasing before scheduling eligibility
- monotonic stream sequences and consumer offsets
- replay with preserved lineage and history

Other behavior is explicitly backend-dependent:

- durability and retention
- transactional enqueue scope
- distributed worker coordination
- notification and wake-up latency
- ordering strength
- backpressure enforcement
- Redis persistence and eviction behavior

And Azums explicitly does **not** guarantee:

- exactly-once execution or external side effects
- execution at an exact wall-clock instant
- global ordering, completion ordering, or worker fairness
- transactions across arbitrary external services
- automatic consumer-group balancing
- permanent retention or automatic scaling
- cancellation undoing an external side effect that already happened

This is what stability means for Azums: guaranteed behavior remains compatible throughout `1.x`, backend-dependent behavior remains declared through capabilities, and unspecified behavior is not quietly marketed as a promise.

## How we tested the release

The `v1.0.0` release candidate passed the workspace and integration suites, property tests, fuzz-hardening tests, documentation build, dependency audit, API compatibility checks, and benchmark harness.

The longer reliability gates included:

- 10,000 randomized chaos scenarios
- worker crash and lease-recovery tests
- transaction commit, rollback, connection-loss, and failure-boundary tests
- concurrency runs from 1 to 100 workers
- a 24-case matrix reaching one million jobs per case
- 6.96 million total job executions in the final large-scale matrix

That evidence means no known documented guarantee was violated by the tested release. It does not mean defects are impossible, and benchmark results are not semantic guarantees.

## Closing

Rust already gives us strong tools for building reliable software. Azums extends that mindset to asynchronous work by making the execution model explicit and the backend differences visible.

The important question is not only, "Can this queue process a job?"

It is:

> After retries, crashes, duplicates, restarts, and replays, can we still explain exactly what happened and which behavior was promised?

That is the standard Azums 1.0 is designed around.

- [Azums on GitHub](https://github.com/BlockForge-Dev/azums)
- [Azums on crates.io](https://crates.io/crates/azums)
- [Execution semantics](https://github.com/BlockForge-Dev/azums/blob/main/docs/src/semantics.md)
- [Backend compatibility matrix](https://github.com/BlockForge-Dev/azums/blob/main/docs/src/backend_equivalence.md)
- [Release-candidate evidence](https://github.com/BlockForge-Dev/azums/blob/main/docs/src/release_candidate.md)
