use crate::error::ReconError;
use crate::model::{
    evidence, make_exception, ExpectedFactDraft, ObservedFactDraft, ReconClassification,
    ReconContext, ReconEmission, ReconMatchResult, ReconOutcome, ReconSubject,
};
use async_trait::async_trait;
use exception_intelligence::{ExceptionCategory, ExceptionSeverity, ExceptionState};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[async_trait]
pub trait ReconRulePack: Send + Sync {
    fn adapter_id(&self) -> &'static str;
    fn rule_pack_id(&self) -> &'static str;

    async fn build_expected_facts(
        &self,
        subject: &ReconSubject,
        context: &ReconContext,
    ) -> Result<Vec<ExpectedFactDraft>, ReconError>;

    async fn collect_observed_facts(
        &self,
        subject: &ReconSubject,
        context: &ReconContext,
        adapter_rows: &[Value],
    ) -> Result<Vec<ObservedFactDraft>, ReconError>;

    fn match_facts(
        &self,
        subject: &ReconSubject,
        expected: &[ExpectedFactDraft],
        observed: &[ObservedFactDraft],
    ) -> ReconMatchResult;

    fn classify(
        &self,
        subject: &ReconSubject,
        context: &ReconContext,
        expected: &[ExpectedFactDraft],
        observed: &[ObservedFactDraft],
        matched: &ReconMatchResult,
    ) -> ReconClassification;

    fn emit_recon_result(
        &self,
        subject: &ReconSubject,
        matched: &ReconMatchResult,
        classification: ReconClassification,
    ) -> ReconEmission {
        let outcome = classification
            .outcome
            .unwrap_or(default_outcome(subject, matched));
        let summary = classification
            .summary
            .unwrap_or_else(|| default_summary(subject, &outcome, matched));
        let machine_reason = classification
            .machine_reason
            .unwrap_or_else(|| default_machine_reason(&outcome, matched));
        let mut details = classification.details;
        details
            .entry("rule_pack".to_owned())
            .or_insert_with(|| self.rule_pack_id().to_owned());
        details
            .entry("matched_fact_count".to_owned())
            .or_insert_with(|| matched.matched_fact_keys.len().to_string());
        details
            .entry("missing_expected_count".to_owned())
            .or_insert_with(|| matched.missing_expected.len().to_string());
        details
            .entry("unexpected_observed_count".to_owned())
            .or_insert_with(|| matched.unexpected_observed.len().to_string());
        details
            .entry("mismatch_count".to_owned())
            .or_insert_with(|| matched.mismatches.len().to_string());

        ReconEmission {
            outcome,
            summary,
            machine_reason,
            details,
            exceptions: classification.exceptions,
        }
    }
}

pub struct ReconRuleRegistry {
    packs: HashMap<String, Box<dyn ReconRulePack>>,
}

impl ReconRuleRegistry {
    pub fn new() -> Self {
        Self {
            packs: HashMap::new(),
        }
    }

    pub fn register(&mut self, pack: Box<dyn ReconRulePack>) {
        self.packs.insert(pack.adapter_id().to_owned(), pack);
    }

    pub fn resolve(&self, adapter_id: &str) -> Option<&dyn ReconRulePack> {
        self.packs.get(adapter_id).map(|pack| pack.as_ref())
    }
}

pub struct SolanaReconRulePack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SolanaMismatchSubcode {
    SignatureMissing,
    AmountMismatch,
    DestinationMismatch,
    PendingTooLong,
    DuplicateSignal,
    OnchainStateUnresolved,
    ObservedErrorDiffersFromExpected,
    SourceMismatch,
    ProgramMismatch,
    ActionMismatch,
}

impl SolanaMismatchSubcode {
    fn as_str(self) -> &'static str {
        match self {
            Self::SignatureMissing => "signature_missing",
            Self::AmountMismatch => "amount_mismatch",
            Self::DestinationMismatch => "destination_mismatch",
            Self::PendingTooLong => "pending_too_long",
            Self::DuplicateSignal => "duplicate_signal",
            Self::OnchainStateUnresolved => "onchain_state_unresolved",
            Self::ObservedErrorDiffersFromExpected => "observed_error_differs_from_expected",
            Self::SourceMismatch => "source_mismatch",
            Self::ProgramMismatch => "program_mismatch",
            Self::ActionMismatch => "action_mismatch",
        }
    }
}

struct SolanaObservationResolver;

impl Default for SolanaReconRulePack {
    fn default() -> Self {
        Self
    }
}

