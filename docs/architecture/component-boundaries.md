# Component Boundaries

## Boundary Rule
Each component owns one type of responsibility. Execution truth belongs to durable core/store state.

## Layer Responsibilities
| Component | Must Do | Must Never Do |
|---|---|---|
| Reverse proxy | TLS/edge filtering, path routing, exposure control | Decide business outcomes or adapter logic |
| Ingress API | Authenticate, verify, validate schema, normalize intent, enqueue durably | Execute long-running domain work or invent final truth |
| PostgresQ + core persistence | Store jobs, transitions, attempts, schedules, lineage | Run domain logic directly |
| Execution core | Own lifecycle, route adapter, classify outcomes, enforce retries/replay policy | Become chain/provider specific |
| Adapters | Execute domain actions and return normalized structured results | Redefine platform lifecycle or mutate unrelated jobs |
| Callback core | Deliver committed outcomes outward, track attempts | Infer execution success independently |
| Status API | Serve tenant-scoped read models and authorized commands | Write execution truth ad hoc |
| Operator UI | Present status and controlled actions | Bypass status/core authorization boundaries |

## Explicit Interface Boundaries
| From | To | Contract |
|---|---|---|
| Ingress | Execution core/store | Normalized intent submission |
| Core | Adapter | Adapter execution contract |
| Adapter | Core | Structured adapter outcome |
| Core | Callback core | Callback job payload after durable state commit |
| Status API | Core | Authorized replay command only |
| Operator UI | Status API | Read/query + replay operations via API |

## Anti-Patterns to Reject
| Anti-Pattern | Risk |
|---|---|
| Adapter-specific state names outside core contract | Fragmented status semantics |
| Direct callback publish before durable commit | External false positives |
| UI-driven retries bypassing core | Missing policy checks and lineage |
| Cross-tenant query leakage | Security and compliance failure |
| Log-derived inferred truth | Inconsistent operator decisions |

