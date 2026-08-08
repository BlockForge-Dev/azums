use azums::{quickstart, Job};
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;

fn bench_enqueue_throughput(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let flow = rt.block_on(async { quickstart("memory").await.unwrap() });

    c.bench_function("enqueue_single_job", |b| {
        b.to_async(&rt).iter(|| async {
            flow.enqueue(Job::new("bench_task", json!({"x": 100})))
                .await
                .unwrap();
        });
    });
}

fn bench_worker_throughput(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("worker_process_batch_100", |b| {
        b.to_async(&rt).iter(|| async {
            let flow = quickstart("memory").await.unwrap();
            flow.register_handler("bench_task", |_job| async move { Ok(()) })
                .await;

            for _ in 0..100 {
                flow.enqueue(Job::new("bench_task", json!({"x": 100})))
                    .await
                    .unwrap();
            }

            let processed = flow.run_until_empty().await.unwrap();
            assert_eq!(processed, 100);
        });
    });
}

criterion_group!(benches, bench_enqueue_throughput, bench_worker_throughput);
criterion_main!(benches);
