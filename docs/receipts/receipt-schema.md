# Receipt Schema

## Purpose
A receipt is the durable explainability object for a request across its entire lifecycle.

## Receipt Record Shape
| Field | Type | Description |
|---|---|---|
| `tenant_id` | string | Tenant isolation boundary |
| `intent_id` | string | Request/intent identifier |
| `entries[]` | array | Ordered timeline of receipt events |

## Receipt Entry Shape
| Field | Type | Description |
|---|---|---|
| `occurred_at_ms` | uint64 | Event timestamp |
| `state` | enum | Canonical lifecycle state at event |
| `classification` | enum | Platform result class for this event |
| `adapter_id` | string optional | Adapter involved in this phase |
| `attempt_no` | uint32 optional | Attempt number for retries/replays |
| `message` | string | Human-readable explanation |
| `machine_reason` | string optional | Structured reason code |
| `details` | object optional | Event metadata (domain evidence, route rule, etc.) |

## Required Receipt Coverage
| Category | Coverage Requirement |
|---|---|
| Intake | acceptance/rejection and key identifiers |
| Queue lifecycle | queued, leased, retry scheduling |
| Execution | adapter dispatch and classified outcomes |
| Failure details | stage, class, message, reason code |
| Delivery | callback outcome linkage (separate from execution success) |
| Replay lineage | replay trigger and linkage to source |

## Design Rules
| Rule | Requirement |
|---|---|
| Append-friendly | New entries append; prior history remains intact |
| Chronological readability | Ordered for operator timeline review |
| Machine + human balance | Include structured fields and readable messages |
| Tenant-safe | No cross-tenant leakage in receipt content |

