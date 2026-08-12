# Retry, Failure Classification & DLQ

Azums failure handling is deterministic:

```text
RUNNING
  |
  v
FAIL
  |
  +-- retryable and attempts remain --> RETRY_WAIT --> QUEUED
  |
  `-- terminal or attempts exhausted --> DLQ
```

## Failure Classes

| Class | Error codes | Retry behavior | Terminal reason |
|---|---|---|---|
| Retryable error | `HANDLER_ERROR`, unknown codes | Retries until `max_attempts` | `MAX_ATTEMPTS_EXCEEDED` |
| Timeout | `TIMEOUT` | Retries until `max_attempts` | `MAX_ATTEMPTS_EXCEEDED` |
| System failure | `DEPENDENCY_DOWN`, `DB_DEADLOCK`, `SERIALIZATION`, `RATE_LIMIT`, `DB_DISCONNECT`, `SYSTEM_FAILURE`, `LEASE_EXPIRED` | Retries until `max_attempts` | `MAX_ATTEMPTS_EXCEEDED` |
| Permanent error | `BAD_PAYLOAD`, `UNKNOWN_JOB_TYPE`, `PERMANENT_ERROR` | No retry | `PERMANENT_ERROR` |
| Panic | `PANIC` | No retry | `PANIC` |
| Cancelled | `CANCELLED` | No retry; cancellation uses `status = 'canceled'` | `CANCELLED` when represented as a failed attempt |

Handlers can opt into a specific class by returning an error string with a known prefix:

```text
TIMEOUT: upstream email API did not respond
BAD_PAYLOAD: missing user_id
SYSTEM_FAILURE: database connection dropped
```

Plain handler errors are classified as `HANDLER_ERROR` and are retryable by default.

## Retry Policy

Retry delay is exponential backoff with optional jitter and a cap:

```text
delay = min(max_seconds, base_seconds * 2^(attempt_no - 1))
```

With `base_seconds = 1`, `jitter_pct = 0`, and `max_seconds = 16`, attempts schedule as:

```text
1s, 2s, 4s, 8s, 16s, 16s...
```

Jitter is applied after the capped base delay and clamped to `[0, max_seconds]`.

## DLQ Inspection

A DLQ job remains the original job row with:

- original job ID
- queue and job type
- payload, including any application metadata stored in the payload
- priority and retry budget
- timestamps
- `dlq_reason_code`
- `dlq_at`

Attempt rows preserve:

- attempt number
- worker identity
- start and finish timestamps
- latency
- error code
- error message
- panic information where available as `error_code = 'PANIC'` and the panic payload in `error_message`

The timeline API reconstructs the lifecycle from the job row, attempt rows, and policy decisions. Replay creates a new queued job with `replay_of_job_id` pointing at the original DLQ job.

## Non-Guarantees

DLQ does not mean the handler had no external side effects. A timeout, panic, crash, or process death may happen after partial work. Applications that touch external systems still need idempotency keys or dedupe storage.
