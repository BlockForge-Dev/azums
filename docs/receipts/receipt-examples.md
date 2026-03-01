# Receipt Examples

## Example A: Successful Execution
```json
{
  "tenant_id": "tenant_demo",
  "intent_id": "intent_abc123",
  "entries": [
    {
      "occurred_at_ms": 1767225601000,
      "state": "validated",
      "classification": "Success",
      "message": "Ingress accepted and normalized request"
    },
    {
      "occurred_at_ms": 1767225601200,
      "state": "queued",
      "classification": "Success",
      "message": "Intent queued for execution"
    },
    {
      "occurred_at_ms": 1767225601900,
      "state": "executing",
      "classification": "Success",
      "adapter_id": "adapter_solana",
      "attempt_no": 1,
      "message": "Adapter execution dispatched"
    },
    {
      "occurred_at_ms": 1767225604100,
      "state": "succeeded",
      "classification": "Success",
      "adapter_id": "adapter_solana",
      "attempt_no": 1,
      "message": "Execution completed",
      "details": {
        "signature": "5Pp...example",
        "provider": "solana_rpc_devnet"
      }
    }
  ]
}
```

## Example B: Retry Then Success
```json
{
  "tenant_id": "tenant_demo",
  "intent_id": "intent_retry_01",
  "entries": [
    {
      "occurred_at_ms": 1767225610000,
      "state": "executing",
      "classification": "Success",
      "adapter_id": "adapter_solana",
      "attempt_no": 1,
      "message": "Adapter execution dispatched"
    },
    {
      "occurred_at_ms": 1767225611200,
      "state": "retry_scheduled",
      "classification": "RetryableFailure",
      "adapter_id": "adapter_solana",
      "attempt_no": 1,
      "machine_reason": "rpc_timeout",
      "message": "Retry scheduled after retryable provider timeout",
      "details": {
        "next_retry_at_ms": 1767225615200
      }
    },
    {
      "occurred_at_ms": 1767225615600,
      "state": "executing",
      "classification": "Success",
      "adapter_id": "adapter_solana",
      "attempt_no": 2,
      "message": "Retry attempt dispatched"
    },
    {
      "occurred_at_ms": 1767225617800,
      "state": "succeeded",
      "classification": "Success",
      "adapter_id": "adapter_solana",
      "attempt_no": 2,
      "message": "Execution completed after retry"
    }
  ]
}
```

## Example C: Terminal Failure
```json
{
  "tenant_id": "tenant_demo",
  "intent_id": "intent_fail_99",
  "entries": [
    {
      "occurred_at_ms": 1767225620300,
      "state": "executing",
      "classification": "Success",
      "adapter_id": "adapter_solana",
      "attempt_no": 1,
      "message": "Adapter execution dispatched"
    },
    {
      "occurred_at_ms": 1767225621100,
      "state": "failed_terminal",
      "classification": "TerminalFailure",
      "adapter_id": "adapter_solana",
      "attempt_no": 1,
      "machine_reason": "insufficient_funds",
      "message": "Terminal failure: payer balance too low",
      "details": {
        "fixable_by_caller": true
      }
    }
  ]
}
```

