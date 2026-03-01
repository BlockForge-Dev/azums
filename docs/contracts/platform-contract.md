# Platform Contract

## Objective
Define a stable interface between ingress, execution core, adapters, callback core, and query surfaces.

## Normalized Intent
| Field | Type | Notes |
|---|---|---|
| `request_id` | string | External-facing stable intake identifier |
| `intent_id` | string | Internal normalized command identifier |
| `tenant_id` | string | Hard authorization boundary |
| `kind` | string | Supported intent type (for routing + schema) |
| `payload` | object | Versioned adapter payload |
| `correlation_id` | string optional | Trace continuity across services |
| `idempotency_key` | string optional | Duplicate protection |
| `auth_context` | object optional | Principal, submitter_kind, channel, auth scheme |
| `metadata` | map<string,string> | Additional machine context |
| `received_at_ms` | uint64 | Intake timestamp |

## Normalized Outcome
| Field | Type | Notes |
|---|---|---|
| `intent_id` | string | Links to original normalized intent |
| `job_id` | string | Attempt-scoped execution identity |
| `adapter_id` | string | Which adapter handled execution |
| `state` | enum | Canonical lifecycle state |
| `classification` | enum | Platform-level result class |
| `retryable` | bool | Retry policy input |
| `failure_code` | string optional | Machine reason |
| `failure_message` | string optional | Human-readable explanation |
| `adapter_metadata` | object optional | Domain evidence (signature, provider info, etc.) |
| `updated_at_ms` | uint64 | Transition timestamp |

## Cross-Layer Invariants
| Invariant | Requirement |
|---|---|
| Tenant scope | All reads/writes and commands are tenant-bound |
| Deterministic lifecycle | States only transition via core policy |
| Outcome normalization | Provider-specific errors are normalized |
| Persistence first | Final truth committed before callbacks |
| Contract safety | Unsupported intents/adapters are rejected early |

