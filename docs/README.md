# Documentation Index

This folder contains the full technical documentation set for the Azums durable execution platform.

## Document Map
| Path | Topic | Audience |
|---|---|---|
| `docs/PLATFORM_DOCUMENTATION.md` | Full product + technical master document | Engineering, architecture, technical buyers |
| `docs/architecture/system-overview.md` | System architecture and service map | Engineering, platform architects |
| `docs/architecture/component-boundaries.md` | Strict ownership boundaries by layer | Engineering, platform architects |
| `docs/architecture/data-flow.md` | Inbound, retry, terminal-failure, replay flows | Engineering, operations |
| `docs/contracts/platform-contract.md` | Normalized intent/outcome contract | Ingress, core, adapter developers |
| `docs/contracts/lifecycle-state-machine.md` | Canonical lifecycle and transition rules | Core and status API developers |
| `docs/contracts/adapter-contract.md` | Adapter integration contract and constraints | Adapter developers |
| `docs/receipts/receipt-schema.md` | Receipt/timeline data model | Core, status API, UI developers |
| `docs/receipts/receipt-examples.md` | Receipt examples for common execution outcomes | Operators, support, engineering |
| `docs/runbooks/operator-runbook.md` | Day-to-day operational procedures | Operators, support |
| `docs/runbooks/replay-runbook.md` | Replay authorization and safe replay steps | Operators, on-call |
| `docs/runbooks/incident-runbook.md` | Incident response and escalation guidance | On-call, platform team |
| `crates/config/README.md` | Runtime env config pack and service templates | Operators, platform engineers |
| `crates/observability/README.md` | Shared logging, correlation, and metrics utilities | Platform and service engineers |
| `deployments/docker/README.md` | Image build and image-based compose deployment | Platform engineers |
| `deployments/k8s/README.md` | Kubernetes baseline manifests and rollout flow | Platform/SRE engineers |
| `.github/workflows/docker-build.yml` | CI docker build validation for all services | Platform engineers |
| `.github/workflows/docker-publish.yml` | CI image publish pipeline to GHCR | Platform engineers |
| `.github/workflows/k8s-deploy.yml` | CI workflow for applying k8s manifests pinned to SHA images | Platform/SRE engineers |

## Conventions
| Convention | Meaning |
|---|---|
| "Durable truth" | Canonical state persisted by core/store, not inferred from logs/UI |
| "Intent" | Normalized request accepted by ingress and core |
| "Outcome" | Normalized adapter/core execution result classification |
| "Receipt" | Human-readable and machine-readable execution timeline |
| "Replay" | Authorized redrive path with lineage preserved |

## Change Control
| Rule | Requirement |
|---|---|
| Architecture changes | Update `architecture/*` and `PLATFORM_DOCUMENTATION.md` |
| Contract changes | Update `contracts/*` before implementation merge |
| Operator behavior changes | Update `runbooks/*` and UI/API docs in same PR |
