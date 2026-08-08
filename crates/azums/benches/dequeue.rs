use azums::{quickstart, Job};
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use std::env;

fn bench_dequeue_and_process(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("dequeue_processing_latency");
    group.warm_up_time(std::time::Duration::from_secs(1));
    group.measurement_time(std::time::Duration::from_secs(3));

    // 1. In-Memory Dequeue & Process Batch (10 jobs)
    group.bench_function("in_memory_dequeue_10_jobs", |b| {
        b.to_async(&rt).iter(|| async {
            let flow = quickstart("memory").await.unwrap();
            flow.register_handler("dequeue_task", |_job| async move { Ok(()) })
                .await;

            for _ in 0..10 {
                flow.enqueue(Job::new("dequeue_task", json!({"bench": "dequeue"})))
                    .await
                    .unwrap();
            }

            let processed = flow.run_until_empty().await.unwrap();
            assert_eq!(processed, 10);
        });
    });

    // 2. Redis Dequeue & Process Batch (10 jobs)
    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    if rt.block_on(async { quickstart(&redis_url).await }).is_ok() {
        group.bench_function("redis_dequeue_10_jobs", |b| {
            b.to_async(&rt).iter(|| async {
                let flow = quickstart(&redis_url).await.unwrap();
                flow.register_handler("dequeue_task", |_job| async move { Ok(()) })
                    .await;

                for _ in 0..10 {
                    flow.enqueue(Job::new("dequeue_task", json!({"bench": "dequeue"})))
                        .await
                        .unwrap();
                }

                let processed = flow.run_until_empty().await.unwrap();
                assert_eq!(processed, 10);
            });
        });
    }

    // 3. Postgres Dequeue & Process Batch (10 jobs)
    if let Ok(pg_url) = env::var("DATABASE_URL").or_else(|_| env::var("TEST_DATABASE_URL")) {
        if rt.block_on(async { quickstart(&pg_url).await }).is_ok() {
            group.bench_function("postgres_dequeue_10_jobs", |b| {
                b.to_async(&rt).iter(|| async {
                    let flow = quickstart(&pg_url).await.unwrap();
                    flow.register_handler("dequeue_task", |_job| async move { Ok(()) })
                        .await;

                    for _ in 0..10 {
                        flow.enqueue(Job::new("dequeue_task", json!({"bench": "dequeue"})))
                            .await
                            .unwrap();
                    }

                    let processed = flow.run_until_empty().await.unwrap();
                    assert_eq!(processed, 10);
                });
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_dequeue_and_process);
criterion_main!(benches);
