# Developer Experience & Integration

M16 formalizes the adoption path:

```powershell
cargo add azums
```

Then:

```rust,no_run
use azums::{quickstart, Job};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let queue = quickstart("memory").await?;

    queue
        .register_handler("send_email", |job| async move {
            println!("email payload = {}", job.payload);
            Ok(())
        })
        .await;

    queue
        .enqueue(Job::new("send_email", json!({ "to": "new@example.com" })))
        .await?;

    queue.run_until_empty().await?;
    Ok(())
}
```

## Progressive Path

| Step | API |
|---|---|
| Basic enqueue | `quickstart("memory").await?`, `queue.enqueue(Job::new(...)).await?` |
| Retry | Return `Err(anyhow!("SYSTEM_FAILURE: ..."))`, set `Job::max_attempts(n)` |
| Scheduling | `Job::run_at(...)`, `Job::deadline_at(...)`, `Job::timeout_seconds(...)` |
| Transactions | SQL backends expose `enqueue_in_tx` on PostgreSQL/SQLite backends |
| Workers | `register_handler`, `register_processor`, `run_until_empty`, `run_with_shutdown` |
| Streams | `queue.stream("events").publish(...)`, `read_events(...)` |
| Consumer groups | `read_next("group", limit)`, `ack("group", seq)` |
| Observability | `queue.get_job(job_id)`, `queue.replay_job(job_id)`, `queue.capabilities()` |

## One-File Example

Run:

```powershell
cargo run --example install_enqueue_process_retry_inspect
```

That example demonstrates:

- install-level imports
- enqueue
- handler registration
- retry on transient failure
- inspect after retry scheduling
- inspect after completion

## Integration Notes

| Environment | First choice |
|---|---|
| Tokio service | `quickstart(url).await?` inside startup state |
| CLI app | `quickstart("sqlite://jobs.db?mode=rwc").await?` or `quickstart("memory").await?` |
| Embedded/desktop | SQLite URL with WAL enabled by Azums |
| AI application | Enqueue model/tool jobs with idempotency keys for request dedupe |
| Axum | Share `Client`/`QuickstartFlow` in app state or use the Axum integration crate |
| Actix | Share the client through app data or use the Actix integration crate |
| Poem | Share the client through app data or use the Poem integration crate |
| Rocket | Manage the client as Rocket state or use the Rocket integration crate |

Advanced users can still drop to `StorageBackend`, backend-specific repositories, transactional enqueue, and the architecture book when they need exact storage semantics.
