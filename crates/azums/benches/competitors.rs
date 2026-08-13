use azums::{quickstart, Job};
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use std::time::{Duration, Instant};

fn bench_wake_up_vs_polling(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("latency_wake_up_vs_polling");

    group.bench_function("azums_event_driven_listen_notify", |b| {
        b.to_async(&rt).iter(|| async {
            let flow = quickstart("memory").await.unwrap();
            let mut stream = flow.backend().subscribe("default").await.unwrap();

            let (tx, rx) = tokio::sync::oneshot::channel::<Instant>();

            let handle = tokio::spawn(async move {
                use tokio_stream::StreamExt;
                let _ = stream.next().await;
                let _ = tx.send(Instant::now());
            });

            tokio::time::sleep(Duration::from_millis(1)).await;
            let start = Instant::now();
            flow.enqueue(Job::new("fast_job", json!({}))).await.unwrap();

            let rcv_time = rx.await.unwrap();
            let _ = handle.await;
            std::hint::black_box(rcv_time.duration_since(start));
        });
    });

    group.bench_function("simulated_500ms_busy_polling", |b| {
        b.to_async(&rt).iter(|| async {
            let start = Instant::now();
            // Busy polling loop checking DB every 500ms
            tokio::time::sleep(Duration::from_millis(250)).await; // avg latency
            let elapsed = start.elapsed();
            std::hint::black_box(elapsed);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_wake_up_vs_polling);
criterion_main!(benches);
