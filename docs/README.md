# Documentation Index

This folder contains the full technical documentation set for the Azums durable execution platform.

## Document Map

| Path | Topic | Audience |
|---|---|---|
| `docs/PLATFORM_DOCUMENTATION.md` | Full product + technical master document | Engineering, architecture, technical buyers |
| `docs/production-gap-closure-checklist.md` | Ordered closure log for metering, billing durability, role bootstrap, and docs/security hardening | Engineering, platform leads |
| `docs/v1-proof-checklist.md` | Launch-gate proof checklist and scoring sheet for v1 readiness | Engineering, product, platform leads |
| `docs/architecture/system-overview.md` | System architecture and service map | Engineering, platform architects |
| `docs/architecture/component-boundaries.md` | Strict ownership boundaries by layer | Engineering, platform architects |
| `docs/architecture/data-flow.md` | Inbound, retry, terminal-failure, replay flows | Engineering, operations |
| `docs/architecture/reconciliation-integration-spec.md` | Architecture freeze and exact attachment points for downstream recon/exception subsystems | Engineering, platform architects |
| `docs/architecture/reconciliation-storage-and-intake.md` | Milestone 3 recon storage, intake, idempotency, and scheduling model | Engineering, platform architects |
| `docs/architecture/exception-intelligence-index-v1.md` | Milestone 6 exception storage, evidence index, search surface, and operator workflow | Engineering, operators |
| `docs/contracts/platform-contract.md` | Normalized intent/outcome contract | Ingress, core, adapter developers |
| `docs/contracts/lifecycle-state-machine.md` | Canonical lifecycle and transition rules | Core and status API developers |
| `docs/contracts/adapter-contract.md` | Adapter integration contract and constraints | Adapter developers |
| `docs/contracts/adapter-integration-playbook.md` | Future adapter conformance checklist across execution, recon, mismatch, and evidence | Adapter developers, platform architects |
| `docs/contracts/reconciliation-contract-v1.md` | Adapter-neutral reconciliation contract and rule-pack interface | Platform architects, adapter developers |
| `docs/contracts/recon-rule-pack-template.md` | Reusable template for new adapter reconciliation rule packs | Adapter developers |
| `docs/contracts/exception-taxonomy-v1.md` | Normalized divergence categories, severity, and exception case model | Platform architects, operators |
| `docs/contracts/exception-subcode-guidance.md` | Guidance for adapter-specific mismatch subcodes and generic category mapping | Adapter developers, operators |
| `docs/contracts/future-adapters/README.md` | Example future adapter mappings and required document structure | Adapter developers, platform architects |
| `docs/receipts/receipt-schema.md` | Receipt/timeline data model | Core, status API, UI developers |
| `docs/receipts/receipt-examples.md` | Receipt examples for common execution outcomes | Operators, support, engineering |
| `docs/receipts/reconciliation-upgrade.md` | Milestone 2 receipt upgrade, migration plan, and recon intake signal contract | Core, recon, status API developers |
| `docs/roadmaps/reconciliation-and-exception-roadmap.md` | Delivery philosophy plus Milestone 0 and Milestone 1 for reconciliation/exception work | Engineering, architecture |
| `docs/adrs/ADR-0001-reconciliation-and-exception-intelligence-downstream-bounded-subsystems.md` | Decision to keep recon and exception intelligence downstream | Engineering, architecture |
| `docs/adrs/ADR-0002-generic-reconciliation-framework-with-adapter-specific-rule-packs.md` | Decision to keep the framework adapter-neutral with Solana-first rule packs | Engineering, architecture |
| `docs/adrs/ADR-0003-adapter-conformance-across-execution-and-reconciliation.md` | Decision to require future adapters to conform across execution and reconciliation | Engineering, architecture |
| `docs/adrs/ADR-0004-agent-and-api-entry-surfaces-converge-into-one-execution-path.md` | Decision to keep AI and agent entry surfaces converged into the same normalized-intent and execution-core path | Engineering, architecture |
| `docs/runbooks/operator-runbook.md` | Day-to-day operational procedures | Operators, support |
| `docs/runbooks/benchmark-runbook.md` | Benchmark scenarios, metrics, and interpretation guide | Engineering, SRE, platform leads |
| `docs/runbooks/replay-runbook.md` | Replay authorization and safe replay steps | Operators, on-call |
| `docs/runbooks/incident-runbook.md` | Incident response and escalation guidance | On-call, platform team |
| `docs/runbooks/reconciliation-rollout-runbook.md` | Rollout stages, promotion gates, and commands for recon/exception visibility | Operators, platform leads |
| `docs/runbooks/reconciliation-backfill-runbook.md` | Safe backfill process from execution receipts into recon intake | Operators, platform engineers |
| `docs/runbooks/reconciliation-false-positive-review-log.md` | Weekly review template for false positive analysis | Operators, platform leads |
| `docs/runbooks/reconciliation-launch-readiness-checklist.md` | Launch checklist for customer-visible confidence | Engineering, operators, product |
| `docs/runbooks/reconciliation-benchmark-report-template.md` | Report template for rollout benchmark output | Engineering, SRE |
| `crates/config/README.md` | Runtime env config pack and service templates | Operators, platform engineers |
| `crates/observability/README.md` | Shared logging, correlation, and metrics utilities | Platform and service engineers |
| `deployments/docker/README.md` | Image build and image-based compose deployment | Platform engineers |
| `deployments/k8s/README.md` | Kubernetes baseline manifests and rollout flow | Platform/SRE engineers |
| `.github/workflows/docker-build.yml` | CI docker build validation for all services | Platform engineers |
| `.github/workflows/docker-publish.yml` | CI image publish pipeline to GHCR | Platform engineers |
| `.github/workflows/k8s-deploy.yml` | CI workflow for applying k8s manifests pinned to SHA images | Platform/SRE engineers |

## Conventions

| Convention             | Meaning                                                                                                 |
| ---------------------- | ------------------------------------------------------------------------------------------------------- |
| "Durable truth"        | Canonical state persisted by core/store, not inferred from logs/UI                                      |
| "Intent"               | Normalized request accepted by ingress and core                                                         |
| "Outcome"              | Normalized adapter/core execution result classification                                                 |
| "Receipt"              | Human-readable and machine-readable execution timeline                                                  |
| "Replay"               | Authorized redrive path with lineage preserved                                                          |
| "Reconciliation truth" | Expected-versus-observed matching result derived downstream from execution truth                        |
| "Exception truth"      | Durable divergence case classification and evidence index derived downstream from execution/recon truth |

## Change Control

| Rule                      | Requirement                                             |
| ------------------------- | ------------------------------------------------------- |
| Architecture changes      | Update `architecture/*` and `PLATFORM_DOCUMENTATION.md` |
| Contract changes          | Update `contracts/*` before implementation merge        |
| Operator behavior changes | Update `runbooks/*` and UI/API docs in same PR          |