impl SolanaObservationResolver {
    fn collect(context: &ReconContext, adapter_rows: &[Value]) -> Vec<ObservedFactDraft> {
        let mut out = Vec::new();
        let primary_row = adapter_rows.first();

        if let Some(row) = primary_row {
            let source_id = row
                .get("attempt_id")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned);
            let observed_at_ms = row.get("updated_at_ms").and_then(as_u64);
            let primary_reference = row
                .get("final_signature")
                .and_then(|value| value.as_str())
                .or_else(|| row.get("signature").and_then(|value| value.as_str()));
            let finality = normalized_solana_finality(row);

            push_observed_fact(
                &mut out,
                "solana",
                "solana.finality",
                json!(finality),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
            );
            if let Some(status) = row
                .get("last_confirmation_status")
                .and_then(|value| value.as_str())
                .or_else(|| row.get("status").and_then(|value| value.as_str()))
            {
                push_observed_fact_with_mode(
                    &mut out,
                    "solana",
                    "solana.confirmation_status",
                    json!(status),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                    "supplemental",
                );
            }
            if let Some(signature) = row.get("signature").and_then(|value| value.as_str()) {
                push_observed_fact(
                    &mut out,
                    "solana",
                    "solana.signature",
                    json!(signature),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                );
            }
            if let Some(reference) = primary_reference {
                push_observed_fact(
                    &mut out,
                    "solana",
                    "solana.execution_reference",
                    json!(reference),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                );
            }
            if let Some(source_addr) = row.get("from_addr").and_then(|value| value.as_str()) {
                push_observed_fact(
                    &mut out,
                    "solana",
                    "solana.source",
                    json!(source_addr),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                );
            }
            if let Some(destination) = row.get("to_addr").and_then(|value| value.as_str()) {
                push_observed_fact(
                    &mut out,
                    "solana",
                    "solana.destination",
                    json!(destination),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                );
            }
            if let Some(amount) = row.get("amount") {
                push_observed_fact(
                    &mut out,
                    "solana",
                    "solana.amount",
                    amount.clone(),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                );
            }
            if let Some(asset) = row.get("asset").and_then(|value| value.as_str()) {
                push_observed_fact(
                    &mut out,
                    "solana",
                    "solana.asset",
                    json!(asset),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                );
            }
            if let Some(program_id) = row.get("program_id").and_then(|value| value.as_str()) {
                push_observed_fact(
                    &mut out,
                    "solana",
                    "solana.program",
                    json!(program_id),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                );
            }
            if let Some(action) = row.get("action").and_then(|value| value.as_str()) {
                push_observed_fact(
                    &mut out,
                    "solana",
                    "solana.action",
                    json!(action),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                );
            }
            if let Some(provider_used) = row.get("provider_used").and_then(|value| value.as_str()) {
                push_observed_fact_with_mode(
                    &mut out,
                    "solana",
                    "solana.provider_used",
                    json!(provider_used),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                    "supplemental",
                );
            }
            if let Some(blockhash_used) = row.get("blockhash_used").and_then(|value| value.as_str()) {
                push_observed_fact_with_mode(
                    &mut out,
                    "solana",
                    "solana.blockhash_used",
                    json!(blockhash_used),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                    "supplemental",
                );
            }
            if let Some(simulation_outcome) = row
                .get("simulation_outcome")
                .and_then(|value| value.as_str())
            {
                push_observed_fact_with_mode(
                    &mut out,
                    "solana",
                    "solana.simulation_outcome",
                    json!(simulation_outcome),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                    "supplemental",
                );
            }
            if let Some(error) = row
                .get("final_err_json")
                .filter(|value| !value.is_null())
                .or_else(|| row.get("last_err_json").filter(|value| !value.is_null()))
            {
                push_observed_fact_with_mode(
                    &mut out,
                    "solana",
                    "solana.observed_error",
                    error.clone(),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                    "supplemental",
                );
            }
        }

        let distinct_signatures: HashSet<String> = adapter_rows
            .iter()
            .filter_map(|row| {
                row.get("final_signature")
                    .and_then(|value| value.as_str())
                    .or_else(|| row.get("signature").and_then(|value| value.as_str()))
                    .map(ToOwned::to_owned)
            })
            .collect();
        if let Some(row) = primary_row {
            let observed_at_ms = row.get("updated_at_ms").and_then(as_u64);
            let metadata = json!({
                "row_count": adapter_rows.len(),
                "distinct_signature_count": distinct_signatures.len(),
            });
            push_observed_fact_with_mode(
                &mut out,
                "solana",
                "solana.distinct_signature_count",
                json!(distinct_signatures.len() as u64),
                row.get("attempt_id").and_then(|value| value.as_str()).map(ToOwned::to_owned),
                observed_at_ms,
                metadata,
                "supplemental",
            );
        }

        if let Some(callback) = context.callback_delivery.as_ref() {
            if let Some(state) = callback.get("state").and_then(|value| value.as_str()) {
                out.push(ObservedFactDraft {
                    fact_type: "delivery".to_owned(),
                    fact_key: "delivery.state".to_owned(),
                    fact_value: json!(state),
                    source_kind: "callback_delivery".to_owned(),
                    source_table: Some("callback_core_deliveries".to_owned()),
                    source_id: callback
                        .get("callback_id")
                        .and_then(|value| value.as_str())
                        .map(ToOwned::to_owned),
                    metadata: callback.clone(),
                    observed_at_ms: callback.get("updated_at_ms").and_then(as_u64),
                });
            }
        }

        out
    }
}

