# Idempotency & Duplicate Execution

Azums provides at-least-once delivery. Idempotency makes that safe.

There are two different semantics:

| Concern | Meaning | Azums primitive |
|---|---|---|
| Delivery semantics | How many logical jobs exist and how often Azums may deliver them. | `idempotency_key` deduplicates enqueue attempts into one logical job. |
| Side-effect semantics | How many times external work actually happens. | Application-owned idempotency key or dedupe store inside the handler. |

## Enqueue Idempotency

Set `idempotency_key` when retrying a producer request might enqueue the same logical work more than once:

```rust,no_run
use azums::Job;
use serde_json::json;

# async fn example(flow: azums::QuickstartFlow) -> anyhow::Result<()> {
let job_id = flow
    .enqueue(
        Job::new("send_receipt", json!({"order_id": "ord_123"}))
            .idempotency_key("receipt:ord_123"),
    )
    .await?;
# Ok(())
# }
```

If another enqueue uses the same non-null key, Azums returns the existing logical job ID instead of creating a second job.

Replay intentionally clears the idempotency key. Replay creates new work with `replay_of_job_id` pointing at the original job.

## Duplicate Execution

Even with enqueue idempotency, duplicate delivery can still happen:

```text
handler performs external side effect
    |
worker crashes before ACK
    |
lease expires
    |
job is delivered again
```

Azums cannot know whether an email, payment, webhook, LLM call, or other external operation succeeded before the crash. The handler must protect that side effect.

Recommended pattern:

```sql
INSERT INTO processed_operations (operation_key, completed_at)
VALUES ($1, now())
ON CONFLICT (operation_key) DO NOTHING;
```

Only perform the external side effect when the insert wins, or pass the same key to a provider that supports idempotency keys.

## Guarantees

Azums guarantees:

- Enqueue attempts with the same non-null `idempotency_key` produce one logical job per backend idempotency scope.
- Duplicate enqueue callers receive the same job ID.
- The idempotency key is visible in job list output.
- Replay creates a new job and does not reuse the source job's idempotency key.

Azums does not guarantee:

- Exactly-once handler execution.
- Exactly-once external side effects.
- Automatic dedupe by payload, job type, business key, request ID, or stream event payload.
- Cross-backend or cross-service idempotency between Azums and arbitrary external systems.
