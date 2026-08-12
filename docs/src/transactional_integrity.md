# Transactional Integrity

Transactional enqueue is the strongest Azums guarantee for SQL backends:

```text
BEGIN
  application mutation
  enqueue job
COMMIT
```

If the transaction commits successfully, both the application mutation and the job are durable. If the transaction rolls back, fails during commit, loses the connection before commit, or the process terminates before commit, neither side is durable.

## Supported Backends

| Backend | Transactional enqueue | Contract |
|---|---|---|
| PostgreSQL | Yes | App data and job rows commit/roll back together when they use the same PostgreSQL transaction. |
| SQLite | Yes | App data and job rows commit/roll back together when they use the same SQLite transaction. |
| Redis | No | Redis enqueue is atomic inside Redis, but not part of a SQL app-data transaction. |
| Memory | No | Memory enqueue is process-local and not durable. |

## APIs

PostgreSQL:

```rust,no_run
use azums::{Job, PostgresBackend};
use serde_json::json;

# async fn example(pool: sqlx::PgPool, backend: PostgresBackend) -> anyhow::Result<()> {
let mut tx = pool.begin().await?;

sqlx::query("INSERT INTO users (id) VALUES ($1)")
    .bind("user_123")
    .execute(&mut *tx)
    .await?;

backend
    .enqueue_in_tx(
        &mut tx,
        Job::new("send_welcome_email", json!({"user_id": "user_123"})).into(),
    )
    .await?;

tx.commit().await?;
# Ok(())
# }
```

SQLite:

```rust,no_run
use azums::{Job, SqliteBackend};
use serde_json::json;

# async fn example(backend: SqliteBackend) -> anyhow::Result<()> {
let mut tx = backend.pool().begin().await?;

sqlx::query("INSERT INTO users (id) VALUES (?)")
    .bind("user_123")
    .execute(&mut *tx)
    .await?;

backend
    .enqueue_in_tx(
        &mut tx,
        Job::new("send_welcome_email", json!({"user_id": "user_123"})).into(),
    )
    .await?;

tx.commit().await?;
# Ok(())
# }
```

## Failure Matrix

| Failure point | Expected result | Automated coverage |
|---|---|---|
| Before enqueue | App mutation rolls back; no job exists. | SQLite rollback-before-enqueue test |
| After enqueue | App mutation and job roll back together. | SQLite and PostgreSQL rollback-after-enqueue tests |
| Before commit | Dropping the transaction rolls back app mutation and job. | SQLite and PostgreSQL drop-before-commit tests |
| During commit | Deferred constraint failure rolls back app mutation and job. | SQLite and PostgreSQL commit-failure tests |
| Connection loss | Dropping the uncommitted transaction/connection rolls back app mutation and job. | PostgreSQL connection-loss test |
| Process termination | Process exits without commit; uncommitted transaction is not durable. | SQLite child-process termination test |

## Notification Semantics

PostgreSQL `pg_notify` is issued inside the transaction, so PostgreSQL only delivers the wake-up if the transaction commits.

SQLite transactional enqueue does not emit an immediate wake-up from inside the transaction. SQLite workers still use interval fallback and storage state as the source of truth.

## Non-Guarantees

Transactional enqueue does not guarantee exactly-once external side effects. It only guarantees atomicity between application state and job state inside the same supported SQL backend transaction.

Transactional enqueue is not available across Redis plus a separate SQL database, Memory plus an external database, or arbitrary external services.
