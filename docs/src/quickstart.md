# Zero-Config Quickstart

You can add `postgresflow` to your application and start processing background jobs in just a few lines of Rust.

## Step 1: Add Dependency

In `Cargo.toml`:

```toml
[dependencies]
postgresflow = "0.2"
tokio = { version = "1", features = ["full"] }
serde_json = "1"
anyhow = "1"
```

## Step 2: Code Example

```rust,no_run
use postgresflow::{quickstart, Job};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Automatically connects to local Postgres or DATABASE_URL and runs migrations
    let flow = quickstart("postgres://localhost/flow").await?;

    // Enqueue a job with a JSON payload
    flow.enqueue(Job::new("greet", serde_json::json!({"name": "World"}))).await?;

    // Register async job handler
    flow.register_handler("greet", |job| async move {
        println!("Hello, {}!", job.payload["name"]);
        Ok(())
    }).await;

    // Run the worker loop and admin server
    flow.run().await?;

    Ok(())
}
```

## Step 3: Run against Docker Compose DB

PostgresFlow includes a ready-to-use local environment in `docker-compose.yml`:

```bash
docker compose up -d db
cargo run
```

You will see:
```text
Hello, "World"!
admin api listening on http://127.0.0.1:3003
```
