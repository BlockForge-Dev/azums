# Azums — Low-Level Design (LLD) & Data Structures Document

## 1. Executive Summary & Core Philosophy

`azums` is a high-performance background job processing and event streaming framework for Rust designed to operate across the entire application spectrum — from zero-I/O embedded devices (SQLite / In-Memory) to multi-node distributed cloud clusters (PostgreSQL).

Unlike conventional job queues that introduce secondary network infrastructure (such as Redis or RabbitMQ), `azums` turns your application's existing relational or embedded database into an ACID-compliant, lock-free task broker using native database concurrency primitives (e.g. `FOR UPDATE SKIP LOCKED` in PostgreSQL and WAL-mode `IMMEDIATE` transactions in SQLite).

---

## 2. Workspace Crate Layout

The project is structured as a multi-crate Cargo workspace to maximize modularity and maintain minimal dependency trees:

```
azums (Workspace Root)
├── crates/azums-core           # Zero-dependency no_std + alloc core traits & models
├── crates/azums-postgres       # Standalone PostgreSQL storage backend driver
├── crates/azums                # Battery-included meta-crate, worker runtime, & CLI (azumsctl)
├── crates/azums-axum           # Native Axum web framework extractor & service layer
├── crates/azums-actix          # Native Actix Web extractor & state integration
├── crates/azums-poem           # Native Poem web framework extractor
├── crates/azums-rocket        # Native Rocket request guard & fairing
└── crates/worker               # Standalone distributed background worker binary
```

---

## 3. Core Abstractions & API Contracts

### `StorageBackend` Trait
The central interface for storage drivers (`PostgresBackend`, `SqliteBackend`, `MemoryBackend`).

```rust
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn run_migrations(&self) -> anyhow::Result<()>;
    async fn health_check(&self) -> anyhow::Result<()>;
    async fn enqueue(&self, job: NewJob) -> anyhow::Result<Uuid>;
    async fn lease_jobs_batch(
        &self,
        queue: &str,
        worker_id: &str,
        lease_seconds: i64,
        batch_size: i64,
    ) -> anyhow::Result<Vec<Job>>;
    async fn reap_expired_locks(&self) -> anyhow::Result<u64>;
    async fn start_attempts_batch(
        &self,
        dataset_ids: &[String],
        job_ids: &[Uuid],
        worker_id: &str,
    ) -> anyhow::Result<Vec<(Uuid, Uuid, i32)>>;
    async fn mark_succeeded(&self, job_id: Uuid, attempt_id: Uuid, worker_id: &str, latency_ms: i32) -> anyhow::Result<()>;
    async fn reschedule_for_retry(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
        next_run_at: DateTime<Utc>,
        error_code: &str,
        error_message: &str,
        attempt_no: i32,
    ) -> anyhow::Result<()>;
    async fn mark_dlq(
        &self,
        job_id: Uuid,
        attempt_id: Uuid,
        worker_id: &str,
        latency_ms: i32,
        reason_code: &str,
        error_code: &str,
        error_message: &str,
        attempt_no: i32,
    ) -> anyhow::Result<()>;
    async fn get_job(&self, job_id: Uuid) -> anyhow::Result<Option<Job>>;
    async fn list_jobs(&self, queue: Option<&str>, status: Option<&str>, limit: i64, cursor_created_at: Option<DateTime<Utc>>, cursor_id: Option<Uuid>) -> anyhow::Result<Vec<JobListItem>>;
    async fn replay_job(&self, job_id: Uuid, override_queue: Option<&str>, override_run_at: Option<DateTime<Utc>>) -> anyhow::Result<Uuid>;
}
```

### Job Model (`Job` & `NewJob`)
- **`Job`**: Complete state representation including `id` (UUIDv4), `queue`, `job_type`, `payload` (`serde_json::Value`), `priority` (`i32`), `max_attempts` (`i32`), `status`, `locked_at`, `locked_by`, `lock_expires_at`, `dlq_reason_code`, `created_at`, `updated_at`.
- **`NewJob`**: Payload struct provided during job creation.

---

## 4. PostgreSQL Storage Engine Architecture

### Database Schema
Jobs are stored in a main parent table partitioned by `dataset_id` (`<queue>_<YYYYMMDD_HH>`), allowing high-throughput parallel writes and fast time-range drop/archiving operations.

