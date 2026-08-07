# Job Lifecycle

Jobs in PostgresFlow transition through distinct, well-defined lifecycle states: `queued`, `running`, `succeeded`, `failed`, `dlq`, and `canceled`.

## State Diagram

```mermaid
stateDiagram-v2
    [*] --> Queued : Enqueue (now or scheduled)
    Queued --> Running : Worker Lease (SKIP LOCKED)
    Running --> Succeeded : Handler Success
    Running --> Queued : Retryable Failure (Backoff)
    Running --> DLQ : Max Attempts Exceeded / Non-retryable
    Queued --> Canceled : User Cancellation
    DLQ --> Queued : Manual Replay
    Succeeded --> Archived : Maintenance Archive
```

## Execution Sequence

```mermaid
sequenceDiagram
    autonumber
    actor App as Application / Client
    participant DB as Postgres DB
    participant Worker as Worker Node
    participant Handler as Job Handler

    App->>DB: Enqueue Job (status='queued')
    Worker->>DB: Lease Batch (FOR UPDATE SKIP LOCKED)
    DB-->>Worker: Return leased jobs (status='running', locked_by=worker_id)
    Worker->>DB: Start Attempt Record (job_attempts)
    Worker->>Handler: Execute Registered Closure
    alt Success
        Handler-->>Worker: Ok(())
        Worker->>DB: Update job (status='succeeded') & Finish Attempt
    else Retryable Failure
        Handler-->>Worker: Err(RetryableError)
        Worker->>DB: Reschedule job (status='queued', run_at = now() + backoff)
    else Exhausted / Non-Retryable Error
        Handler-->>Worker: Err(FatalError)
        Worker->>DB: Move job (status='dlq', dlq_reason_code)
    end
```
