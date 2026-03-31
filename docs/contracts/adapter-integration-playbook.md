# Adapter Integration Playbook

## Purpose

Make future adapter integration procedural instead of bespoke.

This playbook applies to any future adapter that wants to plug into:

- execution via `DomainAdapter`
- reconciliation via `ReconRulePack`
- exception intelligence via adapter-specific subcodes mapped into the generic taxonomy

It does **not** require every future adapter to implement reconciliation immediately. It defines the shape required when the product chooses to support it.

## Real Integration Points

### Execution

Execution adapters plug into:

- `crates/adapter_contract/src/lib.rs`
- `DomainAdapter`
- `AdapterRegistry`
- `DomainAdapterExecutor`

Required execution methods already exist:

- `validate(...)`
- `execute(...)`
- `resume(...)`
- `fetch_status(...)`

Execution adapters should be specified against the real data shapes already in the framework:

- `AdapterExecutionRequest`
- `AdapterExecutionContext`
- `AdapterResumeContext`
- `AdapterExecutionEnvelope`
- `AdapterStatusHandle`
- `AdapterStatusSnapshot`

### Reconciliation

Reconciliation rule packs plug into:

- `crates/recon_core/src/rules.rs`
- `ReconRulePack`
- `ReconRuleRegistry`

Required recon stages already exist:

- `build_expected_facts(...)`
- `collect_observed_facts(...)`
- `match_facts(...)`
- `classify(...)`
- `emit_recon_result(...)`

Rule packs should be specified against the real recon data shapes:

- `ReconSubject`
- `ReconContext`
- `ExpectedFactDraft`
- `ObservedFactDraft`
- `ReconMatchResult`
- `ReconClassification`
- `ReconEmission`

## Non-Negotiable Rules

1. Execution truth stays in Azums core.
2. Adapters do domain execution, not lifecycle ownership.
3. Reconciliation consumes durable truth; it does not replace it.
4. Exception intelligence classifies divergence; it does not mutate execution history.
5. Framework contracts stay adapter-neutral even when adapter-specific details are rich.

## Conformance Checklist

### A. Execution Adapter Contract

- [ ] Choose a stable `adapter_id`.
- [ ] Define the supported normalized `intent_kind` values.
- [ ] Register routing in `AdapterRegistry`.
- [ ] Implement `validate(...)` with adapter-local payload checks only.
- [ ] Implement `execute(...)` and return an `AdapterExecutionEnvelope` with a valid `AdapterStatusSnapshot` plus `AdapterOutcome`.
- [ ] Implement `resume(...)` only if retry/resume semantics differ from `execute(...)`.
- [ ] Implement `fetch_status(...)` if the adapter has asynchronous or externally-finalized execution.
- [ ] Return stable provider references in `AdapterStatusSnapshot` where available.
- [ ] Keep provider-specific details in `details`, not in framework enums.
- [ ] Do not redefine canonical execution states.

### B. Expected Fact Mapping

- [ ] Define the minimal facts Azums expects after successful execution.
- [ ] Separate strict facts from advisory facts.
- [ ] Record which execution receipt fields or adapter-local tables those facts derive from.
- [ ] Version the expected-fact mapping.

Typical fact families:

- source
- destination
- asset
- amount
- action or method
- provider/execution reference
- timing/finality expectation

### C. Observed Fact Resolver

- [ ] Name the durable evidence source.
- [ ] Define the observation freshness window.
- [ ] Normalize external/provider fields into adapter facts.
- [ ] Avoid letting raw provider payloads become the only operator evidence.
- [ ] Emit observation source references for explainability.

### D. Recon Rule Pack Contract

- [ ] Implement a `ReconRulePack` for the adapter.
- [ ] Keep adapter logic inside the rule pack, not in `ReconEngine`.
- [ ] Emit only framework-level `ReconOutcome` values:
  - `queued`
  - `collecting_observations`
  - `matching`
  - `matched`
  - `partially_matched`
  - `unmatched`
  - `stale`
  - `manual_review_required`
  - `resolved`
- [ ] Treat `pending_observation` as a read-model normalization only, not a rule-pack outcome.
- [ ] Keep adapter-specific nuance in subcodes, details, and evidence.
- [ ] Write durable evidence snapshots for each run.

### E. Mismatch Taxonomy Mapping

- [ ] Define adapter-specific mismatch subcodes in `snake_case`.
- [ ] Map each subcode to a generic exception category.
- [ ] Assign a severity baseline for each subcode.
- [ ] Define whether the default operator path is:
  - acknowledge
  - investigate
  - resolve
  - false_positive
  - replay_review

### F. Evidence Schema Mapping

- [ ] List the adapter-local durable evidence tables or records.
- [ ] Define which identifiers become `source_table` and `source_id`.
- [ ] Define which payloads are safe to show to operators.
- [ ] Redact or avoid secrets, tokens, signing material, and sensitive customer data.

### G. Tests and Benchmarks

- [ ] Fixture-backed adapter execution tests exist.
- [ ] Fixture-backed recon rule-pack tests exist.
- [ ] Common mismatch scenarios have explicit coverage.
- [ ] A benchmark path exists for the adapter recon path.
- [ ] Query/read performance impact is understood.

## Required Adapter Deliverable Set

Every new adapter proposal should produce these artifacts before implementation is considered ready for review:

1. execution adapter mapping
2. recon rule-pack mapping
3. expected fact map
4. observed fact resolver map
5. mismatch subcode map
6. evidence schema map
7. fixture test plan
8. benchmark note
9. security and redaction note

For this repo, the mapping docs live under:

- `docs/contracts/future-adapters/`

Each adapter document should answer the same questions:

1. Which normalized intents does this adapter support?
2. What stable execution reference does it produce?
3. Which facts are expected from durable execution truth?
4. Which facts are observed from durable evidence or provider lookups?
5. Which mismatches matter operationally?
6. Which evidence is safe to show to operators?
7. Which parts are intentionally left out of the first version?

## What a Future Adapter Owner Must Ship

Before an adapter is considered ready for production execution:

- execution adapter implementation
- routing registration
- contract doc update
- adapter-local evidence plan

Before an adapter is considered ready for reconciliation-backed confidence:

- recon rule pack
- expected/observed fact mapping
- mismatch subcodes + category mapping
- evidence mapping
- fixture tests
- benchmark note

## What Must Stay Generic

The following must not become adapter-specific at framework level:

- canonical execution lifecycle states
- reconciliation top-level outcomes
- exception top-level categories
- status API top-level shapes
- unified receipt/dashboard vocabulary

## Product Decision Boundary

Adding a future adapter should be a product decision, not an architectural rewrite.

This playbook is complete when an adapter owner can answer:

1. How does execution plug in?
2. How does reconciliation plug in?
3. What durable evidence exists?
4. How are mismatches classified?
5. What remains generic?
