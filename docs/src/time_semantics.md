# Scheduling & Time Semantics

M9 defines Azums time behavior for scheduled, delayed, deadline-bound, timed-out, retried, and recurring work.

## Time Model

Azums stores all job timestamps as UTC instants.

| Field | Meaning |
|---|---|
| `run_at` | Earliest documented eligibility time. A job may not be leased while `run_at > now`. |
| `deadline_at` | Optional latest start time. If a job is eligible but the backend clock is already past `deadline_at`, Azums moves it to DLQ with `DEADLINE_EXCEEDED`. |
| `timeout_seconds` | Optional per-attempt handler timeout enforced by the worker runtime. Timeout failures use error code `TIMEOUT` and follow normal retry/DLQ policy. |
| `recurring_interval_seconds` | Optional fixed interval. After a successful occurrence, Azums enqueues the next occurrence as a new logical job. |

## Guarantees

- A scheduled job is never intentionally leased before `run_at <= now` according to the backend time source.
- Delayed enqueue is just `run_at = enqueue_time + delay`.
- Jobs scheduled before worker downtime become eligible immediately after workers restart.
- Expired-deadline jobs do not execute late; they transition to DLQ with `DEADLINE_EXCEEDED`.
- Retry backoff computes a future `run_at`; retries are not eligible before that timestamp.
- Handler timeout produces a retryable `TIMEOUT` failure until retry budget is exhausted.
- Recurrence schedules the next occurrence from the previous occurrence's `run_at`, not from completion time.
- Recurring jobs create new logical job IDs and preserve lineage through `replay_of_job_id`.

## Backend Time Source

| Backend | Eligibility clock |
|---|---|
| PostgreSQL | Database `now()` |
| SQLite | Azums process `Utc::now()` bound into SQL |
| Redis | Azums process `Utc::now()` during leasing |
| Memory | Azums process `Utc::now()` during leasing |

For distributed workers, prefer PostgreSQL when clock-skew safety matters because eligibility is evaluated by the database clock. For process-clock backends, workers should run with NTP or equivalent clock synchronization.

## Downtime & Long Pauses

If workers are stopped for one hour, Azums does not create a special catch-up state. On restart:

- Jobs with `run_at <= now` and no expired deadline are immediately eligible.
- Jobs with `deadline_at < now` move to DLQ instead of running late.
- Recurring jobs create the next occurrence only after a successful occurrence ACK.

This means recurrence is deterministic and conservative: Azums does not automatically generate every missed interval after downtime unless each prior occurrence is actually executed and ACKed.

## Daylight Saving Time

Azums does not store local civil times. Daylight-saving ambiguity must be resolved by the application before enqueueing. Once a local time is converted to a UTC `run_at`, Azums treats it as a normal instant.

## Non-Guarantees

Azums does not guarantee:

- Execution exactly at `run_at`.
- Protection from incorrect system clocks on Memory, SQLite, or Redis workers.
- Global clock-skew correction across workers.
- Automatic catch-up for every missed recurring interval.
- Calendar-aware recurrence such as "every weekday at 9am" or DST-aware local recurrence.
- Exactly-once external side effects when timeouts, retries, crash recovery, or replay occur.
