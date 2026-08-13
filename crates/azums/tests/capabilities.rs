mod common;

use azums::{
    make_sqlite_pool, quickstart, BackendCapabilities, BackpressureCapability, OrderingCapability,
    PostgresBackend, SqliteBackend, StorageBackend,
};

#[tokio::test]
async fn memory_declares_process_local_capabilities() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;

    assert_eq!(
        client.capabilities(),
        BackendCapabilities {
            transactional_enqueue: false,
            durable_jobs: false,
            notifications: true,
            streams: true,
            consumer_groups: true,
            distributed_workers: false,
            ordering: OrderingCapability::FifoAndFastestLeasing,
            backpressure: BackpressureCapability::BacklogOnly,
        }
    );

    Ok(())
}

#[tokio::test]
async fn sqlite_declares_embedded_sql_capabilities() -> anyhow::Result<()> {
    let db_url = format!(
        "sqlite://file:test_sqlite_capabilities_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let pool = make_sqlite_pool(&db_url).await?;
    let backend = SqliteBackend::new(pool);

    assert_eq!(
        backend.capabilities(),
        BackendCapabilities {
            transactional_enqueue: true,
            durable_jobs: true,
            notifications: true,
            streams: true,
            consumer_groups: true,
            distributed_workers: false,
            ordering: OrderingCapability::FifoAndFastestLeasing,
            backpressure: BackpressureCapability::BacklogOnly,
        }
    );

    Ok(())
}

#[tokio::test]
async fn postgres_declares_distributed_sql_capabilities_when_available() -> anyhow::Result<()> {
    let Some(pool) = common::setup_db().await else {
        return Ok(());
    };
    let backend = PostgresBackend::new(pool);

    assert_eq!(
        backend.capabilities(),
        BackendCapabilities {
            transactional_enqueue: true,
            durable_jobs: true,
            notifications: true,
            streams: true,
            consumer_groups: true,
            distributed_workers: true,
            ordering: OrderingCapability::FifoAndFastestLeasing,
            backpressure: BackpressureCapability::ExecutionRateLimit,
        }
    );

    Ok(())
}

#[tokio::test]
async fn redis_declares_atomic_distributed_capabilities_when_available() -> anyhow::Result<()> {
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

    let client = match quickstart(&redis_url).await {
        Ok(c) => c,
        Err(_) => {
            eprintln!(
                "Skipping live Redis capabilities test: No Redis server reachable at {redis_url}"
            );
            return Ok(());
        }
    };

    assert_eq!(
        client.capabilities(),
        BackendCapabilities {
            transactional_enqueue: false,
            durable_jobs: true,
            notifications: true,
            streams: true,
            consumer_groups: true,
            distributed_workers: true,
            ordering: OrderingCapability::FifoLeasing,
            backpressure: BackpressureCapability::BacklogOnly,
        }
    );

    Ok(())
}
