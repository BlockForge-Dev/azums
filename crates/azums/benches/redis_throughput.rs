use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use tokio::runtime::Runtime;

fn bench_redis_enqueue_and_stream(c: &mut Criterion) {
    if std::env::var("AZUMS_BENCH_REDIS").as_deref() != Ok("1") {
        eprintln!("skipping redis_throughput; set AZUMS_BENCH_REDIS=1 to run Redis benchmarks");
        return;
    }

    let rt = Runtime::new().unwrap();

    let redis_url =
        std::env::var("REDIS_URL").expect("REDIS_URL must be set when AZUMS_BENCH_REDIS=1");

    let mut group = c.benchmark_group("redis_throughput");
    group.sample_size(10);
    group.measurement_time(std::time::Duration::from_secs(10));

    group.bench_function("redis_enqueue_100_jobs", |b| {
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

    group.bench_function("redis_stream_publish_100_events", |b| {
        b.to_async(&rt).iter(|| async {
            if let Ok(client) = azums::quickstart(&redis_url).await {
                let stream = client.stream("bench_stream");
                for i in 0..100 {
                    let _ = stream.publish("bench_event", json!({"i": i})).await;
                }
            }
        });
    });

    group.finish();
}

criterion_group!(benches, bench_redis_enqueue_and_stream);
criterion_main!(benches);
