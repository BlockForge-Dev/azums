use azums::{quickstart, Job};
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

fn bench_wake_up_latency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("notify_driven_wakeup_latency", |b| {
        b.to_async(&rt).iter(|| async {
            let flow = quickstart("memory").await.unwrap();
            let (tx, _rx) = tokio::sync::oneshot::channel::<Instant>();

            flow.register_handler("bench_wakeup", move |_job| {
                let start = Instant::now();
                let _ = tx;
                async move {
                    let elapsed = start.elapsed();
                    assert!(elapsed.as_millis() < 50);
                    Ok(())
                }
            })
            .await;

            let worker_flow = Arc::new(flow);
            let worker_flow_clone = worker_flow.clone();

            let stop = Arc::new(AtomicBool::new(false));
            let stop_clone = stop.clone();

            let handle = tokio::spawn(async move {
                let mut stream = worker_flow_clone.backend().subscribe("default").await.unwrap();
                use tokio_stream::StreamExt;
                while !stop_clone.load(Ordering::Relaxed) {
                    let batch = worker_flow_clone
                        .backend()
                        .lease_jobs_batch("default", "bench_worker", 10, 1)
                        .await
                        .unwrap();
                    if batch.is_empty() {
                        tokio::select! {
                            _ = stream.next() => {},
                            _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {},
                        }
                        continue;
                    }
                    stop_clone.store(true, Ordering::Relaxed);
                    break;
                }
            });

            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            let start = Instant::now();
            worker_flow
                .enqueue(Job::new("bench_wakeup", json!({})))
                .await
                .unwrap();

            let _ = handle.await;
            let wake_latency = start.elapsed();
            assert!(wake_latency.as_millis() < 50);
        });
    });
}

criterion_group!(benches, bench_wake_up_latency);
criterion_main!(benches);