#[async_trait]
impl ReconRulePack for SolanaReconRulePack {
    fn adapter_id(&self) -> &'static str {
        "adapter_solana"
    }

    fn rule_pack_id(&self) -> &'static str {
        "solana.v1"
    }

    async fn build_expected_facts(
        &self,
        subject: &ReconSubject,
        context: &ReconContext,
    ) -> Result<Vec<ExpectedFactDraft>, ReconError> {
        let mut out = vec![
            ExpectedFactDraft {
                fact_type: "execution".to_owned(),
                fact_key: "execution.final_state".to_owned(),
                fact_value: json!(subject.canonical_state),
                derived_from: json!({
                    "source": "execution_core_jobs",
                    "job_id": subject.job_id,
                }),
            },
            ExpectedFactDraft {
                fact_type: "execution".to_owned(),
                fact_key: "execution.classification".to_owned(),
                fact_value: json!(subject.platform_classification),
                derived_from: json!({
                    "source": "execution_core_jobs",
                    "job_id": subject.job_id,
                }),
            },
        ];

        let expected_finality = if subject.canonical_state.eq_ignore_ascii_case("Succeeded") {
            "finalized"
        } else if subject.canonical_state.eq_ignore_ascii_case("FailedTerminal")
            || subject.canonical_state.eq_ignore_ascii_case("Rejected")
            || subject.canonical_state.eq_ignore_ascii_case("DeadLettered")
        {
            "not_finalized"
        } else {
            "pending"
        };
        out.push(ExpectedFactDraft {
            fact_type: "solana".to_owned(),
            fact_key: "solana.finality".to_owned(),
            fact_value: json!(expected_finality),
            derived_from: json!({
                "source": "execution_core_receipts",
                "receipt_id": subject.latest_receipt_id,
            }),
        });
        out.push(ExpectedFactDraft {
            fact_type: "solana".to_owned(),
            fact_key: "solana.confirmation_expectation".to_owned(),
            fact_value: json!(expected_finality),
            derived_from: advisory_source("execution_core_receipts", subject.latest_receipt_id.as_deref()),
        });
        out.push(ExpectedFactDraft {
            fact_type: "solana".to_owned(),
            fact_key: "solana.terminal_window_ms".to_owned(),
            fact_value: json!(expected_terminal_window_ms(subject)),
            derived_from: advisory_source("recon_policy", Some(subject.subject_id.as_str())),
        });

        if let Some(intent) = context.intent.as_ref() {
            if let Some(source_addr) = extract_text_from_value(
                intent,
                &["/payload/from_addr", "/payload/from", "/payload/fee_payer", "/payload/payer"],
            ) {
                out.push(ExpectedFactDraft {
                    fact_type: "solana".to_owned(),
                    fact_key: "solana.source".to_owned(),
                    fact_value: json!(source_addr),
                    derived_from: json!({
                        "source": "execution_core_intents",
                        "intent_id": subject.intent_id,
                    }),
                });
            }
            if let Some(to_addr) =
                extract_text_from_value(intent, &["/payload/to_addr", "/payload/to"])
            {
                out.push(ExpectedFactDraft {
                    fact_type: "solana".to_owned(),
                    fact_key: "solana.destination".to_owned(),
                    fact_value: json!(to_addr),
                    derived_from: json!({
                        "source": "execution_core_intents",
                        "intent_id": subject.intent_id,
                    }),
                });
            }
            if let Some(amount) = intent.pointer("/payload/amount").cloned() {
                out.push(ExpectedFactDraft {
                    fact_type: "solana".to_owned(),
                    fact_key: "solana.amount".to_owned(),
                    fact_value: amount,
                    derived_from: json!({
                        "source": "execution_core_intents",
                        "intent_id": subject.intent_id,
                    }),
                });
            }
            let asset = extract_text_from_value(intent, &["/payload/asset", "/payload/mint"])
                .unwrap_or_else(|| "SOL".to_owned());
            out.push(ExpectedFactDraft {
                fact_type: "solana".to_owned(),
                fact_key: "solana.asset".to_owned(),
                fact_value: json!(asset),
                derived_from: json!({
                    "source": "execution_core_intents",
                    "intent_id": subject.intent_id,
                }),
            });
            let program_id =
                extract_text_from_value(intent, &["/payload/program_id", "/payload/program"])
                    .unwrap_or_else(|| "system_program".to_owned());
            out.push(ExpectedFactDraft {
                fact_type: "solana".to_owned(),
                fact_key: "solana.program".to_owned(),
                fact_value: json!(program_id),
                derived_from: json!({
                    "source": "execution_core_intents",
                    "intent_id": subject.intent_id,
                }),
            });
            let action = extract_text_from_value(intent, &["/payload/action", "/payload/type"])
                .unwrap_or_else(|| extract_text_from_value(intent, &["/kind"]).unwrap_or_else(|| "transfer".to_owned()));
            out.push(ExpectedFactDraft {
                fact_type: "solana".to_owned(),
                fact_key: "solana.action".to_owned(),
                fact_value: json!(action),
                derived_from: json!({
                    "source": "execution_core_intents",
                    "intent_id": subject.intent_id,
                }),
            });
        }

        if let Some(reference) = expected_execution_reference(subject, context) {
            out.push(ExpectedFactDraft {
                fact_type: "solana".to_owned(),
                fact_key: "solana.execution_reference".to_owned(),
                fact_value: json!(reference),
                derived_from: json!({
                    "source": "execution_core_receipts",
                    "receipt_id": subject.latest_receipt_id,
                }),
            });
        }

        Ok(out)
    }

    async fn collect_observed_facts(
        &self,
        _subject: &ReconSubject,
        context: &ReconContext,
        adapter_rows: &[Value],
    ) -> Result<Vec<ObservedFactDraft>, ReconError> {
        Ok(SolanaObservationResolver::collect(context, adapter_rows))
    }

    fn match_facts(
        &self,
        _subject: &ReconSubject,
        expected: &[ExpectedFactDraft],
        observed: &[ObservedFactDraft],
    ) -> ReconMatchResult {
        let mut observed_map = HashMap::new();
        for fact in observed {
            observed_map.insert(fact.fact_key.clone(), fact.fact_value.clone());
        }

        let mut result = ReconMatchResult::default();
        for fact in expected {
            if is_advisory_expected_fact(fact) {
                continue;
            }
            match observed_map.get(&fact.fact_key) {
                Some(value) if values_match(&fact.fact_key, &fact.fact_value, value) => {
                    result.matched_fact_keys.push(fact.fact_key.clone());
                }
                Some(value) => result.mismatches.push(crate::model::FactMismatch {
                    fact_key: fact.fact_key.clone(),
                    mismatch_type: mismatch_type_for_fact(&fact.fact_key),
                    expected: Some(fact.fact_value.clone()),
                    observed: Some(value.clone()),
                    message: format!("expected `{}` to match observed value", fact.fact_key),
                }),
                None => result.missing_expected.push(fact.fact_key.clone()),
            }
        }

        for fact in observed {
            if is_supplemental_observed_fact(fact) {
                continue;
            }
            if !expected
                .iter()
                .filter(|expected_fact| !is_advisory_expected_fact(expected_fact))
                .any(|expected_fact| expected_fact.fact_key == fact.fact_key)
            {
                result.unexpected_observed.push(fact.fact_key.clone());
            }
        }

        result
    }

    fn classify(
        &self,
        subject: &ReconSubject,
        _context: &ReconContext,
        expected: &[ExpectedFactDraft],
        observed: &[ObservedFactDraft],
        matched: &ReconMatchResult,
    ) -> ReconClassification {
        let mut classification = ReconClassification::default();
        let delivery_terminal_failure = observed.iter().any(|fact| {
            fact.fact_key == "delivery.state"
                && fact.fact_value.as_str() == Some("terminal_failure")
        });
        let observed_error_present = observed.iter().any(|fact| {
            fact.fact_key == "solana.observed_error" && !fact.fact_value.is_null()
        });
        let distinct_signature_count = observed_fact_u64(observed, "solana.distinct_signature_count")
            .unwrap_or_default();
        let expected_window_ms =
            expected_fact_u64(expected, "solana.terminal_window_ms").unwrap_or(300_000);
        let age_ms = current_ms().saturating_sub(subject.updated_at_ms);
        let expected_reference = expected_fact_value(expected, "solana.execution_reference");
        let observed_reference = observed_fact_value(observed, "solana.execution_reference");
        let expected_success = subject.canonical_state.eq_ignore_ascii_case("Succeeded");
        let missing_finality = matched
            .missing_expected
            .iter()
            .any(|key| key == "solana.finality");
        let reference_mismatch = matched
            .mismatches
            .iter()
            .any(|mismatch| mismatch.fact_key == "solana.execution_reference");
        let observed_finality_unknown = observed
            .iter()
            .any(|fact| fact.fact_key == "solana.finality" && fact.fact_value.as_str() == Some("unknown"));
        let has_finality_mismatch = matched
            .mismatches
            .iter()
            .any(|mismatch| mismatch.fact_key == "solana.finality");
        let is_finalized_match = matched
            .matched_fact_keys
            .iter()
            .any(|key| key == "solana.finality");

        if subject.canonical_state.eq_ignore_ascii_case("Queued")
            || subject.canonical_state.eq_ignore_ascii_case("Received")
            || subject.canonical_state.eq_ignore_ascii_case("Validated")
            || subject.canonical_state.eq_ignore_ascii_case("Leased")
        {
            classification.outcome = Some(ReconOutcome::Queued);
            classification.summary =
                Some("reconciliation subject queued behind execution progression".to_owned());
            classification.machine_reason = Some("execution_not_ready_for_reconciliation".to_owned());
            return classification;
        }

        if distinct_signature_count > 1 {
            apply_subcode(&mut classification, SolanaMismatchSubcode::DuplicateSignal);
            classification.outcome = Some(ReconOutcome::ManualReviewRequired);
            classification.summary =
                Some("multiple Solana signatures were observed for one execution subject".to_owned());
            classification.machine_reason =
                Some(SolanaMismatchSubcode::DuplicateSignal.as_str().to_owned());
            classification.exceptions.push(make_exception(
                ExceptionCategory::DuplicateSignal,
                ExceptionSeverity::High,
                ExceptionState::Open,
                "multiple Solana execution references were observed for one subject",
                SolanaMismatchSubcode::DuplicateSignal.as_str(),
                vec![evidence(
                    "duplicate_signature_set",
                    Some("solana.tx_attempts".to_owned()),
                    Some(subject.intent_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({
                        "distinct_signature_count": distinct_signature_count,
                    }),
                )],
            ));
            return classification;
        }

        if observed.is_empty() && subject.updated_at_ms > 0 {
            classification.outcome = Some(ReconOutcome::CollectingObservations);
            classification.summary = Some("waiting for downstream observations".to_owned());
            apply_subcode(&mut classification, SolanaMismatchSubcode::SignatureMissing);
            classification.machine_reason =
                Some(SolanaMismatchSubcode::SignatureMissing.as_str().to_owned());
            classification.exceptions.push(make_exception(
                ExceptionCategory::ObservationMissing,
                ExceptionSeverity::Warning,
                ExceptionState::Open,
                "no downstream observations have been collected yet",
                SolanaMismatchSubcode::SignatureMissing.as_str(),
                vec![evidence(
                    "subject",
                    Some("recon_core_subjects".to_owned()),
                    Some(subject.subject_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({
                        "subject_id": subject.subject_id,
                        "canonical_state": subject.canonical_state,
                    }),
                )],
            ));
            return classification;
        }

        if expected.is_empty() && observed.is_empty() {
            classification.outcome = Some(ReconOutcome::Queued);
            classification.summary = Some("no reconciliation facts defined for subject".to_owned());
            classification.machine_reason = Some("no_reconciliation_facts".to_owned());
            return classification;
        }

        if expected_reference.is_some() && observed_reference.is_none() {
            apply_subcode(&mut classification, SolanaMismatchSubcode::SignatureMissing);
            if age_ms > expected_window_ms {
                apply_subcode(&mut classification, SolanaMismatchSubcode::PendingTooLong);
                classification.details.insert(
                    "primary_mismatch_subcode".to_owned(),
                    SolanaMismatchSubcode::PendingTooLong.as_str().to_owned(),
                );
                classification.outcome = Some(ReconOutcome::Stale);
                classification.summary =
                    Some("Solana execution reference is still missing beyond the expected window".to_owned());
                classification.machine_reason =
                    Some(SolanaMismatchSubcode::PendingTooLong.as_str().to_owned());
                classification.exceptions.push(make_exception(
                    ExceptionCategory::DelayedFinality,
                    ExceptionSeverity::Warning,
                    ExceptionState::Investigating,
                    "Solana execution reference was not observed within the expected confirmation window",
                    SolanaMismatchSubcode::PendingTooLong.as_str(),
                    vec![evidence(
                        "reconciliation_window",
                        Some("recon_core_subjects".to_owned()),
                        Some(subject.subject_id.clone()),
                        Some(subject.updated_at_ms),
                        json!({
                            "age_ms": age_ms,
                            "expected_window_ms": expected_window_ms,
                        }),
                    )],
                ));
            } else {
                classification.outcome = Some(ReconOutcome::CollectingObservations);
                classification.summary =
                    Some("waiting for a Solana execution reference to appear".to_owned());
                classification.machine_reason =
                    Some(SolanaMismatchSubcode::SignatureMissing.as_str().to_owned());
            }
            return classification;
        }

        if has_finality_mismatch {
            apply_subcode(&mut classification, SolanaMismatchSubcode::OnchainStateUnresolved);
            classification.outcome = Some(ReconOutcome::Unmatched);
            classification.summary =
                Some("execution truth diverges from observed Solana state".to_owned());
            classification.machine_reason =
                Some(SolanaMismatchSubcode::OnchainStateUnresolved.as_str().to_owned());
            classification.exceptions.push(make_exception(
                ExceptionCategory::StateMismatch,
                ExceptionSeverity::High,
                ExceptionState::Open,
                "finality mismatch between execution truth and Solana observations",
                SolanaMismatchSubcode::OnchainStateUnresolved.as_str(),
                vec![evidence(
                    "fact_mismatch",
                    Some("solana.tx_attempts".to_owned()),
                    subject.latest_receipt_id.clone(),
                    Some(subject.updated_at_ms),
                    json!({
                        "mismatches": matched.mismatches,
                    }),
                )],
            ));
            return classification;
        }

        if missing_finality {
            if age_ms > expected_window_ms {
                apply_subcode(&mut classification, SolanaMismatchSubcode::PendingTooLong);
                classification.details.insert(
                    "primary_mismatch_subcode".to_owned(),
                    SolanaMismatchSubcode::PendingTooLong.as_str().to_owned(),
                );
                classification.outcome = Some(ReconOutcome::Stale);
                classification.summary = Some(
                    "execution expects finalized evidence but the confirmation window was exceeded"
                        .to_owned(),
                );
                classification.machine_reason =
                    Some(SolanaMismatchSubcode::PendingTooLong.as_str().to_owned());
                classification.exceptions.push(make_exception(
                    ExceptionCategory::DelayedFinality,
                    ExceptionSeverity::Warning,
                    ExceptionState::Investigating,
                    "finalized execution has not yet produced matching finalized observation",
                    SolanaMismatchSubcode::PendingTooLong.as_str(),
                    vec![evidence(
                        "missing_finality",
                        Some("execution_core_receipts".to_owned()),
                        subject.latest_receipt_id.clone(),
                        Some(subject.updated_at_ms),
                        json!({
                            "missing_expected": matched.missing_expected,
                            "age_ms": age_ms,
                            "expected_window_ms": expected_window_ms,
                        }),
                    )],
                ));
            } else {
                classification.outcome = Some(ReconOutcome::CollectingObservations);
                classification.summary =
                    Some("execution expects finalized evidence and is still within the observation window".to_owned());
                classification.machine_reason =
                    Some(SolanaMismatchSubcode::SignatureMissing.as_str().to_owned());
            }
            return classification;
        }

        if reference_mismatch {
            apply_subcode(&mut classification, SolanaMismatchSubcode::OnchainStateUnresolved);
            classification.outcome = Some(ReconOutcome::Unmatched);
            classification.summary =
                Some("observed Solana execution reference does not correlate with execution truth".to_owned());
            classification.machine_reason =
                Some(SolanaMismatchSubcode::OnchainStateUnresolved.as_str().to_owned());
            classification.exceptions.push(make_exception(
                ExceptionCategory::StateMismatch,
                ExceptionSeverity::High,
                ExceptionState::Investigating,
                "observed Solana execution reference diverges from durable execution reference",
                SolanaMismatchSubcode::OnchainStateUnresolved.as_str(),
                vec![evidence(
                    "execution_reference_mismatch",
                    Some("solana.tx_attempts".to_owned()),
                    Some(subject.intent_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({
                        "mismatches": matched.mismatches,
                    }),
                )],
            ));
            return classification;
        }

        if observed_finality_unknown {
            apply_subcode(&mut classification, SolanaMismatchSubcode::OnchainStateUnresolved);
            classification.outcome = Some(ReconOutcome::ManualReviewRequired);
            classification.summary =
                Some("observed Solana state is unresolved and requires manual review".to_owned());
            classification.machine_reason =
                Some(SolanaMismatchSubcode::OnchainStateUnresolved.as_str().to_owned());
            classification.exceptions.push(make_exception(
                ExceptionCategory::ExternalStateUnknown,
                ExceptionSeverity::Warning,
                ExceptionState::Open,
                "Solana observation state is unresolved and cannot be classified automatically",
                SolanaMismatchSubcode::OnchainStateUnresolved.as_str(),
                vec![evidence(
                    "unresolved_observation",
                    Some("solana.tx_attempts".to_owned()),
                    Some(subject.intent_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({
                        "observed_finality": observed_fact_value(observed, "solana.finality"),
                    }),
                )],
            ));
            return classification;
        }

        if expected_success && observed_error_present {
            apply_subcode(
                &mut classification,
                SolanaMismatchSubcode::ObservedErrorDiffersFromExpected,
            );
            classification.outcome = Some(ReconOutcome::Unmatched);
            classification.summary =
                Some("observed Solana error diverges from successful execution truth".to_owned());
            classification.machine_reason = Some(
                SolanaMismatchSubcode::ObservedErrorDiffersFromExpected
                    .as_str()
                    .to_owned(),
            );
            classification.exceptions.push(make_exception(
                ExceptionCategory::StateMismatch,
                ExceptionSeverity::High,
                ExceptionState::Open,
                "observed Solana error payload differs from successful execution truth",
                SolanaMismatchSubcode::ObservedErrorDiffersFromExpected.as_str(),
                vec![evidence(
                    "observed_error",
                    Some("solana.tx_attempts".to_owned()),
                    Some(subject.intent_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({
                        "observed_error": observed_fact_value(observed, "solana.observed_error"),
                    }),
                )],
            ));
            return classification;
        }

        if matched
            .mismatches
            .iter()
            .any(|mismatch| mismatch.fact_key == "solana.amount")
        {
            apply_subcode(&mut classification, SolanaMismatchSubcode::AmountMismatch);
            classification.outcome = Some(ReconOutcome::PartiallyMatched);
            classification.summary =
                Some("solana amount differs from expected execution truth".to_owned());
            classification.exceptions.push(make_exception(
                ExceptionCategory::AmountMismatch,
                ExceptionSeverity::High,
                ExceptionState::Open,
                "amount mismatch detected between execution truth and observed Solana state",
                SolanaMismatchSubcode::AmountMismatch.as_str(),
                vec![evidence(
                    "fact_mismatch",
                    Some("solana.tx_intents".to_owned()),
                    Some(subject.intent_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({
                        "mismatches": matched.mismatches,
                    }),
                )],
            ));
        }

        if matched
            .mismatches
            .iter()
            .any(|mismatch| mismatch.fact_key == "solana.destination")
        {
            apply_subcode(&mut classification, SolanaMismatchSubcode::DestinationMismatch);
            classification.outcome = Some(ReconOutcome::PartiallyMatched);
            classification.summary =
                Some("solana destination differs from expected execution truth".to_owned());
            classification.exceptions.push(make_exception(
                ExceptionCategory::DestinationMismatch,
                ExceptionSeverity::High,
                ExceptionState::Open,
                "destination mismatch detected between execution truth and observed Solana state",
                SolanaMismatchSubcode::DestinationMismatch.as_str(),
                vec![evidence(
                    "fact_mismatch",
                    Some("solana.tx_intents".to_owned()),
                    Some(subject.intent_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({
                        "mismatches": matched.mismatches,
                    }),
                )],
            ));
        }

        if matched
            .mismatches
            .iter()
            .any(|mismatch| mismatch.fact_key == "solana.source")
        {
            apply_subcode(&mut classification, SolanaMismatchSubcode::SourceMismatch);
            classification.outcome.get_or_insert(ReconOutcome::PartiallyMatched);
        }

        if matched
            .mismatches
            .iter()
            .any(|mismatch| mismatch.fact_key == "solana.program")
        {
            apply_subcode(&mut classification, SolanaMismatchSubcode::ProgramMismatch);
            classification.outcome.get_or_insert(ReconOutcome::PartiallyMatched);
        }

        if matched
            .mismatches
            .iter()
            .any(|mismatch| mismatch.fact_key == "solana.action")
        {
            apply_subcode(&mut classification, SolanaMismatchSubcode::ActionMismatch);
            classification.outcome.get_or_insert(ReconOutcome::PartiallyMatched);
        }

        if delivery_terminal_failure {
            classification.outcome = Some(ReconOutcome::ManualReviewRequired);
            classification.summary =
                Some("execution truth is durable but delivery failed terminally".to_owned());
            classification.machine_reason = Some("delivery_terminal_failure".to_owned());
            classification.exceptions.push(make_exception(
                ExceptionCategory::ExternalStateUnknown,
                ExceptionSeverity::Warning,
                ExceptionState::Open,
                "callback delivery failed terminally after execution truth was committed",
                "external_state_unknown",
                vec![evidence(
                    "callback_delivery",
                    Some("callback_core_deliveries".to_owned()),
                    subject.latest_callback_id.clone(),
                    Some(subject.updated_at_ms),
                    json!({
                        "delivery_terminal_failure": true,
                    }),
                )],
            ));
            return classification;
        }

        if is_finalized_match && classification.exceptions.is_empty() {
            classification.outcome = Some(ReconOutcome::Matched);
            classification.summary =
                Some("execution truth and observed Solana state match".to_owned());
        } else if classification.outcome.is_none() {
            classification.outcome = Some(ReconOutcome::Matching);
            classification.summary =
                Some("reconciliation facts collected and awaiting stable convergence".to_owned());
        }

        if classification.machine_reason.is_none() {
            classification.machine_reason = Some(
                classification
                    .details
                    .get("primary_mismatch_subcode")
                    .cloned()
                    .unwrap_or_else(|| {
                        if classification.outcome == Some(ReconOutcome::Matched) {
                            "matched".to_owned()
                        } else {
                            "matching".to_owned()
                        }
                    }),
            );
        }

        classification
    }
}

fn expected_execution_reference(subject: &ReconSubject, context: &ReconContext) -> Option<String> {
    subject
        .adapter_execution_reference
        .clone()
        .or_else(|| subject.external_observation_key.clone())
        .or_else(|| {
            context.latest_receipt.as_ref().and_then(|receipt| {
                extract_text_from_value(
                    receipt,
                    &[
                        "/details/signature",
                        "/details/final_signature",
                        "/details/adapter_execution_reference",
                    ],
                )
            })
        })
}

fn expected_terminal_window_ms(subject: &ReconSubject) -> u64 {
    if subject.canonical_state.eq_ignore_ascii_case("Succeeded") {
        300_000
    } else if subject.canonical_state.eq_ignore_ascii_case("FailedTerminal")
        || subject.canonical_state.eq_ignore_ascii_case("Rejected")
        || subject.canonical_state.eq_ignore_ascii_case("DeadLettered")
    {
        120_000
    } else {
        60_000
    }
}

fn advisory_source(source: &str, id: Option<&str>) -> Value {
    json!({
        "source": source,
        "source_id": id,
        "match_mode": "advisory",
    })
}

fn push_observed_fact(
    out: &mut Vec<ObservedFactDraft>,
    fact_type: &str,
    fact_key: &str,
    fact_value: Value,
    source_id: Option<String>,
    observed_at_ms: Option<u64>,
    metadata: Value,
) {
    push_observed_fact_with_mode(
        out,
        fact_type,
        fact_key,
        fact_value,
        source_id,
        observed_at_ms,
        metadata,
        "strict",
    );
}

fn push_observed_fact_with_mode(
    out: &mut Vec<ObservedFactDraft>,
    fact_type: &str,
    fact_key: &str,
    fact_value: Value,
    source_id: Option<String>,
    observed_at_ms: Option<u64>,
    metadata: Value,
    match_mode: &str,
) {
    let mut metadata_obj = metadata;
    if let Some(obj) = metadata_obj.as_object_mut() {
        obj.entry("match_mode".to_owned())
            .or_insert_with(|| json!(match_mode));
    } else {
        metadata_obj = json!({
            "value": metadata_obj,
            "match_mode": match_mode,
        });
    }

    out.push(ObservedFactDraft {
        fact_type: fact_type.to_owned(),
        fact_key: fact_key.to_owned(),
        fact_value,
        source_kind: "adapter_observation".to_owned(),
        source_table: Some("solana.tx_attempts".to_owned()),
        source_id,
        metadata: metadata_obj,
        observed_at_ms,
    });
}

fn normalized_solana_finality(row: &Value) -> &'static str {
    let status = row
        .get("last_confirmation_status")
        .and_then(|value| value.as_str())
        .or_else(|| row.get("status").and_then(|value| value.as_str()))
        .unwrap_or("unknown");
    if status.eq_ignore_ascii_case("finalized") || row.get("final_signature").is_some() {
        "finalized"
    } else if status.eq_ignore_ascii_case("confirmed") {
        "confirmed"
    } else if status.eq_ignore_ascii_case("sent") || status.eq_ignore_ascii_case("created") {
        "pending"
    } else if status.eq_ignore_ascii_case("expired") {
        "not_finalized"
    } else {
        "unknown"
    }
}

fn values_match(fact_key: &str, expected: &Value, observed: &Value) -> bool {
    match fact_key {
        "solana.amount" => expected.as_i64() == observed.as_i64(),
        "solana.finality" => normalize_text(expected.as_str()) == normalize_text(observed.as_str()),
        "solana.execution_reference" | "solana.signature" => {
            normalize_text(expected.as_str()) == normalize_text(observed.as_str())
        }
        "solana.asset" | "solana.program" | "solana.action" => {
            normalize_text(expected.as_str()) == normalize_text(observed.as_str())
        }
        _ => expected == observed,
    }
}

fn mismatch_type_for_fact(fact_key: &str) -> String {
    match fact_key {
        "solana.amount" => SolanaMismatchSubcode::AmountMismatch.as_str().to_owned(),
        "solana.destination" => SolanaMismatchSubcode::DestinationMismatch.as_str().to_owned(),
        "solana.execution_reference" | "solana.signature" => {
            SolanaMismatchSubcode::SignatureMissing.as_str().to_owned()
        }
        "solana.source" => SolanaMismatchSubcode::SourceMismatch.as_str().to_owned(),
        "solana.program" => SolanaMismatchSubcode::ProgramMismatch.as_str().to_owned(),
        "solana.action" => SolanaMismatchSubcode::ActionMismatch.as_str().to_owned(),
        _ => "value_mismatch".to_owned(),
    }
}

fn is_advisory_expected_fact(fact: &ExpectedFactDraft) -> bool {
    fact.derived_from
        .get("match_mode")
        .and_then(|value| value.as_str())
        == Some("advisory")
}

fn is_supplemental_observed_fact(fact: &ObservedFactDraft) -> bool {
    fact.metadata
        .get("match_mode")
        .and_then(|value| value.as_str())
        == Some("supplemental")
}

fn apply_subcode(classification: &mut ReconClassification, subcode: SolanaMismatchSubcode) {
    let subcode_str = subcode.as_str().to_owned();
    let mut codes: Vec<String> = classification
        .details
        .get("mismatch_subcodes")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !codes.iter().any(|value| value == &subcode_str) {
        codes.push(subcode_str.clone());
    }
    classification
        .details
        .insert("mismatch_subcodes".to_owned(), codes.join(","));
    classification
        .details
        .entry("primary_mismatch_subcode".to_owned())
        .or_insert(subcode_str);
}

fn expected_fact_value<'a>(expected: &'a [ExpectedFactDraft], key: &str) -> Option<&'a Value> {
    expected
        .iter()
        .find(|fact| fact.fact_key == key)
        .map(|fact| &fact.fact_value)
}

fn observed_fact_value<'a>(observed: &'a [ObservedFactDraft], key: &str) -> Option<&'a Value> {
    observed
        .iter()
        .find(|fact| fact.fact_key == key)
        .map(|fact| &fact.fact_value)
}

fn expected_fact_u64(expected: &[ExpectedFactDraft], key: &str) -> Option<u64> {
    expected_fact_value(expected, key).and_then(as_u64)
}

fn observed_fact_u64(observed: &[ObservedFactDraft], key: &str) -> Option<u64> {
    observed_fact_value(observed, key).and_then(as_u64)
}

fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn extract_text_from_value(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|pointer| {
        value.pointer(pointer).and_then(|candidate| match candidate {
            Value::String(text) => {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_owned())
                }
            }
            Value::Number(number) => Some(number.to_string()),
            Value::Bool(flag) => Some(flag.to_string()),
            _ => None,
        })
    })
}

