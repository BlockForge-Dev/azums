# Azums Durable Execution Platform
## Product and Technical Documentation

Version: 0.1  
Date: 2026-02-28  
Audience: platform engineers, solution architects, operators, and technical buyers

## 1. Executive Summary
Azums is a self-hostable durable execution platform that accepts supported intents (API or webhook), records them durably, executes them through a strict execution core plus adapter contract, and returns explainable outcomes through receipts, status APIs, and delivery history.

Core promise:

For every accepted request, the platform either:

1. Completes the action, or
2. Produces a durable, queryable, replay-safe explanation of what happened and why.

## 2. Product Definition
### 2.1 What It Is
Azums is a multi-service execution platform with:

- strict lifecycle ownership in a chain-agnostic execution core
- adapter-based domain execution (Solana is first adapter)
- durable receipts and state transitions
- callback delivery tracking separate from execution truth
- operator-grade replay and audit visibility

### 2.2 What It Is Not
Azums is not:

- only a queue
- only a webhook relay
- only an RPC abstraction
- only a Solana-specific product

## 3. Design Principles
| Principle | Implementation Meaning |
|---|---|
| Durability before convenience | Accepted requests are recorded in durable storage before execution decisions. |
| No mystery states | State transitions are explicit, timestamped, and queryable. |
| Core first, adapters second | Adapters execute domain work; core owns lifecycle truth and classification. |
| Truth before notification | Outbound callbacks happen only after durable outcome commit. |
| Operator-first diagnostics | Replay, failure classification, callback attempt history, and receipts are first-class. |

## 4. Architecture Overview
### 4.1 Components
| Layer | Responsibility | Must Not Do | Primary Output |
|---|---|---|---|
| Reverse Proxy | Edge routing, filtering, public surface control | Business execution logic | Routed HTTP traffic to ingress/status |
| Ingress API | AuthN/AuthZ, schema validation, intent normalization | Long-running execution | Durable normalized intent submission |
| PostgresQ + Core tables | Durable queueing, leasing, scheduling, lifecycle persistence | Domain-specific business execution | Queryable durable state and attempts |
| Execution Core | Lifecycle policy, adapter routing, classification, retry decisions | Chain-specific semantics | Canonical transitions, receipts, retry/replay decisions |
| Adapter Runtime | Domain execution using normalized intent | Invent platform truth | Structured normalized adapter outcomes |
| Callback Core | Outbound delivery with retry + attempt history | Decide execution success | Delivery records and callback status |
| Status API | Tenant-scoped read/query + authorized replay command | Mutate truth outside core policy | Request/job/receipt/history/audit views |
| Operator UI | Human dashboard for status, replay, audit views | Direct DB writes | Operator workflows via status surface |

### 4.2 Current Implemented Services
| Service | Path |
|---|---|
| Ingress API | `apps/ingress_api` |
| Execution Worker Runtime | `apps/admin_cli` |
| Operator UI | `apps/operator_ui` |
| Execution Core | `crates/execution_core` |
| Adapter Contract | `crates/adapter_contract` |
| Solana Adapter | `crates/adapter_solana` |
| RPC Layer | `crates/rpc_layer` |
| Callback Core | `crates/callback_core` |
| Status API | `crates/status_api` |
| Reverse Proxy | `crates/reverse-proxy` |
| Shared Auth Utilities | `crates/auth` |

### 4.3 Cross-Chain Provider Strategy
Azums treats provider routing as an adapter-local transport concern, not a core lifecycle concern.

That contract is now explicit:

- Core lifecycle and receipt schema stay chain-agnostic.
- Adapters may use ordered provider failover underneath without changing state semantics.
- Production provider order should be:
  1. managed/external RPC
  2. self-hosted fallback RPC
- Every chain adapter should preserve these receipt/attempt fields:
  - `provider_used`
  - `rpc_url`
  - `rpc_urls`
  - `signing_mode`
  - `payer_source`
  - `fee_payer`
  - chain tx reference (`signature`, `tx_hash`, or `tx_digest`)

Shared env convention for all chain adapters:

- `<CHAIN>_RPC_PRIMARY_URL`
- `<CHAIN>_RPC_FALLBACK_URLS`
- `<CHAIN>_RPC_URLS`
- `<CHAIN>_RPC_URL`

Current implementation:

- Solana: implemented
- EVM: env/template and architectural pattern defined; adapter not yet implemented
- Sui: env/template and architectural pattern defined; adapter not yet implemented

### 4.4 Downstream Reconciliation and Exception Intelligence
Azums will add reconciliation and exception intelligence as downstream bounded subsystems.

The governing rules are:

1. execution truth remains owned by Azums core
2. reconciliation consumes truth and does not replace it
3. exception intelligence classifies divergence and does not silently mutate history
4. framework-level contracts remain adapter-neutral, with Solana-first rule packs

