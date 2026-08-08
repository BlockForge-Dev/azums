use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents an immutable event stored within a durable stream log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Monotonically increasing 1-based sequence number within the stream.
    pub sequence_no: i64,
    /// Name of the target stream log (e.g., "orders", "audit_logs").
    pub stream: String,
    /// Domain-specific identifier for the event type (e.g., "order_created").
    pub event_type: String,
    /// JSON payload content of the event.
    pub payload: serde_json::Value,
    /// Timestamp when the event was appended to the stream log.
    pub created_at: DateTime<Utc>,
}

/// Input model for publishing a new event into a stream log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewEvent {
    /// Domain-specific identifier for the event type (e.g., "order_created").
    pub event_type: String,
    /// JSON payload content of the event.
    pub payload: serde_json::Value,
}

impl NewEvent {
    /// Creates a new `NewEvent` with the specified event type and JSON payload.
    pub fn new(event_type: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            event_type: event_type.into(),
            payload,
        }
    }
}

/// Status and offset information for a consumer group registered on a stream log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsumerGroupStatus {
    /// Identifier of the consumer group (e.g., "analytics_processor").
    pub consumer_group: String,
    /// Name of the stream log.
    pub stream: String,
    /// Highest sequence number successfully acknowledged by this consumer group.
    pub last_acked_seq: i64,
    /// Timestamp when the offset was last updated.
    pub updated_at: DateTime<Utc>,
}
