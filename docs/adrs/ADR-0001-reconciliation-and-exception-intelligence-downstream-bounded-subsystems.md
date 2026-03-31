# ADR-0001: Reconciliation and Exception Intelligence as Downstream Bounded Subsystems

## Status

Accepted

## Context

Azums already has a clean execution shape:

- ingress normalizes and durably submits
- execution core owns lifecycle truth
- callback core owns delivery truth
- status/query surfaces read from durable truth

Reconciliation and exception handling are now required, but adding them by injecting logic into execution core would blur the platform boundary that Azums depends on.

## Decision

Reconciliation and exception intelligence are downstream bounded subsystems.

- execution truth remains owned by Azums core
- reconciliation consumes durable execution truth and emits recon truth
- exception intelligence consumes recon and evidence signals and emits exception truth
- neither subsystem silently mutates execution history

## Consequences

### Positive

- preserves current execution-core ownership
- keeps replay, receipts, and status semantics stable
- makes reconciliation re-runnable from durable truth
- keeps exception workflows queryable and auditable

### Negative

- introduces additional durable models and read-side joins
- requires explicit integration specs and watermarks instead of informal hooks

## Non-Goals

- replacing execution lifecycle with reconciliation lifecycle
- using exception cases as hidden execution mutations
- embedding chain-specific recon semantics into framework-level execution code
