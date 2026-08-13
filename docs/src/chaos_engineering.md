# Chaos Engineering

M11 makes destructive behavior an executable part of the test suite instead of an anecdote.

Azums chaos tests are invariant tests. They do not prove that external side effects are exactly-once, and they do not pretend every backend can expose the same failure controls. They prove the core reliability contract:

> No committed job silently disappears, and abandoned work becomes recoverable according to lease semantics.

## Automated Profiles

Run the default chaos profile:

```powershell
cargo test -p azums --test chaos
```

This runs:

- `m11_memory_randomized_chaos_ci_matrix`
- `m11_sqlite_contention_chaos_ci_matrix`

The memory profile executes hundreds of deterministic randomized scenarios by default. Increase it with:

```powershell
$env:AZUMS_CHAOS_CI_SCENARIOS = "1000"
$env:AZUMS_CHAOS_SEED = "0xA11CE2026"
cargo test -p azums --test chaos m11_memory_randomized_chaos_ci_matrix
```

Run the long M11 profile:

```powershell
$env:AZUMS_CHAOS_SCENARIOS = "10000"
$env:AZUMS_CHAOS_SEED = "0xA11CE10000"
cargo test -p azums --test chaos m11_memory_randomized_chaos_10000_plus -- --ignored
```

The long profile rejects values below 10,000 scenarios.

## Failure Model

The randomized memory profile injects:

- worker crash before attempt creation
- worker crash during attempt
- SIGKILL/process termination before ACK
- handler panic
- handler timeout
- database connection timeout
- database connection reset
- retryable system failure
- permanent failure
- successful ACK

Each scenario randomizes:

- job count
- worker count
- max attempts
- priority
- elapsed latency
- scheduling eligibility
- deadline expiry
- which fault happens to which leased job
- retry path versus DLQ path

The SQLite profile deliberately creates embedded-database contention with concurrent workers. Transient deadlocks are treated as recoverable chaos outcomes; the invariant is that contention cannot make a committed job vanish.

## Invariants

Every chaos scenario checks:

- every committed job ID remains readable
- every job reaches a terminal state after recovery
- terminal jobs have no active lease
- terminal jobs reject further cancellation
- abandoned running work can be reaped and executed again
- SQLite contention drains all committed jobs
- no queued or running SQLite work remains after recovery

## Backend Scope

| Backend | Automated M11 profile | What is proven |
|---|---|---|
| Memory | Randomized failure matrix, including the 10,000+ opt-in profile | Core state machine, retry/DLQ, terminality, abandoned lease recovery |
| SQLite | Concurrent contention profile | Single-writer contention is recoverable; committed jobs are not lost |
| PostgreSQL | Covered by existing lease, transactional enqueue, crash, and connection-loss tests; live database restart remains environment-dependent | SQL transaction and lease invariants under supported test environments |
| Redis | Covered by existing Redis backend tests when `REDIS_URL` is configured; disconnect/restart profiles are environment-dependent | Redis command-level atomicity and stream/job lifecycle under a reachable Redis service |

Azums does not fake live infrastructure failures. Restarting PostgreSQL, restarting Redis, and forcing real network partitions require control over the deployment environment, so those profiles are documented as environment-dependent unless the test runner provides those controls.

## Interpreting Failures

A chaos test failure should be classified as one of:

- **Guaranteed behavior violation**: a committed job disappeared, a terminal job became mutable, or duplicate active claims were observed.
- **Backend limitation**: the backend cannot expose the requested failure mode or isolation level.
- **Harness limitation**: the failure requires external process, network, or database orchestration not available to the current test runner.

The classification must be explicit. A failure mode is never silently ignored.
