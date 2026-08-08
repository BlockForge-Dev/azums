use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use tokio::runtime::Runtime;

fn bench_redis_enqueue_and_stream(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

    c.bench_function("redis_enqueue_100_jobs", |b| {
        b.to_async(&rt).iter(|| async {
            if let Ok(client) = azums::quickstart(&redis_url).await {
                for _ in 0..100 {
                    let _ = client
                        .enqueue(azums::Job::new("bench_job", json!({"key": "val"})))
                        .await;
                }
            }
        });
    });

    c.bench_function("redis_stream_publish_100_events", |b| {
        b.to_async(&rt).iter(|| async {
            if let Ok(client) = azums::quickstart(&redis_url).await {
                let stream = client.stream("bench_stream");
                for i in 0..100 {
                    let _ = stream.publish("bench_event", json!({"i": i})).await;
                }
            }
        });
    });
}

criterion_group!(benches, bench_redis_enqueue_and_stream);
criterion_main!(benches);
