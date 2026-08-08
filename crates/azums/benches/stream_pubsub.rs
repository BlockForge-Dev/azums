use azums::quickstart;
use criterion::{criterion_group, criterion_main, Criterion};
use serde_json::json;

fn bench_stream_pubsub(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("stream_publish_read_ack_1000_events", |b| {
        b.to_async(&rt).iter(|| async {
            let client = quickstart("memory").await.unwrap();
            let stream = client.stream("orders");

            for i in 1..=1000 {
                let _ = stream.publish("order_created", json!({"id": i})).await;
            }

            let events = stream.read_events(0, 1000).await.unwrap();
            assert_eq!(events.len(), 1000);

            stream.ack("analytics", 1000).await.unwrap();
        });
    });
}

criterion_group!(benches, bench_stream_pubsub);
criterion_main!(benches);
