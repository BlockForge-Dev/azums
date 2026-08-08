use azums::quickstart;
use serde_json::json;

#[tokio::test]
async fn test_in_memory_stream_10k_events_publish_read_ack_replay() -> anyhow::Result<()> {
    let client = quickstart("memory").await?;
    let stream = client.stream("orders");

    let count = 10_000;

    for i in 1..=count {
        let seq = stream
            .publish("order_created", json!({ "index": i }))
            .await?;
        assert_eq!(
            seq, i as i64,
            "Sequence number should be 1-based strictly increasing"
        );
    }

    // Read all events in batches of 1000
    let mut read_events = Vec::new();
    let mut last_seq = 0i64;

    loop {
        let batch = stream.read_events(last_seq, 1000).await?;
        if batch.is_empty() {
            break;
        }
        last_seq = batch.last().unwrap().sequence_no;
        read_events.extend(batch);
    }

    assert_eq!(read_events.len(), count);

    for (idx, event) in read_events.iter().enumerate() {
        let expected_seq = (idx + 1) as i64;
        assert_eq!(event.sequence_no, expected_seq);
        assert_eq!(event.event_type, "order_created");
        assert_eq!(event.payload_json["index"], expected_seq);
    }

    // Acknowledge for consumer group "analytics"
    stream.ack("analytics", 10_000).await?;

    let group_info = stream.consumer_group_info().await?;
    assert_eq!(group_info.len(), 1);
    assert_eq!(group_info[0].consumer_group, "analytics");
    assert_eq!(group_info[0].last_acked_seq, 10_000);

    // Replay from arbitrary offset (e.g. sequence 5,000)
    let replayed = stream.read_events(5000, 100).await?;
    assert_eq!(replayed.len(), 100);
    assert_eq!(replayed[0].sequence_no, 5001);
    assert_eq!(replayed[99].sequence_no, 5100);

    Ok(())
}

#[tokio::test]
async fn test_sqlite_stream_publish_read_ack_replay() -> anyhow::Result<()> {
    let db_url = format!(
        "sqlite://file:test_sqlite_stream_{}?mode=memory&cache=shared",
        uuid::Uuid::new_v4()
    );
    let client = quickstart(&db_url).await?;
    let stream = client.stream("payments");

    let total = 500;
    for i in 1..=total {
        let seq = stream
            .publish("payment_processed", json!({ "id": i }))
            .await?;
        assert_eq!(seq, i as i64);
    }

    let events = stream.read_events(0, 1000).await?;
    assert_eq!(events.len(), total);
    assert_eq!(events[0].sequence_no, 1);
    assert_eq!(events[total - 1].sequence_no, total as i64);

    stream.ack("ledger_group", 250).await?;

    let info = stream.consumer_group_info().await?;
    assert_eq!(info.len(), 1);
    assert_eq!(info[0].last_acked_seq, 250);

    // Replay after offset 250
    let replay_after_offset = stream.read_events(250, 50).await?;
    assert_eq!(replay_after_offset.len(), 50);
    assert_eq!(replay_after_offset[0].sequence_no, 251);

    Ok(())
}
