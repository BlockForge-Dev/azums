# Callback Core

`callback_core` is the delivery/egress layer for execution outcomes.

## Responsibilities

- Read committed callback jobs from durable queue storage.
- Deliver outbound callbacks (stdout or HTTP).
- Optionally sign outbound callback payloads.
- Classify delivery failures into retryable vs terminal.
- Retry callback jobs with backoff.
- Record durable delivery status and attempt history.
- Reduce duplicate outward delivery with callback-level claim checks.

## Primary Types

- `PostgresQCallbackWorker`
- `PostgresQCallbackWorkerConfig`
- `PostgresQDeliveryStore`
- `CallbackDispatcher`
- `HttpCallbackDispatcher`
- `StdoutCallbackDispatcher`
- `TenantRoutedCallbackDispatcher`

## Store APIs

- `publish_callback(...)`
- `retry_callback(...)`
- `record_delivery_attempt(...)`
- `get_delivery_status(...)`
- `list_delivery_attempts(...)`
- `get_tenant_destination(...)`
- `upsert_tenant_destination(...)`
- `delete_tenant_destination(...)`

## Security Notes

- HTTP destinations are validated and can be host-allowlisted.
- Private/local destinations can be blocked by default.
- Optional HMAC signature headers can be attached to each callback.
- Delivery state is stored separately from execution outcome state.
