use azums::{quickstart, Job};
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use std::env;

fn bench_enqueue(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("enqueue_throughput");
    group.warm_up_time(std::time::Duration::from_secs(1));
    group.measurement_time(std::time::Duration::from_secs(3));

    // 1. In-Memory Backend Enqueue
    let mem_flow = rt.block_on(async { quickstart("memory").await.unwrap() });
    group.bench_function("in_memory_enqueue", |b| {
        b.to_async(&rt).iter(|| async {
            mem_flow
                .enqueue(Job::new("enqueue_test", json!({"bench": "memory"})))
                .await
                .unwrap();
        });
    });

    // 2. Postgres Backend Enqueue (if DATABASE_URL / TEST_DATABASE_URL available)
    if let Ok(pg_url) = env::var("DATABASE_URL").or_else(|_| env::var("TEST_DATABASE_URL")) {
        if let Ok(pg_flow) = rt.block_on(async { quickstart(&pg_url).await }) {
            group.bench_function("postgres_enqueue", |b| {
                b.to_async(&rt).iter(|| async {
                    pg_flow
                        .enqueue(Job::new("enqueue_test", json!({"bench": "postgres"})))
                        .await
                        .unwrap();
                });
            });
        }
    }

    // 3. Redis Backend Enqueue (if REDIS_URL available or default redis)
    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    if let Ok(redis_flow) = rt.block_on(async { quickstart(&redis_url).await }) {
        group.bench_function("redis_enqueue", |b| {
            b.to_async(&rt).iter(|| async {
                redis_flow
                    .enqueue(Job::new("enqueue_test", json!({"bench": "redis"})))
                    .await
                    .unwrap();
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_enqueue);
criterion_main!(benches);
