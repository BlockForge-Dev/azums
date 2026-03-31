# Recon Rule Pack Template

## Purpose

Use this template when adding reconciliation support for a new adapter.

Keep the framework generic. Put adapter-specific logic in the rule pack only.

## Required Rust Shape

```rust
use async_trait::async_trait;
use recon_core::{
    ExpectedFactDraft, ObservedFactDraft, ReconClassification, ReconContext, ReconEmission,
    ReconError, ReconMatchResult, ReconOutcome, ReconRulePack, ReconSubject,
};

pub struct ExampleReconRulePack;

#[async_trait]
impl ReconRulePack for ExampleReconRulePack {
    fn adapter_id(&self) -> &'static str {
        "example_adapter"
    }

    fn rule_pack_id(&self) -> &'static str {
        "example_adapter.v1"
    }

    async fn build_expected_facts(
        &self,
        subject: &ReconSubject,
        context: &ReconContext,
    ) -> Result<Vec<ExpectedFactDraft>, ReconError> {
        let _ = (subject, context);
        Ok(vec![])
    }

    async fn collect_observed_facts(
        &self,
        subject: &ReconSubject,
        context: &ReconContext,
        adapter_rows: &[serde_json::Value],
    ) -> Result<Vec<ObservedFactDraft>, ReconError> {
        let _ = (subject, context, adapter_rows);
        Ok(vec![])
    }

    fn match_facts(
        &self,
        subject: &ReconSubject,
        expected: &[ExpectedFactDraft],
        observed: &[ObservedFactDraft],
    ) -> ReconMatchResult {
        let _ = (subject, expected, observed);
        ReconMatchResult::default()
    }

    fn classify(
        &self,
        subject: &ReconSubject,
        context: &ReconContext,
        expected: &[ExpectedFactDraft],
        observed: &[ObservedFactDraft],
        matched: &ReconMatchResult,
    ) -> ReconClassification {
        let _ = (subject, context, expected, observed, matched);
        ReconClassification {
            outcome: Some(ReconOutcome::Matched),
            summary: Some("replace with adapter-specific summary".to_owned()),
            machine_reason: Some("replace_with_adapter_subcode".to_owned()),
            details: std::collections::BTreeMap::new(),
            exceptions: Vec::new(),
        }
    }

    fn emit_recon_result(
        &self,
        subject: &ReconSubject,
        matched: &ReconMatchResult,
        classification: ReconClassification,
    ) -> ReconEmission {
        let _ = subject;
        ReconRulePack::emit_recon_result(self, subject, matched, classification)
    }
}
```

## Outcome Guidance

The rule pack emits framework-level `ReconOutcome`.

Use these outcomes intentionally:

| Outcome | Use when |
|---|---|
| `queued` | subject exists but no meaningful collection should happen yet |
| `collecting_observations` | expected facts exist but required observations are still missing |
| `matching` | enough data exists to compare, but convergence is still legitimately in progress |
| `matched` | expected and observed facts materially agree |
| `partially_matched` | some facts match but there is material divergence |
| `unmatched` | material contradiction exists |
| `stale` | observations are too old or incomplete to trust |
| `manual_review_required` | automation cannot safely close the case |
| `resolved` | a later run or operator action has closed prior divergence |

Important:

- `pending_observation` is a normalized read-model result, not a framework outcome to emit from rule packs.
- `ReconResult::PendingObservation` is derived later from `queued`, `collecting_observations`, or `matching`.

## Required Mapping Sections

Fill in all of these before implementation is considered complete.

### 1. Expected Facts

| Fact key | Strict or advisory | Derived from | Notes |
|---|---|---|---|
|  |  |  |  |

### 2. Observed Facts

| Fact key | Source table / API | Source ID | Freshness rule | Notes |
|---|---|---|---|---|
|  |  |  |  |  |

### 3. Match Logic

For each fact family, specify:

- exact match rule
- acceptable normalization
- partial-match rule
- stale rule
- manual-review rule

### 4. Mismatch Subcodes

| Subcode | Generic category | Baseline severity | Default operator path |
|---|---|---|---|
|  |  |  |  |

### 5. Evidence Snapshot

Minimum evidence to emit per run:

- adapter-local context
- expected fact snapshot
- observed fact snapshot
- match result
- exception candidates

### 6. Operator Explainability

Document:

- one-line operator summary style
- machine-readable subcode style
- which mismatches should open exceptions automatically
- which mismatches default to review versus `replay_review`

### 7. Test and Benchmark Plan

Document:

- happy-path fixture
- missing-observation fixture
- partial-match fixture
- hard mismatch fixture
- stale/pending timing fixture
- benchmark scenario and acceptable baseline

## Guardrails

- Do not introduce new top-level recon outcomes.
- Do not leak raw provider terms into framework enums.
- Do not mutate execution truth.
- Do not let one adapter redefine another adapter’s semantics.
