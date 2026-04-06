# Paystack Adapter

`adapter_paystack` plugs Paystack-backed fiat execution into the Azums execution core.

## Supported intent kinds

- `paystack.transaction.verify.v1`
- `paystack.refund.create.v1`
- `paystack.refund.verify.v1`
- `paystack.transfer.create.v1`
- `paystack.transfer.verify.v1`

## Notes

- The adapter owns Paystack-specific payload validation, provider calls, and adapter-local durable evidence.
- Execution lifecycle, retries, receipts, replay, reconciliation, and exception handling remain owned by Azums core and downstream subsystems.
- `refund.create` and `transfer.create` avoid blind duplicate provider submission by persisting adapter-local state and converting ambiguous transport failures into manual review.
- Execution can use either:
  - a global worker fallback via `PAYSTACK_SECRET_KEY`
  - or a tenant/environment-scoped connector binding resolved through the ingress broker with:
    - `EXECUTION_CONNECTOR_BROKER_BASE_URL`
    - `EXECUTION_CONNECTOR_BROKER_BEARER_TOKEN`
    - `EXECUTION_CONNECTOR_BROKER_PRINCIPAL_ID`
- When a request carries `connector.binding_id` or payload `connector_binding_id`, the adapter resolves the secret through the connector broker instead of the worker-wide fallback.

## Reconciliation

- `recon_core` now ships a `paystack.v1` rule pack for `adapter_paystack`.
- Reconciliation consumes Paystack adapter evidence from `paystack.executions` plus provider evidence from `paystack.webhook_events`.
- The rule pack materializes expected facts for:
  - execution reference
  - verification state
  - amount
  - currency
  - source/destination reference when present
  - connector reference when present
- The rule pack classifies baseline divergence with explicit machine reasons:
  - `verification_reference_missing`
  - `payment_status_mismatch`
  - `amount_mismatch`
  - `currency_mismatch`
  - `verification_pending_too_long`
  - `duplicate_event`
