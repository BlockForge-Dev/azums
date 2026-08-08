-- Stream Events log table
CREATE TABLE IF NOT EXISTS stream_events (
    sequence_no BIGSERIAL PRIMARY KEY,
    stream_name TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_stream_events_lookup
    ON stream_events (stream_name, sequence_no ASC);

-- Consumer group sequence offset tracking
CREATE TABLE IF NOT EXISTS stream_offsets (
    consumer_group TEXT NOT NULL,
    stream_name TEXT NOT NULL,
    last_acked_seq BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (consumer_group, stream_name)
);
