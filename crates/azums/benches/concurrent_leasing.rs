use azums::{quickstart, Job};
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;
use std::sync::Arc;

fn bench_concurrent_leasing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("concurrent_leasing");

    for num_workers in [4, 8, 16] {
        group.bench_function(format!("{}_workers_concurrent_lease", num_workers), |b| {
            b.to_async(&rt).iter(|| async {
                let flow = Arc::new(quickstart("memory").await.unwrap());

                // Enqueue 1,000 jobs
                for i in 0..1000 {
                    let _ = flow.enqueue(Job::new("work_unit", json!({"i": i}))).await;
                }

                let mut handles = Vec::new();
                for worker_idx in 0..num_workers {
                    let flow_clone = flow.clone();
                    let worker_id = format!("worker_{worker_idx}");
                    handles.push(tokio::spawn(async move {
                        let mut count = 0;
                        loop {
                            let batch = flow_clone
                                .backend()
                                .lease_jobs_batch("default", &worker_id, 30, 25)
                                .await
                                .unwrap();
                            if batch.is_empty() {
                                break;
                            }
                            count += batch.len();
                        }
                        count
                    }));
                }

                let mut total = 0;
                for h in handles {
                    total += h.await.unwrap();
                }
                assert_eq!(total, 1000);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_concurrent_leasing);
criterion_main!(benches);
