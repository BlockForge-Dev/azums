# Replay Runbook

## Purpose
Provide a safe and auditable replay process.

## Replay Preconditions
| Condition | Requirement |
|---|---|
| Authorization | Caller must satisfy replay authorization policy |
| Tenant scope | Replay must be requested under correct tenant |
| Eligibility | Core replay policy must allow replay for current state |
| Auditability | Reason for replay must be captured |

## Replay Procedure
| Step | Action | Verification |
|---|---|---|
| 1 | Confirm source request state and failure class | Request has durable record and known reason |
| 2 | Validate root cause is addressed | Caller/config/provider issue is mitigated |
| 3 | Trigger replay with reason | `POST /requests/:id/replay` returns replay job metadata |
| 4 | Track replay job transitions | New lineage appears in history/receipt |
| 5 | Confirm terminal outcome | Success or terminal failure is durably recorded |
| 6 | Confirm callback delivery status | Delivery result evaluated separately |

## Replay Safety Rules
| Rule | Rationale |
|---|---|
| Never mutate original history | Preserve forensic integrity |
| Never bypass status/core replay API | Enforce authorization + invariants |
| Always include human reason | Audit clarity and accountability |
| Verify idempotency assumptions | Avoid duplicate side effects |

## Replay Result Interpretation
| Outcome | Operator Response |
|---|---|
| Replay succeeds | Close incident with lineage record |
| Replay fails retryable | Monitor retry path or tune policy |
| Replay fails terminal | Escalate with updated evidence and failure class |
| Replay denied | Review authorization/eligibility policy mismatch |

