# System Overview

## Purpose
Azums is a durable execution platform that accepts supported intents and guarantees a durable, queryable outcome trail.

## Service Topology
| Layer | Service | Role |
|---|---|---|
| Edge | `crates/reverse-proxy` | Public ingress routing, edge controls, path ACL |
| Intake | `apps/ingress_api` | Authentication, validation, normalization, durable submit |
| Core | `crates/execution_core` | Lifecycle ownership, routing, classification, retry/replay policy |
| Adapter | `crates/adapter_solana` | Domain execution for Solana intents |
| Provider abstraction | `crates/rpc_layer` | RPC request/response normalization and resilience |
| Delivery | `crates/callback_core` | Outbound callback delivery and attempt tracking |
| Read/query | `crates/status_api` | Status, history, receipt, callback, replay endpoints |
| UI | `apps/operator_ui` | Operator dashboard for querying and controlled actions |
| Worker runtime | `apps/admin_cli` | Execution dispatch and callback worker loops |
| Shared auth library | `crates/auth` | Shared auth parsing and binding helpers |
| Shared observability library | `crates/observability` | Shared tracing, correlation, and HTTP metric helpers |

## Source of Truth
| Artifact | Source |
|---|---|
| Current request/job state | Durable core store tables |
| Transition history | `execution_core_state_transitions` |
| Receipt timeline | `execution_core_receipts` |
| Callback status | `callback_core_deliveries` + `callback_core_delivery_attempts` |
| Intake audit trail | `ingress_api_intake_audits` |

## Core Guarantees
| Guarantee | Explanation |
|---|---|
| Durable acceptance | Accepted intents are persisted before execution workflow |
| Canonical lifecycle | Core-owned state machine is adapter-agnostic |
| Deterministic classification | Adapter outcomes are normalized into platform classes |
| Truth before notify | Final external delivery only after durable truth is written |
| Replay-safe lineage | Replay creates linked lineage rather than mutating history |
