# Execution Core

`execution_core` is the policy and lifecycle brain for normalized intents.

It is intentionally platform-centric and adapter-agnostic.

## Responsibilities

- Canonical lifecycle states and legal transition enforcement.
- Adapter routing for supported normalized intent kinds.
- Adapter invocation through strict contract ports.
- Adapter outcome classification into platform outcomes.
- Retry scheduling and retry exhaustion handling.
- Terminal vs retryable vs blocked vs manual-review decisions.
- Durable state transition recording and canonical receipts.
- Intake idempotency dedupe by `tenant_id + idempotency_key`.
- Replay authorization and replay policy enforcement.
- Callback enqueue only after terminal state is durably written.

## Canonical States

- `received`
- `validated`
- `rejected`
- `queued`
- `leased`
- `executing`
- `retry_scheduled`
- `succeeded`
- `failed_terminal`
- `dead_lettered`
- `replayed`

## Core APIs

- `submit_intent(...)`
- `dispatch_job(...)`
- `handle_adapter_result(...)`
- `schedule_retry(...)`
- `mark_terminal_failure(...)`
- `emit_receipt(...)`
- `request_replay(...)`

## Security Controls

- Tenant-scoped operations (`tenant_id` is required and checked).
- Authorization gates for adapter routing and operator replay/manual actions.
- Replay only from replayable states and within replay budget.
- Illegal lifecycle transitions are rejected.
- Unsupported intents are rejected before adapter execution.
- Adapter contract violations are not promoted to platform truth.

## Ports

- `DurableStore`: intents/jobs/idempotency-bindings/transitions/receipts/replay decisions/dispatch+callback queues.
- `AdapterRouter` + `AdapterExecutor`: route and execute adapter requests.
- `Authorizer`: route, replay, and manual-action authorization.
- `Clock`: deterministic time source for scheduling and receipts.

## PostgresQ Integration

Enable feature `postgresq` for the concrete Postgres-backed store and worker loop:

```bash
cargo check --features postgresq
```

Feature module:

- `integration::postgresq::PostgresQStore`
- `integration::postgresq::PostgresQWorker`
- `integration::postgresq::PostgresQCallbackWorker`
- `integration::postgresq::HttpCallbackDispatcher`
- `integration::postgresq::StdoutCallbackDispatcher`

`PostgresQStore` persists core records in `execution_core_*` tables and enqueues dispatch/callback work into the existing `jobs` queue table.
