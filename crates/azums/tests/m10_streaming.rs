use azums::{quickstart, StreamHandle};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Barrier;
use tokio::time::{timeout, Duration};
use tokio_stream::StreamExt;

#[tokio::test]
async fn m10_consumer_group_offsets_define_next_event_without_ambiguity() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let stream = client.stream("m10-orders");

    publish_range(&stream, 1, 5).await?;

    stream.ack("consumer-a", 3).await?;
    stream.ack("consumer-b", 1).await?;

    let next_a = stream.read_next("consumer-a", 10).await?;
    let next_b = stream.read_next("consumer-b", 10).await?;
    let next_c = stream.read_next("consumer-c", 10).await?;

    assert_sequence_numbers(&next_a, &[4, 5]);
    assert_sequence_numbers(&next_b, &[2, 3, 4, 5]);
    assert_sequence_numbers(&next_c, &[1, 2, 3, 4, 5]);

    stream.ack("consumer-a", 2).await?;
    assert_sequence_numbers(&stream.read_next("consumer-a", 10).await?, &[4, 5]);

    Ok(())
}

#[tokio::test]
async fn m10_consumer_restart_uses_persisted_offset() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let stream = client.stream("m10-restart");

    publish_range(&stream, 1, 4).await?;
    stream.ack("restarted-consumer", 2).await?;

    let restarted_handle = client.stream("m10-restart");
    let next = restarted_handle.read_next("restarted-consumer", 10).await?;
    assert_sequence_numbers(&next, &[3, 4]);

    Ok(())
}

#[tokio::test]
async fn m10_unacked_events_are_delivered_again_until_ack() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let stream = client.stream("m10-duplicates");

    publish_range(&stream, 1, 3).await?;

    let first_delivery = stream.read_next("crashy-consumer", 2).await?;
    let second_delivery = stream.read_next("crashy-consumer", 2).await?;
    assert_sequence_numbers(&first_delivery, &[1, 2]);
    assert_sequence_numbers(&second_delivery, &[1, 2]);

    stream.ack("crashy-consumer", 2).await?;
    assert_sequence_numbers(&stream.read_next("crashy-consumer", 10).await?, &[3]);

    Ok(())
}

#[tokio::test]
async fn m10_replay_reads_from_requested_offset_without_mutating_group_offset() -> anyhow::Result<()>
{
    let client = quickstart("memory").await?;
    let stream = client.stream("m10-replay");

    publish_range(&stream, 1, 8).await?;
    stream.ack("analytics", 6).await?;

    let replay = stream.read_events(2, 3).await?;
    assert_sequence_numbers(&replay, &[3, 4, 5]);
    assert_sequence_numbers(&stream.read_next("analytics", 10).await?, &[7, 8]);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m10_concurrent_consumers_keep_independent_monotonic_offsets() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let stream = Arc::new(client.stream("m10-concurrent"));
    publish_range(&stream, 1, 100).await?;

    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = Vec::new();

    for (group, ack_to) in [("consumer-a", 100), ("consumer-b", 40), ("consumer-c", 75)] {
        let stream = stream.clone();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            let events = stream.read_next(group, 100).await.unwrap();
            assert_eq!(events[0].sequence_no, 1);
            stream.ack(group, ack_to).await.unwrap();
        }));
    }

    for task in tasks {
        task.await?;
    }

    assert!(stream.read_next("consumer-a", 10).await?.is_empty());
    assert_sequence_numbers(&stream.read_next("consumer-b", 3).await?, &[41, 42, 43]);
    assert_sequence_numbers(&stream.read_next("consumer-c", 3).await?, &[76, 77, 78]);

    Ok(())
}

#[tokio::test]
async fn m10_retention_does_not_prune_events_needed_by_slowest_consumer() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let stream = client.stream("m10-retention");

    publish_range(&stream, 1, 10).await?;
    stream.ack("fast", 8).await?;
    stream.ack("slow", 3).await?;

    let pruned = stream.prune_events(8).await?;
    assert_eq!(pruned, 3);

    assert_sequence_numbers(&stream.read_events(0, 10).await?, &[4, 5, 6, 7, 8, 9, 10]);
    assert_sequence_numbers(
        &stream.read_next("slow", 10).await?,
        &[4, 5, 6, 7, 8, 9, 10],
    );
    assert_sequence_numbers(&stream.read_next("fast", 10).await?, &[9, 10]);

    Ok(())
}

#[tokio::test]
async fn m10_subscribe_is_a_wakeup_hint_and_events_remain_readable() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let stream = client.stream("m10-subscribe");
    let mut notifications = stream.subscribe("subscriber", None).await?;

    let seq = stream.publish("created", json!({"id": 1})).await?;
    assert_eq!(seq, 1);

    let notification = timeout(Duration::from_secs(1), notifications.next()).await?;
    assert!(notification.is_some());
    assert_sequence_numbers(&stream.read_next("subscriber", 10).await?, &[1]);

    Ok(())
}

async fn publish_range(stream: &StreamHandle, start: i64, end: i64) -> anyhow::Result<()> {
    for value in start..=end {
        let seq = stream
            .publish("m10_event", json!({ "value": value }))
            .await?;
        assert_eq!(seq, value);
    }
    Ok(())
}

fn assert_sequence_numbers(events: &[azums::Event], expected: &[i64]) {
    let actual: Vec<i64> = events.iter().map(|event| event.sequence_no).collect();
    assert_eq!(actual, expected);
}