fn current_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn default_outcome(subject: &ReconSubject, matched: &ReconMatchResult) -> ReconOutcome {
    if subject.canonical_state.eq_ignore_ascii_case("Succeeded")
        && matched.missing_expected.is_empty()
        && matched.mismatches.is_empty()
    {
        ReconOutcome::Matched
    } else if !matched.mismatches.is_empty() {
        ReconOutcome::Unmatched
    } else if !matched.missing_expected.is_empty() {
        ReconOutcome::CollectingObservations
    } else {
        ReconOutcome::Matching
    }
}

fn default_summary(
    subject: &ReconSubject,
    outcome: &ReconOutcome,
    matched: &ReconMatchResult,
) -> String {
    format!(
        "subject {} for intent {} is {} with {} matched facts and {} mismatches",
        subject.subject_id,
        subject.intent_id,
        outcome.as_str(),
        matched.matched_fact_keys.len(),
        matched.mismatches.len()
    )
}

fn default_machine_reason(outcome: &ReconOutcome, matched: &ReconMatchResult) -> String {
    if !matched.mismatches.is_empty() {
        "mismatch_detected".to_owned()
    } else if !matched.missing_expected.is_empty() {
        "missing_expected_observations".to_owned()
    } else {
        outcome.as_str().to_owned()
    }
}

