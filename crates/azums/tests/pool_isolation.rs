mod common;
use common::setup_db;

use azums::{quickstart, Job};
use serde_json::json;

#[tokio::test]
async fn test_in_memory_pool_isolation() -> anyhow::Result<()> {
    let flow = quickstart("memory").await?.with_queue("isolation_q");
    flow.register_handler("test", |_j| async move { Ok(()) })
        .await;

    let _stream = flow.backend().subscribe("isolation_q").await?;

    flow.enqueue(Job::new("test", json!({})).queue("isolation_q"))
        .await?;

    let count = flow.run_until_empty().await?;
    assert_eq!(count, 1);

    Ok(())
}

#[tokio::test]
#[cfg(feature = "postgres")]
async fn test_postgres_pool_isolation_single_connection_pool() -> anyhow::Result<()> {
    if setup_db().await.is_none() {
        return Ok(());
    }

    let url = std::env::var("DATABASE_URL")
        .or_else(|_| std::env::var("TEST_DATABASE_URL"))
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/azums_dev".to_string());

    let opts = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(2));

    let pool = opts.connect(&url).await?;
    let backend = azums::PostgresBackend::new_with_url(pool, &url);
    let flow = azums::QuickstartFlow::new(std::sync::Arc::new(backend));

    // Subscribe opens unpooled LISTEN connection
    let _stream = flow.backend().subscribe("single_conn_q").await?;

    // Query pool max_connections=1 is still completely open for enqueuing and leasing!
    flow.enqueue(Job::new("isolated", json!({})).queue("single_conn_q"))
        .await?;

    flow.register_handler("isolated", |_j| async move { Ok(()) })
        .await;

    let count = flow.run_until_empty().await?;
    assert_eq!(count, 1);

    Ok(())
}