Reference documents:

- `docs/roadmaps/reconciliation-and-exception-roadmap.md`
- `docs/architecture/reconciliation-integration-spec.md`
- `docs/architecture/exception-intelligence-index-v1.md`
- `docs/contracts/reconciliation-contract-v1.md`
- `docs/contracts/adapter-integration-playbook.md`
- `docs/contracts/recon-rule-pack-template.md`
- `docs/contracts/exception-taxonomy-v1.md`
- `docs/contracts/exception-subcode-guidance.md`
- `docs/contracts/solana-reconciliation-rule-pack-v1.md`
- `docs/contracts/future-adapters/README.md`
- `docs/receipts/reconciliation-upgrade.md`
- `docs/adrs/ADR-0001-reconciliation-and-exception-intelligence-downstream-bounded-subsystems.md`
- `docs/adrs/ADR-0002-generic-reconciliation-framework-with-adapter-specific-rule-packs.md`
- `docs/adrs/ADR-0003-adapter-conformance-across-execution-and-reconciliation.md`

Milestone 2 implementation status:

- execution receipts now carry stable reconciliation-facing references without embedding recon matching semantics
- downstream intake signals are emitted into `platform_recon_intake_signals`
- `recon_core` consumes those signals while execution truth remains owned by `execution_core`

Milestone 3 implementation status:

- recon intake is materialized through a dedicated `ReconIntakeService`
- `recon_core_intake_events` provides signal-id dedupe and replay-safe intake lineage
- `recon_core_subjects` stores scheduling metadata and intake-facing expected fact snapshots
- `recon_core_outcomes` persists durable run-level outcome summaries
- recon scheduling remains adjacent to queue machinery via durable subject scheduling state, not execution-core queue semantics

Milestone 4 implementation status:

- `ReconEngine` is now the adapter-neutral reconciliation processor boundary
- recon processor lifecycle is stored separately from execution lifecycle in `recon_core_run_state_transitions`
- recon retries are isolated from execution retries through subject-side retry metadata and backoff scheduling
- each recon run now emits a durable evidence snapshot into `recon_core_evidence_snapshots`
- rule-pack resolution remains adapter-neutral at the framework layer while Solana stays the first concrete rule pack

Milestone 5 implementation status:

- the first real adapter-aware rule pack is now `SolanaReconRulePack`
- expected facts now include Solana source, destination, asset, amount, program, action, execution reference, and finality expectations
- observed facts now resolve from durable Solana adapter evidence plus callback delivery state
- explicit Solana mismatch subcodes are emitted for operator-facing reconciliation truth
- fixture-backed Solana recon scenarios now verify match, stale, duplicate-signal, amount mismatch, and observed-error divergence behavior
- a self-contained Solana recon benchmark example is available for launch-path performance checks

Milestone 6 implementation status:

- exception intelligence now persists durable operator-facing cases in `exception_cases`
- evidence is linked back to execution receipts, recon outcomes, recon receipts, and observed fact snapshots
- case events and resolution history are stored separately for append-friendly operator workflow tracking
- a searchable exception index is now exposed through `status_api`
- operator state transitions are explicit and validated without mutating execution truth

Milestone 10 implementation status:

- a future adapter conformance kit now exists across execution and reconciliation
- the playbook points future adapter owners at the existing `DomainAdapter` and `ReconRulePack` interfaces
- a reusable recon rule-pack template and exception-subcode guidance are now documented
- mapping documents now exist for EVM, Sui, HTTP, Slack, Email, Stripe, Paystack, and Flutterwave
- adding future adapters is now a product decision, not an architectural rewrite

### 4.5 Agent and AI Entry Surfaces
Azums may expose AI, agent, Slack, approval, and UI-assisted product surfaces.

Those surfaces are entry points, not alternate execution cores.

The governing rules are:

1. AI and agent paths converge into the same normalized intent model used by API and webhook traffic
2. only Azums execution workers may trigger protected adapter execution
3. approvals, Slack actions, UI actions, and agent messages do not directly mutate final execution truth
4. API and agent-originated work share the same receipt, replay, reconciliation, and exception surfaces

Reference document:

- `docs/adrs/ADR-0004-agent-and-api-entry-surfaces-converge-into-one-execution-path.md`

## 5. Canonical Lifecycle
| State | Meaning |
|---|---|
| `received` | Ingress accepted raw request boundary and is processing intake. |
| `validated` | Request passed platform validation/contract checks. |
| `rejected` | Intake rejected due to auth/schema/policy violations. |
| `queued` | Durable execution work is ready for workers. |
| `leased` | A worker has leased the next attempt. |
| `executing` | Core dispatched adapter execution. |
| `retry_scheduled` | Retryable failure classified and scheduled. |
| `succeeded` | Execution successfully completed for platform semantics. |
| `failed_terminal` | Terminal failure; no automatic retry. |
| `dead_lettered` | Retry policy exhausted; terminal inspection path. |
| `replayed` | A replay lineage path has been created. |