fn as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().map(|value| value.max(0) as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ReconContext;

    fn subject(state: &str) -> ReconSubject {
        ReconSubject {
            subject_id: "reconsub_test".to_owned(),
            tenant_id: "tenant_demo".to_owned(),
            intent_id: "intent_demo".to_owned(),
            job_id: "job_demo".to_owned(),
            adapter_id: "adapter_solana".to_owned(),
            canonical_state: state.to_owned(),
            platform_classification: "Success".to_owned(),
            latest_receipt_id: Some("receipt_demo".to_owned()),
            latest_transition_id: Some("transition_demo".to_owned()),
            latest_callback_id: None,
            latest_signal_id: Some("signal_demo".to_owned()),
            latest_signal_kind: Some("finalized".to_owned()),
            execution_correlation_id: Some("corr_demo".to_owned()),
            adapter_execution_reference: Some("sig_demo".to_owned()),
            external_observation_key: Some("sig_demo".to_owned()),
            expected_fact_snapshot: Some(json!({ "version": 1 })),
            dirty: true,
            recon_attempt_count: 0,
            recon_retry_count: 0,
            created_at_ms: 1,
            updated_at_ms: 10,
            scheduled_at_ms: Some(10),
            next_reconcile_after_ms: Some(10),
            last_reconciled_at_ms: None,
            last_recon_error: None,
            last_run_state: None,
        }
    }

    fn context() -> ReconContext {
        ReconContext {
            intent: Some(json!({
                "payload": {
                    "from_addr": "source_123",
                    "to_addr": "dest_123",
                    "amount": 42,
                    "asset": "SOL",
                    "program_id": "system_program",
                    "action": "transfer"
                }
            })),
            latest_receipt: Some(json!({
                "details": {
                    "signature": "sig_demo",
                    "fee_payer": "source_123"
                }
            })),
            ..ReconContext::default()
        }
    }

    #[tokio::test]
    async fn solana_rule_pack_matches_finalized_success() {
        let pack = SolanaReconRulePack;
        let subject = subject("Succeeded");
        let context = context();
        let expected = pack.build_expected_facts(&subject, &context).await.unwrap();
        let observed = pack
            .collect_observed_facts(
                &subject,
                &context,
                &[json!({
                    "attempt_id": "attempt_1",
                    "intent_id": "intent_demo",
                    "status": "finalized",
                    "signature": "sig_demo",
                    "final_signature": "sig_demo",
                    "from_addr": "source_123",
                    "to_addr": "dest_123",
                    "amount": 42,
                    "asset": "SOL",
                    "program_id": "system_program",
                    "action": "transfer",
                    "updated_at_ms": 10
                })],
            )
            .await
            .unwrap();
        let matched = pack.match_facts(&subject, &expected, &observed);
        let classification = pack.classify(&subject, &context, &expected, &observed, &matched);
        let emission = pack.emit_recon_result(&subject, &matched, classification);
        assert_eq!(emission.outcome, ReconOutcome::Matched);
    }

    #[tokio::test]
    async fn solana_rule_pack_detects_missing_observations() {
        let pack = SolanaReconRulePack;
        let subject = subject("Succeeded");
        let context = context();
        let expected = pack.build_expected_facts(&subject, &context).await.unwrap();
        let observed = pack
            .collect_observed_facts(&subject, &context, &[])
            .await
            .unwrap();
        let matched = pack.match_facts(&subject, &expected, &observed);
        let classification = pack.classify(&subject, &context, &expected, &observed, &matched);
        let emission = pack.emit_recon_result(&subject, &matched, classification);
        assert_eq!(emission.outcome, ReconOutcome::CollectingObservations);
        assert_eq!(emission.machine_reason, "signature_missing");
    }

    #[tokio::test]
    async fn solana_rule_pack_distinguishes_failed_execution_from_matched() {
        let pack = SolanaReconRulePack;
        let mut subject = subject("FailedTerminal");
        subject.platform_classification = "TerminalFailure".to_owned();
        let context = context();
        let expected = pack.build_expected_facts(&subject, &context).await.unwrap();
        let observed = pack
            .collect_observed_facts(
                &subject,
                &context,
                &[json!({
                    "attempt_id": "attempt_1",
                    "intent_id": "intent_demo",
                    "status": "finalized",
                    "signature": "sig_demo",
                    "final_signature": "sig_demo",
                    "from_addr": "source_123",
                    "to_addr": "dest_123",
                    "amount": 42,
                    "asset": "SOL",
                    "program_id": "system_program",
                    "action": "transfer",
                    "updated_at_ms": 10
                })],
            )
            .await
            .unwrap();
        let matched = pack.match_facts(&subject, &expected, &observed);
        let classification = pack.classify(&subject, &context, &expected, &observed, &matched);
        let emission = pack.emit_recon_result(&subject, &matched, classification);
        assert_eq!(emission.outcome, ReconOutcome::Unmatched);
        assert_eq!(emission.machine_reason, "onchain_state_unresolved");
    }
}
