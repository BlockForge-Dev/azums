use crate::{
    backend::NotificationStream,
    model::{ConsumerGroupStatus, Event, NewEvent},
};
use async_trait::async_trait;

/// Interface for append-only, replayable event streams with consumer groups and acknowledgments.
#[async_trait]
pub trait StreamBackend: Send + Sync {
    /// Appends a new event to the specified stream log, returning its assigned sequence number.
    async fn publish(&self, stream: &str, event: NewEvent) -> anyhow::Result<i64>;

    /// Subscribes to notification events when new entries are appended to a stream.
    async fn subscribe_stream(
        &self,
        stream: &str,
        consumer_group: &str,
        last_seq: Option<i64>,
    ) -> anyhow::Result<NotificationStream>;

    /// Acknowledges event processing up to `seq` for a consumer group on a stream log.
    async fn ack(&self, stream: &str, consumer_group: &str, seq: i64) -> anyhow::Result<()>;

    /// Reads events from a stream with sequence numbers strictly greater than `after_seq`.
    async fn read_events(
        &self,
        stream: &str,
        after_seq: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<Event>>;

    /// Reads the next events for a consumer group from its durable acknowledged offset.
    async fn read_next(
        &self,
        stream: &str,
        consumer_group: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<Event>> {
        let last_acked_seq = self
            .consumer_group_info(stream)
            .await?
            .into_iter()
            .find(|status| status.consumer_group == consumer_group)
            .map(|status| status.last_acked_seq)
            .unwrap_or(0);

        self.read_events(stream, last_acked_seq, limit).await
    }

    /// Prunes retained events for `stream` with `sequence_no <= through_seq`.
    ///
    /// Implementations must not prune past the lowest known consumer-group offset. If no consumer
    /// groups are known for the stream, `through_seq` is used as the retention cutoff.
    async fn prune_events(&self, stream: &str, through_seq: i64) -> anyhow::Result<u64>;

    /// Fetches consumer group offset status for a stream log.
    async fn consumer_group_info(&self, stream: &str) -> anyhow::Result<Vec<ConsumerGroupStatus>>;
}