```sql
CREATE TABLE jobs (
  dataset_id text NOT NULL DEFAULT 'legacy',
  id uuid NOT NULL DEFAULT gen_random_uuid(),
  queue text NOT NULL,
  job_type text NOT NULL,
  payload_json jsonb NOT NULL DEFAULT '{}'::jsonb,
  run_at timestamptz NOT NULL DEFAULT now(),
  status text NOT NULL DEFAULT 'queued'
    CHECK (status IN ('queued','running','succeeded','failed','dlq','canceled')),
  priority int NOT NULL DEFAULT 0,
  max_attempts int NOT NULL DEFAULT 25,
  locked_at timestamptz NULL,
  locked_by text NULL,
  lock_expires_at timestamptz NULL,
  last_error_code text NULL,
  last_error_message text NULL,
  dlq_reason_code text NULL,
  dlq_at timestamptz NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  replay_of_job_id uuid NULL,
  PRIMARY KEY (dataset_id, id)
) PARTITION BY LIST (dataset_id);
```

### Transactional Lease Algorithm (`FOR UPDATE SKIP LOCKED`)
Batch job leasing uses a Common Table Expression (CTE) to atomically select, lock, and transition jobs from `queued` to `running`:

```sql
WITH candidates AS (
    SELECT id
    FROM jobs
    WHERE dataset_id = $1 AND queue = $2
      AND status = 'queued' AND run_at <= now()
    ORDER BY priority DESC, run_at ASC, created_at ASC
    FOR UPDATE SKIP LOCKED
    LIMIT $3
),
leased AS (
    UPDATE jobs j
    SET status = 'running',
        locked_by = $4,
        locked_at = now(),
        lock_expires_at = now() + ($5::int * interval '1 second'),
        updated_at = now()
    FROM candidates c
    WHERE j.id = c.id
    RETURNING j.*
)
SELECT * FROM leased
ORDER BY priority DESC, run_at ASC, created_at ASC;
```

---

## 5. SQLite Storage Engine Architecture

- **Write-Ahead Logging (WAL)**: SQLite connections are configured with `PRAGMA journal_mode = WAL;` and `PRAGMA busy_timeout = 5000;`.
- **Immediate Transactions**: Lease and state update queries execute inside `BEGIN IMMEDIATE` transactions to prevent write contention deadlocks.
- **Single-Writer Concurrency**: Manages in-process worker locking via channel queues to optimize embedded throughput.

---

## 6. In-Memory Storage Engine Architecture

- **`MemoryBackend`**: Implements `StorageBackend` using `Arc<RwLock<HashMap<Uuid, Job>>>` and `Arc<RwLock<Vec<MemoryAttempt>>>`.
- **Zero I/O Execution**: Enables instant sub-millisecond execution for local development and unit tests without external databases.

---

## 7. Data Structures & Algorithms (DSA) Analysis

### Complexity Matrix

| Component / Operation | Underlying Data Structure | Time Complexity (Average) | Time Complexity (Worst) | Space Complexity |
|---|---|---|---|---|
| **Postgres Job Selection** | Partial B-Tree Index `(queue, status, priority DESC, run_at ASC)` | $\mathcal{O}(\log N + k)$ | $\mathcal{O}(\log N + k)$ | $\mathcal{O}(N)$ |
| **Memory Backend Lease** | Priority Linear Scan / Min-Heap | $\mathcal{O}(N \log k)$ | $\mathcal{O}(N \log k)$ | $\mathcal{O}(N)$ |
| **Exponential Backoff** | Bit-Shift Left (`1_i64 << exp`) + Uniform Jitter | $\mathcal{O}(1)$ | $\mathcal{O}(1)$ | $\mathcal{O}(1)$ |
| **Queue Rate Control** | Fixed-Window Atomic Counter | $\mathcal{O}(1)$ | $\mathcal{O}(1)$ | $\mathcal{O}(Q)$ |
| **Stream Watermark Tracking** | Monotonic Sequence Pointer | $\mathcal{O}(1)$ | $\mathcal{O}(1)$ | $\mathcal{O}(C)$ |

### Exponential Backoff with Jitter Formula
When a job failure is classified as `Retryable`, the wait duration before the next attempt is calculated using exponential backoff with full jitter to avoid thundering herd contention:

$$T_{\text{delay}} = \text{clamp}\left(0, T_{\text{max}}, \text{round}\left(T_{\text{base}} \cdot 2^{a-1} + \text{Uniform}(-\delta, +\delta)\right)\right)$$

Where:
- $T_{\text{base}} = 2$ seconds
- $a = \text{attempt\_number}$
- $\delta = (T_{\text{base}} \cdot 2^{a-1}) \cdot 0.20$ (20% jitter range)
- $T_{\text{max}} = 900$ seconds (15 minutes)

---

## 8. Web Framework Extractors

`azums` supplies zero-boilerplate extractors for popular Rust web frameworks:
- **`azums-axum`**: `JobQueue` extractor implementing `FromRequestParts<S>`
- **`azums-actix`**: `JobQueue` extractor implementing `FromRequest`
- **`azums-poem`**: `JobQueue` extractor implementing `FromRequest`
- **`azums-rocket`**: `JobQueue` request guard implementing `FromRequest`
