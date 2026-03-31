# ADR-0003: Adapter Conformance Across Execution and Reconciliation

## Status

Accepted

## Context

Azums now has:

- a generic execution adapter contract
- a generic reconciliation rule-pack contract
- a generic exception taxonomy

Without a conformance kit, each future adapter would still become a one-off implementation with drift in:

- expected fact shape
- observed fact quality
- mismatch naming
- evidence mapping
- operator explainability

## Decision

Every future adapter must be specified through one conformance kit before implementation is considered production-ready.

That kit includes:

- execution adapter mapping
- reconciliation rule-pack mapping
- expected fact mapping
- observed fact resolver mapping
- mismatch subcode mapping
- evidence schema mapping
- fixture and benchmark plan

The framework remains generic:

- execution lifecycle stays core-owned
- recon outcomes stay framework-owned
- exception top-level categories stay framework-owned

Adapter-specific nuance belongs in:

- adapter-local execution logic
- adapter-local evidence
- adapter-specific subcodes
- operator-facing details

## Consequences

### Positive

- future adapters become product choices, not rewrites
- operator UX stays coherent across adapters
- recon and exception layers remain generic

### Negative

- adapter owners must do more upfront specification work
- some domain nuance will remain in subcodes/details instead of top-level fields

## Non-Goals

- implementing every future adapter now
- forcing every adapter to ship reconciliation on day one
- allowing future adapters to redefine framework-level outcomes or categories
