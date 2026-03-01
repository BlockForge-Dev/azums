# Operator Runbook

## Purpose
Standard operating procedure for day-to-day platform operations.

## Pre-Checks
| Check | Procedure | Expected Result |
|---|---|---|
| Service health | Query proxy health/readiness endpoints | Healthy responses from proxy and status API |
| Queue backlog | Inspect jobs list by non-terminal states | Backlog within expected range |
| Callback failures | Inspect callback history for repeated failures | No sustained failure clusters |
| Intake rejections | Review intake audits by `validation_result=rejected` | Rejection rates within baseline |

## Request Investigation Workflow
| Step | Action |
|---|---|
| 1 | Capture `intent_id` and `tenant_id` from incident or alert |
| 2 | Query `GET /requests/:id` for current state and classification |
| 3 | Review `GET /requests/:id/history` to identify transition path |
| 4 | Review `GET /requests/:id/receipt` for readable phase context |
| 5 | Review `GET /requests/:id/callbacks` to separate delivery from execution |
| 6 | Decide next action: observe, replay, or escalate |

## Common Operator Actions
| Action | Endpoint/Tool | Notes |
|---|---|---|
| Inspect job queue | `GET /jobs` | Filter by state and date |
| Inspect request | `GET /requests/:id` | Includes classification and failure context |
| Inspect intake rejection cause | `GET /tenant/intake-audits` | Filter by `validation_result` and `channel` |
| Manage callback destination | `GET/POST/DELETE /tenant/callback-destination` | Admin role only |
| Trigger replay | `POST /requests/:id/replay` | Must include operator reason |

## Escalation Criteria
| Condition | Escalation |
|---|---|
| Cross-tenant data exposure concern | Immediate security escalation |
| Repeated terminal failures for same intent class | Platform engineering + adapter owner |
| Sustained callback failure cluster | Delivery owner + tenant integration contact |
| Unexpected illegal transition errors | Execution core owner |

