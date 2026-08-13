use azums_core::{
    backend::NotificationStream,
    model::{ConsumerGroupStatus, Event, NewEvent},
};
use sqlx::PgPool;

/// Repository providing PostgreSQL transactional event streaming operations.
#[derive(Clone)]
pub struct StreamRepo {
    pool: PgPool,
}

impl StreamRepo {
    /// Creates a new `StreamRepo` wrapping a SQLx PostgreSQL connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn notify_channel_name(stream: &str) -> String {
        let sanitized: String = stream
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() {
                    ch.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        format!("azums_stream_{sanitized}")
    }

    /// Appends a new event into `stream_events` and issues PostgreSQL `NOTIFY`.
    pub async fn publish(&self, stream: &str, event: NewEvent) -> anyhow::Result<i64> {
        let seq = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO stream_events (stream_name, event_type, payload_json, created_at)
            VALUES ($1, $2, $3, now())
            RETURNING sequence_no
            "#,
        )
        .bind(stream)
        .bind(event.event_type)
        .bind(event.payload_json)
        .fetch_one(&self.pool)
        .await?;

        let channel = Self::notify_channel_name(stream);
        let _ = sqlx::query("SELECT pg_notify($1, '')")
            .bind(&channel)
            .execute(&self.pool)
            .await;

        Ok(seq)
    }

    /// Subscribes to PostgreSQL `LISTEN` events for stream updates.
    pub async fn subscribe_stream(
        &self,
        stream: &str,
        _consumer_group: &str,
        _last_seq: Option<i64>,
    ) -> anyhow::Result<NotificationStream> {
        use sqlx::postgres::PgListener;
        use tokio_stream::StreamExt;

        let channel = Self::notify_channel_name(stream);
        let mut listener = PgListener::connect_with(&self.pool).await?;
        listener.listen(&channel).await?;

        let stream = listener
            .into_stream()
            .filter_map(|res| res.ok().map(|_| ()));
        Ok(Box::pin(stream))
    }

    /// Acknowledges processed sequence number up to `seq` for a consumer group.
    pub async fn ack(&self, stream: &str, consumer_group: &str, seq: i64) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO stream_offsets (consumer_group, stream_name, last_acked_seq, updated_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT (consumer_group, stream_name)
            DO UPDATE SET last_acked_seq = GREATEST(stream_offsets.last_acked_seq, EXCLUDED.last_acked_seq),
                          updated_at = now()
            "#,
        )
        .bind(consumer_group)
        .bind(stream)
        .bind(seq)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Reads events from a stream log with sequence numbers strictly greater than `after_seq`.
    pub async fn read_events(
        &self,
        stream: &str,
        after_seq: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<Event>> {
        let events = sqlx::query_as::<_, Event>(
            r#"
            SELECT sequence_no, stream_name, event_type, payload_json, created_at
            FROM stream_events
            WHERE stream_name = $1 AND sequence_no > $2
            ORDER BY sequence_no ASC
            LIMIT $3
            "#,
        )
        .bind(stream)
        .bind(after_seq)
        .bind(limit.clamp(1, 1000))
        .fetch_all(&self.pool)
        .await?;

        Ok(events)
    }

    /// Prunes retained stream events without deleting entries needed by known consumer groups.
    pub async fn prune_events(&self, stream: &str, through_seq: i64) -> anyhow::Result<u64> {
        let res = sqlx::query(
            r#"
            WITH retention AS (
                SELECT LEAST(
                    $2::bigint,
                    COALESCE(
                        (SELECT MIN(last_acked_seq) FROM stream_offsets WHERE stream_name = $1),
                        $2::bigint
                    )
                ) AS cutoff
            )
            DELETE FROM stream_events
            USING retention
            WHERE stream_name = $1
              AND sequence_no <= retention.cutoff
            "#,
        )
        .bind(stream)
        .bind(through_seq)
        .execute(&self.pool)
        .await?;

        Ok(res.rows_affected())
    }

    /// Returns offset status for consumer groups registered on a stream log.
    pub async fn consumer_group_info(
        &self,
        stream: &str,
    ) -> anyhow::Result<Vec<ConsumerGroupStatus>> {
        let info = sqlx::query_as::<_, ConsumerGroupStatus>(
            r#"
            SELECT consumer_group, stream_name, last_acked_seq, updated_at
            FROM stream_offsets
            WHERE stream_name = $1
            "#,
        )
        .bind(stream)
        .fetch_all(&self.pool)
        .await?;

        Ok(info)
    }
}
