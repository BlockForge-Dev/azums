# Observability

Azums makes every durable job explainable through a stable observation contract.

The quickstart client exposes:

```rust
let explanation = queue.explain_job(job_id).await?;
let metrics = queue.queue_metrics(Some("default")).await?;
let log_event = queue.job_log_event(job_id).await?;
```

`JobExplanation` answers the operational question: what happened to this job?

It includes `job_id`, `job_type`, `queue`, current `status`, `retry_count`, latest
`worker_id`, latest `error`, propagated `trace_id`, lifecycle events, and a human-readable
summary. Backends with native attempt history include each durable attempt. Backends that only
expose the job row still return the same shape, but attempt-level latency and retry details are
backend-dependent.

## Structured Logs

`job_log_event(job_id)` returns a JSON object with the required production fields:

```json
{
  "job_id": "7c6b6b59-9b43-4a68-a6db-82bc3a59df9b",
  "attempt": 2,
  "worker_id": "worker-a",
  "queue": "email",
  "duration": 37,
  "duration_ms": 37,
  "status": "succeeded",
  "retry_count": 1,
  "error": null,
  "trace_id": "trace-123",
  "summary": "Job completed after 2 attempt(s)."
}
```

Applications can emit this object through `tracing`, `log`, JSON stdout, or their platform logger.
Azums does not force a logging backend.

## Metrics

`QueueMetrics` exposes:

- `jobs_total`
- `jobs_completed`
- `jobs_failed`
- `jobs_retried`
- `jobs_dlq`
- `queue_depth`
- `execution_latency_ms_avg`
- `claim_latency_ms_avg`
- `retry_latency_ms_avg`
- `worker_count`

Metrics are snapshots of persisted backend state. Memory has native attempt-level coverage. Generic
fallback metrics count job rows and terminal states, but attempt latency, retry latency, failed
attempts, and worker counts are backend-dependent unless the backend implements
`ObservabilityBackend`.

## Trace Propagation And Spans

Azums propagates trace identity from job payload fields named `trace_id` or
`metadata.trace_id`. That value is copied into explanations, structured log events, and observation
events.

Each `JobObservationEvent` can be mapped to OpenTelemetry span attributes:

```rust
let attrs = event.span_attributes();
assert_eq!(attrs["azums.job_id"], job_id.to_string());
```

Attribute names include `azums.job_id`, `azums.queue`, `azums.status`, `azums.attempt`,
`azums.worker_id`, `azums.duration_ms`, `azums.error`, and `trace_id`.

Azums provides OpenTelemetry-compatible span attributes, not an exporter. Applications choose their
own tracing subscriber, OTLP exporter, sampling policy, and backend.

## Guarantees

Guaranteed:

- `explain_job(job_id)` returns a stable shape when the job exists.
- Every returned observation event is derived from persisted job or attempt state.
- Structured log events use the same field names across backends.
- Terminal states remain visible through explanation APIs while retained by the backend.

Backend-dependent:

- Attempt-level history depth.
- Latency accuracy and granularity.
- Worker counts after worker shutdown.
- Metrics derived from archived job history.

Not guaranteed:

- Exactly-once external side effects.
- End-to-end distributed traces unless the application supplies and exports trace context.
- Visibility for data already pruned by retention or maintenance policies.
