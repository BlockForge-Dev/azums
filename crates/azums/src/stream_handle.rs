use azums_core::{
    model::{ConsumerGroupStatus, Event, NewEvent},
    NotificationStream, StorageBackend, StreamBackend,
};
use std::sync::Arc;

/// High-level handle for durable stream log operations produced by [`Client::stream`](crate::Client::stream).
#[derive(Clone)]
pub struct StreamHandle {
    backend: Arc<dyn StorageBackend>,
    stream_name: String,
}

impl StreamHandle {
    /// Creates a new `StreamHandle` bound to a specific stream log name.
    pub fn new(backend: Arc<dyn StorageBackend>, stream_name: impl Into<String>) -> Self {
        Self {
            backend,
            stream_name: stream_name.into(),
        }
    }

    /// Returns the name of the target stream log.
    pub fn name(&self) -> &str {
        &self.stream_name
    }

    fn stream_backend(&self) -> anyhow::Result<&dyn StreamBackend> {
        self.backend
            .as_stream()
            .ok_or_else(|| anyhow::anyhow!("Current backend does not support StreamBackend"))
    }

    /// Appends a new event into the stream log, returning its assigned 1-based sequence number.
    pub async fn publish(
        &self,
        event_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> anyhow::Result<i64> {
        let sb = self.stream_backend()?;
        sb.publish(&self.stream_name, NewEvent::new(event_type, payload))
            .await
    }

    /// Reads events from the stream log with sequence numbers strictly greater than `after_seq`.
    pub async fn read_events(&self, after_seq: i64, limit: i64) -> anyhow::Result<Vec<Event>> {
        let sb = self.stream_backend()?;
        sb.read_events(&self.stream_name, after_seq, limit).await
    }

    /// Acknowledges event processing up to sequence number `seq` for a consumer group.
    pub async fn ack(&self, consumer_group: &str, seq: i64) -> anyhow::Result<()> {
        let sb = self.stream_backend()?;
        sb.ack(&self.stream_name, consumer_group, seq).await
    }

    /// Returns offset status for consumer groups registered on this stream log.
    pub async fn consumer_group_info(&self) -> anyhow::Result<Vec<ConsumerGroupStatus>> {
        let sb = self.stream_backend()?;
        sb.consumer_group_info(&self.stream_name).await
    }

    /// Subscribes to real-time notification events when new entries are appended to the stream.
    pub async fn subscribe(
        &self,
        consumer_group: &str,
        last_seq: Option<i64>,
    ) -> anyhow::Result<NotificationStream> {
        let sb = self.stream_backend()?;
        sb.subscribe_stream(&self.stream_name, consumer_group, last_seq)
            .await
    }
}
