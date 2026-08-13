# Durable Event Streaming

M10 defines streams as a first-class Azums primitive: append-only logs with monotonic offsets, consumer groups, acknowledgement, replay, subscription wake-ups, and bounded retention.

## Core Rule

For a given stream and consumer group:

```text
next event = first retained event where sequence_no > last_acked_seq
```

If the consumer group has never ACKed, `last_acked_seq = 0`.

Example:

```text
EVENT LOG: 1 2 3 ... 120

Consumer A last_acked_seq = 120 -> next = none until 121 exists
Consumer B last_acked_seq = 84  -> next = 85
Consumer C last_acked_seq = 42  -> next = 43
```

## Guarantees

| Primitive | Contract |
|---|---|
| Append | `publish` appends one event and returns a 1-based `sequence_no` that increases within the stream. |
| Read by offset | `read_events(after_seq, limit)` returns retained events where `sequence_no > after_seq` in ascending order. |
| Read by group | `read_next(group, limit)` reads from the group's durable `last_acked_seq`. |
| ACK | `ack(group, seq)` advances that group's offset monotonically; lower ACKs do not move it backward. |
| Independent offsets | Each consumer group has its own offset. ACKing group A does not affect group B. |
| Restart | A restarted consumer receives from its persisted group offset. |
| Replay | Reading from an older offset returns retained historical events and does not mutate group offsets. |
| Duplicate delivery | Events read but not ACKed are delivered again on the next group read. |
| Subscribe | `subscribe` is a wake-up hint. Durable delivery still comes from reading the stream log. |
| Retention | `prune_events(through_seq)` never prunes past the lowest known consumer-group offset. |

## Non-Guarantees

Azums does not guarantee:

- Exactly-once stream consumer side effects.
- Automatic load balancing within a consumer group.
- Pending-entry ownership transfer.
- Global ordering across different streams.
- Unlimited retention.
- That notifications are delivered exactly once or at all; notifications are hints.

## Retention

Retention is explicit. If no consumer group exists for a stream, `prune_events(through_seq)` may delete retained events with `sequence_no <= through_seq`.

If consumer groups exist, pruning is capped by the slowest known group:

```text
cutoff = min(requested_through_seq, min(last_acked_seq across groups))
```

This keeps replay deterministic for known consumers while still allowing operators to bound storage growth.

## Backend Notes

| Backend | Stream durability |
|---|---|
| Memory | Process-local, useful for tests and ephemeral workflows. |
| SQLite | Durable when file-backed; embedded/single-process target. |
| PostgreSQL | Durable SQL log and offsets; strongest operational choice. |
| Redis | Durable only when Redis persistence is configured; stream entries are stored as Redis data structures. |
