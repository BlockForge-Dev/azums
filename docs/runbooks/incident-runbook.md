# Incident Runbook

## Severity Model
| Severity | Definition | Example |
|---|---|---|
| Sev-1 | Platform-wide outage or security boundary failure | Cross-tenant leakage, broad request loss |
| Sev-2 | Major degradation with high customer impact | Execution backlog growth with widespread terminal failures |
| Sev-3 | Localized tenant or adapter issue | Single-tenant callback failures |

## Incident Response Stages
| Stage | Actions |
|---|---|
| Detect | Alert, user report, error-rate or queue-depth threshold breach |
| Triage | Confirm scope by tenant, adapter, state, and timeframe |
| Mitigate | Contain impact, apply temporary controls, stabilize queue growth |
| Recover | Restore expected flow and verify durable state consistency |
| Review | Produce timeline, root cause, corrective actions |

## Diagnostic Queries
| Objective | Query Surface |
|---|---|
| Find affected requests | `GET /jobs` with state filters |
| Inspect failure progression | `GET /requests/:id/history` |
| Inspect human-readable evidence | `GET /requests/:id/receipt` |
| Separate delivery vs execution issues | `GET /requests/:id/callbacks` |
| Measure intake rejection spike | `GET /tenant/intake-audits` |

## Containment Playbook
| Problem Pattern | Immediate Action |
|---|---|
| Provider instability (retry storm) | Tighten retry policy or temporarily pause affected adapter path |
| Callback destination outage | Disable destination or increase backoff |
| Bad intake payload wave | Tighten schema checks and notify caller integration owner |
| Unauthorized replay attempts | Confirm auth config and review action audit trails |

## Post-Incident Deliverables
| Deliverable | Owner |
|---|---|
| Incident timeline (UTC) | Incident commander |
| Root cause + contributing factors | Service owner |
| Corrective action plan | Engineering lead |
| Runbook/document updates | Component owner |

