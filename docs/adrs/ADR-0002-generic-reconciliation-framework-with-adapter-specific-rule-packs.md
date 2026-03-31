# ADR-0002: Generic Reconciliation Framework with Adapter-Specific Rule Packs

## Status

Accepted

## Context

Azums is Solana-first today, but the platform is explicitly adapter-based and must remain extensible. Reconciliation must therefore avoid becoming Solana-shaped at the framework level.

## Decision

Azums will use:

- a generic reconciliation framework contract
- adapter-specific rule packs that implement framework interfaces

Framework-level concepts remain generic:

- reconciliation subject
- expected facts
- observed facts
- reconciliation outcome
- reconciliation receipt
- exception category
- severity

Solana-specific details remain inside Solana rule packs and evidence collectors.

## Consequences

### Positive

- future adapters can adopt the same framework without redesign
- exception taxonomy remains comparable across adapters
- shared UI/read models can present recon status consistently

### Negative

- initial Solana implementation must translate into generic vocabulary
- some adapter-specific nuance will live in rule-pack details rather than top-level fields

## Non-Goals

- exposing Solana-specific states as framework-level reconciliation outcomes
- allowing adapters to create their own incompatible exception taxonomies