## 6. Platform Contract
### 6.1 Normalized Intent (representative fields)
| Field | Purpose |
|---|---|
| `request_id` | Stable intake identity |
| `intent_id` | Internal execution identity |
| `tenant_id` | Isolation boundary |
| `kind` / `intent_kind` | Action requested |
| `payload` | Adapter-specific versioned body |
| `correlation_id` | Cross-service tracing |
| `idempotency_key` | Duplicate protection |
| `auth_context` | Submitter identity + auth scheme + channel |
| `metadata` | Additional machine-readable context |

### 6.2 Normalized Outcome (representative fields)
| Field | Purpose |
|---|---|
| `intent_id` / `job_id` | Links outcome to attempt lineage |
| `adapter_id` | Executor identity |
| `state` | Canonical lifecycle state |
| `classification` | Platform-level result class |
| `retryable` | Retry policy input |
| `machine_reason` | Structured reason code |
| `human_message` | Operator-readable explanation |
| `adapter_metadata` | Domain-specific evidence |

## 7. Flow Definitions
### 7.1 Flow A: Inbound Execution
| Step | Action | Durable Outcome |
|---|---|---|
| 1 | Client sends API/webhook | Request reaches proxy/ingress boundary |
| 2 | Ingress authenticates, validates, normalizes | Intake audit + normalized intent persisted |
| 3 | Core/worker leases queued work | Lease/attempt recorded |
| 4 | Core dispatches adapter | Executing transition recorded |
| 5 | Adapter executes and returns structured result | Adapter outcome normalized |
| 6 | Core classifies + persists transition/receipt | Canonical truth committed |
| 7 | Callback core delivers outward updates | Delivery attempts/history recorded |
| 8 | Status API / UI queries durable read model | Full journey visible |

### 7.2 Flow B: Retry
| Step | Action |
|---|---|
| 1 | Adapter returns retryable failure |
| 2 | Core classifies failure as retryable |
| 3 | Retry schedule persisted (next attempt time) |
| 4 | Worker re-leases job later |
| 5 | Core re-dispatches adapter |
| 6 | Receipt/history shows multiple attempts |

### 7.3 Flow C: Terminal Failure
| Step | Action |
|---|---|
| 1 | Adapter returns terminal failure |
| 2 | Core marks `failed_terminal` |
| 3 | Receipt records stage + reason + remediation posture |
| 4 | Callback core may notify client |
| 5 | UI shows durable classified failure |

### 7.4 Flow D: Replay
| Step | Action |
|---|---|
| 1 | Authorized user/operator requests replay |
| 2 | Status API enforces permission |
| 3 | Core validates replay eligibility |
| 4 | Replay lineage record created |
| 5 | New execution path scheduled |
| 6 | Lineage preserved across old and replayed attempts |

## 8. Security Model
### 8.1 Mandatory Controls
| Control | Status in Platform |
|---|---|
| Tenant isolation | Enforced across ingress/status/callback and store queries |
| Authenticated submit/query | Header + token + principal binding model |
| Replay authorization | Restricted by role and audited |
| Callback destination controls | Admin-gated configuration with validation |
| Sensitive output handling | Redaction controls by role in status views |
| Durable audit trails | Query audit, operator action audit, ingress intake audits |

### 8.2 Authorization Questions Addressed
| Question | Current Rule Pattern |
|---|---|
| Who can submit? | Authenticated principal with allowed submitter kind and tenant binding |
| Who can query? | Authenticated principal with tenant visibility + role controls |
| Who can replay? | Admin-authorized by policy (status + core path) |
| Who can view sensitive details? | Role-based redaction in status responses |
| Who can trigger adapter execution? | Internal workers/core only |
| Who can manage callback URLs? | Tenant admin paths only |

## 9. API Surface Summary
### 9.1 Ingress (write path)
| Endpoint | Purpose |
|---|---|
| `POST /api/requests` | Submit normalized supported intent |
| `POST /webhooks/...` | Webhook intake (configured channel rules) |

### 9.2 Status API (read and controlled actions)
| Endpoint | Purpose |
|---|---|
| `GET /requests/:id` | Current request summary/status |
| `GET /requests/:id/receipt` | Receipt/timeline entries |
| `GET /requests/:id/history` | State transition history |
| `GET /requests/:id/callbacks` | Callback delivery history |
| `GET /jobs` | Job list/filtering |
| `POST /requests/:id/replay` | Authorized replay |
| `GET /tenant/intake-audits` | Ingress intake audit history |
| `GET/POST/DELETE /tenant/callback-destination` | Tenant callback destination config |

### 9.3 Operator UI
| Capability | Backing Status API |
|---|---|
| Job table/filter | `GET /jobs` |
| Request inspector | `GET /requests/:id`, `/receipt`, `/history`, `/callbacks` |
| Replay trigger | `POST /requests/:id/replay` |
| Intake audit explorer | `GET /tenant/intake-audits` |
| Callback destination manager | `GET/POST/DELETE /tenant/callback-destination` |

## 10. Operations and Runbook
### 10.1 Standard Operator Actions
| Action | Path |
|---|---|
| Diagnose failed request | Inspect request + receipt + history + callbacks |
| Identify retry loops | Filter jobs and inspect repeated `retry_scheduled` transitions |
| Validate callback failures separately | Inspect callback history without mutating execution truth |
| Replay safe candidates | Use authorized replay endpoint with reason code |
| Track intake issues | Query tenant intake audits by `validation_result`/`channel` |

### 10.2 Deployment
Local full stack uses `deployments/compose`:

1. `cd deployments/compose`
2. `cp .env.example .env`
3. `docker compose up`

Default ports:

- Reverse proxy: `8000`
- Ingress API: `8081`
- Status API: `8082`
- Operator UI: `8083`

## 11. Competitive Positioning (Top 3 Reference Tools)
Selected comparison set:

1. Temporal
2. AWS Step Functions
3. Hookdeck

These are strong tools with different primary scopes. The table below focuses on this platform’s target problem: durable, explainable execution with adapter uniformity and replay-safe lineage in a self-hostable architecture.

### 11.1 Capability Comparison
| Capability | Azums | Temporal | AWS Step Functions | Hookdeck |
|---|---|---|---|---|
| Unified execution + callback truth separation | Yes (explicit execution truth vs delivery truth boundary) | Partial (workflow-centric, callback separation is custom) | Partial (state machine-centric, custom delivery separation) | No (delivery-focused, not full execution core) |
| Canonical adapter contract across domains | Yes (core-owned lifecycle + adapter normalization) | Custom implementation required | Custom implementation required | Not primary design target |
| Durable receipt/timeline as first-class product object | Yes | Possible, but typically app-defined | Possible, but typically app-defined | Delivery/event history focused |
| Replay lineage linked to original attempts | Yes (explicit replay/redrive lineage model) | Yes, but workflow semantics differ | Limited without additional lineage modeling | Not execution replay-focused |
| Strict “truth before notify” pattern across stack | Yes (core commit before delivery) | Depends on workflow implementation | Depends on orchestration design | Delivery engine focus, not execution truth engine |
| Self-hostable full stack control | Yes | Yes (OSS), plus managed offerings | No (managed AWS service) | SaaS-first |
| Multi-domain adapters beyond chain-specific use | First-class architecture principle | Requires workflow/activity modeling | Requires state machine/task integration | Not orchestration-first |

### 11.2 Where Azums Is Stronger for This Use Case
| Scenario | Why Azums is Better Positioned |
|---|---|
| Teams needing one durable execution foundation across heterogeneous adapters | Core semantics stay stable while adapters evolve. |
| Teams that need explainability as a product requirement, not a custom add-on | Receipt, transition, audit, and callback history are native concepts. |
| Teams requiring strict separation of execution success from callback delivery success | Separate lifecycle handling prevents false “success” narratives. |
| Teams that prioritize self-hosted governance and tenant-isolated auditability | End-to-end model is built for explicit boundaries and internal policy control. |

### 11.3 Fair Tradeoff Statement
Azums is strongest when organizations want explicit ownership of lifecycle semantics, replay policy, and operator-grade diagnostics in a self-hosted model.  
If a team needs purely managed cloud orchestration with minimal operational ownership, a managed service may be simpler.

## 12. Current Scope and Known Gaps
| Area | Current State |
|---|---|
| Kubernetes deployment assets | Placeholder (`deployments/k8s`) |
| Generic adapter catalog breadth | Solana adapter is primary implemented adapter |
| Production hardening artifacts | Ongoing across deployment and observability layers |

## 13. Documentation Map
| Document | Purpose |
|---|---|
| `README.md` | Quick start and top-level architecture |
| `apps/ingress_api/README.md` | Ingress behavior and env contract |
| `crates/execution_core/README.md` | Core lifecycle and policy behavior |
| `crates/status_api/README.md` | Query and replay API surface |
| `apps/operator_ui/README.md` | Operator dashboard setup and env |
| `deployments/compose/README.md` | Local end-to-end stack instructions |

## 14. Glossary
| Term | Meaning |
|---|---|
| Intent | Normalized request command accepted by platform |
| Adapter | Domain-specific executor implementing contract |
| Receipt | Durable explainable timeline of execution lifecycle |
| Classification | Platform-level failure/success category |
| Replay | Authorized redrive path with lineage preservation |
| Durable truth | Canonical persisted state in core/store, not UI/log inference |
