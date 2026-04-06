use crate::error::ReconError;
use crate::model::{
    evidence, make_exception, ExpectedFactDraft, ObservedFactDraft, ReconClassification,
    ReconContext, ReconMatchResult, ReconOutcome, ReconSubject,
};
use crate::rules::ReconRulePack;
use async_trait::async_trait;
use exception_intelligence::{ExceptionCategory, ExceptionSeverity, ExceptionState};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub struct PaystackReconRulePack;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaystackMismatchSubcode {
    VerificationReferenceMissing,
    PaymentStatusMismatch,
    AmountMismatch,
    CurrencyMismatch,
    VerificationPendingTooLong,
    DuplicateEvent,
    DestinationMismatch,
    SourceReferenceMismatch,
    ConnectorReferenceMismatch,
    ExternalStateUnknown,
    ObservedErrorDiffersFromExpected,
}

impl PaystackMismatchSubcode {
    fn as_str(self) -> &'static str {
        match self {
            Self::VerificationReferenceMissing => "verification_reference_missing",
            Self::PaymentStatusMismatch => "payment_status_mismatch",
            Self::AmountMismatch => "amount_mismatch",
            Self::CurrencyMismatch => "currency_mismatch",
            Self::VerificationPendingTooLong => "verification_pending_too_long",
            Self::DuplicateEvent => "duplicate_event",
            Self::DestinationMismatch => "destination_mismatch",
            Self::SourceReferenceMismatch => "source_reference_mismatch",
            Self::ConnectorReferenceMismatch => "connector_reference_mismatch",
            Self::ExternalStateUnknown => "external_state_unknown",
            Self::ObservedErrorDiffersFromExpected => "observed_error_differs_from_expected",
        }
    }
}

#[async_trait]
impl ReconRulePack for PaystackReconRulePack {
    fn adapter_id(&self) -> &'static str {
        "adapter_paystack"
    }

    fn rule_pack_id(&self) -> &'static str {
        "paystack.v1"
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
                derived_from: json!({ "source": "execution_core_jobs", "job_id": subject.job_id }),
            },
            ExpectedFactDraft {
                fact_type: "execution".to_owned(),
                fact_key: "execution.classification".to_owned(),
                fact_value: json!(subject.platform_classification),
                derived_from: json!({ "source": "execution_core_jobs", "job_id": subject.job_id }),
            },
            ExpectedFactDraft {
                fact_type: "paystack".to_owned(),
                fact_key: "paystack.verification_state".to_owned(),
                fact_value: json!(expected_verification_state(subject)),
                derived_from: json!({
                    "source": "execution_core_receipts",
                    "receipt_id": subject.latest_receipt_id,
                }),
            },
            ExpectedFactDraft {
                fact_type: "paystack".to_owned(),
                fact_key: "paystack.verification_window_ms".to_owned(),
                fact_value: json!(expected_window_ms(subject)),
                derived_from: json!({
                    "source": "recon_policy",
                    "source_id": subject.subject_id,
                    "match_mode": "advisory",
                }),
            },
        ];

        if let Some(intent) = context.intent.as_ref() {
            if let Some(amount) = intent.pointer("/payload/amount").cloned() {
                out.push(ExpectedFactDraft {
                    fact_type: "paystack".to_owned(),
                    fact_key: "paystack.amount_minor".to_owned(),
                    fact_value: amount,
                    derived_from: json!({ "source": "execution_core_intents", "intent_id": subject.intent_id }),
                });
            }
            if let Some(currency) = extract_text(intent, &["/payload/currency"]) {
                out.push(ExpectedFactDraft {
                    fact_type: "paystack".to_owned(),
                    fact_key: "paystack.currency".to_owned(),
                    fact_value: json!(currency),
                    derived_from: json!({ "source": "execution_core_intents", "intent_id": subject.intent_id }),
                });
            }
            if let Some(source_reference) = extract_text(
                intent,
                &[
                    "/payload/payment_reference",
                    "/payload/customer_reference",
                    "/payload/source",
                ],
            ) {
                out.push(ExpectedFactDraft {
                    fact_type: "paystack".to_owned(),
                    fact_key: "paystack.source_reference".to_owned(),
                    fact_value: json!(source_reference),
                    derived_from: json!({ "source": "execution_core_intents", "intent_id": subject.intent_id }),
                });
            }
            if let Some(destination_reference) = extract_text(
                intent,
                &["/payload/destination_reference", "/payload/recipient_code"],
            ) {
                out.push(ExpectedFactDraft {
                    fact_type: "paystack".to_owned(),
                    fact_key: "paystack.destination_reference".to_owned(),
                    fact_value: json!(destination_reference),
                    derived_from: json!({ "source": "execution_core_intents", "intent_id": subject.intent_id }),
                });
            }
        }

        if let Some(reference) = expected_reference(subject, context) {
            out.push(ExpectedFactDraft {
                fact_type: "paystack".to_owned(),
                fact_key: "paystack.execution_reference".to_owned(),
                fact_value: json!(reference),
                derived_from: json!({ "source": "execution_core_receipts", "receipt_id": subject.latest_receipt_id }),
            });
        }
        if let Some(reference) = expected_connector_reference(subject, context) {
            out.push(ExpectedFactDraft {
                fact_type: "paystack".to_owned(),
                fact_key: "paystack.connector_reference".to_owned(),
                fact_value: json!(reference),
                derived_from: json!({
                    "source": "execution_core_receipts",
                    "receipt_id": subject.latest_receipt_id,
                    "match_mode": "advisory",
                }),
            });
        }

        Ok(out)
    }

    async fn collect_observed_facts(
        &self,
        _subject: &ReconSubject,
        _context: &ReconContext,
        adapter_rows: &[Value],
    ) -> Result<Vec<ObservedFactDraft>, ReconError> {
        Ok(collect_observed(adapter_rows))
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
            if is_advisory_expected(fact) {
                continue;
            }
            match observed_map.get(&fact.fact_key) {
                Some(value) if values_match(&fact.fact_key, &fact.fact_value, value) => {
                    result.matched_fact_keys.push(fact.fact_key.clone());
                }
                Some(value) => result.mismatches.push(crate::model::FactMismatch {
                    fact_key: fact.fact_key.clone(),
                    mismatch_type: mismatch_type(&fact.fact_key),
                    expected: Some(fact.fact_value.clone()),
                    observed: Some(value.clone()),
                    message: format!("expected `{}` to match observed value", fact.fact_key),
                }),
                None => result.missing_expected.push(fact.fact_key.clone()),
            }
        }

        for fact in observed {
            if is_supplemental_observed(fact) {
                continue;
            }
            if !expected
                .iter()
                .filter(|value| !is_advisory_expected(value))
                .any(|value| value.fact_key == fact.fact_key)
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
        let age_ms = current_ms().saturating_sub(subject.updated_at_ms);
        let expected_window_ms =
            expected_u64(expected, "paystack.verification_window_ms").unwrap_or(300_000);
        let observed_state =
            observed_value(observed, "paystack.verification_state").and_then(Value::as_str);
        let expected_state =
            expected_value(expected, "paystack.verification_state").and_then(Value::as_str);
        let distinct_reference_count =
            observed_u64(observed, "paystack.distinct_reference_count").unwrap_or_default();

        if is_pre_execution_state(subject.canonical_state.as_str()) {
            classification.outcome = Some(ReconOutcome::Queued);
            classification.summary =
                Some("reconciliation subject queued behind execution progression".to_owned());
            classification.machine_reason =
                Some("execution_not_ready_for_reconciliation".to_owned());
            return classification;
        }

        if distinct_reference_count > 1 {
            apply_subcode(&mut classification, PaystackMismatchSubcode::DuplicateEvent);
            classification.outcome = Some(ReconOutcome::ManualReviewRequired);
            classification.summary = Some(
                "multiple Paystack references were observed for one execution subject".to_owned(),
            );
            classification.machine_reason =
                Some(PaystackMismatchSubcode::DuplicateEvent.as_str().to_owned());
            classification.exceptions.push(make_exception(
                ExceptionCategory::DuplicateSignal,
                ExceptionSeverity::High,
                ExceptionState::Open,
                "multiple Paystack references or webhook correlations were observed for one subject",
                PaystackMismatchSubcode::DuplicateEvent.as_str(),
                vec![evidence(
                    "duplicate_reference_set",
                    Some("paystack.webhook_events".to_owned()),
                    Some(subject.intent_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({ "distinct_reference_count": distinct_reference_count }),
                )],
            ));
            return classification;
        }

        if observed.is_empty()
            || matched
                .missing_expected
                .iter()
                .any(|key| key == "paystack.execution_reference")
        {
            apply_subcode(
                &mut classification,
                PaystackMismatchSubcode::VerificationReferenceMissing,
            );
            if age_ms > expected_window_ms {
                apply_subcode(
                    &mut classification,
                    PaystackMismatchSubcode::VerificationPendingTooLong,
                );
                classification.outcome = Some(ReconOutcome::Stale);
                classification.summary = Some(
                    "waiting for Paystack verification evidence exceeded the expected window"
                        .to_owned(),
                );
                classification.machine_reason = Some(
                    PaystackMismatchSubcode::VerificationPendingTooLong
                        .as_str()
                        .to_owned(),
                );
                classification.exceptions.push(make_exception(
                    ExceptionCategory::DelayedVerification,
                    ExceptionSeverity::Warning,
                    ExceptionState::Investigating,
                    "Paystack verification evidence was not observed within the expected window",
                    PaystackMismatchSubcode::VerificationPendingTooLong.as_str(),
                    vec![evidence(
                        "reconciliation_window",
                        Some("recon_core_subjects".to_owned()),
                        Some(subject.subject_id.clone()),
                        Some(subject.updated_at_ms),
                        json!({ "age_ms": age_ms, "expected_window_ms": expected_window_ms }),
                    )],
                ));
            } else {
                classification.outcome = Some(ReconOutcome::CollectingObservations);
                classification.summary =
                    Some("waiting for downstream Paystack observations".to_owned());
                classification.machine_reason = Some(
                    PaystackMismatchSubcode::VerificationReferenceMissing
                        .as_str()
                        .to_owned(),
                );
            }
            return classification;
        }

        if matched.mismatches.iter().any(|value| {
            value.fact_key == "paystack.execution_reference"
                || value.fact_key == "paystack.verification_state"
        }) {
            apply_subcode(
                &mut classification,
                PaystackMismatchSubcode::PaymentStatusMismatch,
            );
            classification.outcome = Some(ReconOutcome::Unmatched);
            classification.summary = Some(
                "execution truth diverges from observed Paystack verification state".to_owned(),
            );
            classification.machine_reason = Some(
                PaystackMismatchSubcode::PaymentStatusMismatch
                    .as_str()
                    .to_owned(),
            );
            classification.exceptions.push(make_exception(
                ExceptionCategory::StateMismatch,
                ExceptionSeverity::High,
                ExceptionState::Open,
                "Paystack verification state or reference differs from durable execution truth",
                PaystackMismatchSubcode::PaymentStatusMismatch.as_str(),
                vec![evidence(
                    "verification_state_mismatch",
                    Some("paystack.executions".to_owned()),
                    Some(subject.intent_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({ "mismatches": matched.mismatches }),
                )],
            ));
            return classification;
        }

        if observed_state == Some("unknown") {
            apply_subcode(
                &mut classification,
                PaystackMismatchSubcode::ExternalStateUnknown,
            );
            classification.outcome = Some(ReconOutcome::ManualReviewRequired);
            classification.summary =
                Some("observed Paystack state is unresolved and requires manual review".to_owned());
            classification.machine_reason = Some(
                PaystackMismatchSubcode::ExternalStateUnknown
                    .as_str()
                    .to_owned(),
            );
            classification.exceptions.push(make_exception(
                ExceptionCategory::ExternalStateUnknown,
                ExceptionSeverity::Warning,
                ExceptionState::Open,
                "Paystack observation state is unresolved and cannot be classified automatically",
                PaystackMismatchSubcode::ExternalStateUnknown.as_str(),
                vec![evidence(
                    "unresolved_observation",
                    Some("paystack.executions".to_owned()),
                    Some(subject.intent_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({ "observed_state": observed_state }),
                )],
            ));
            return classification;
        }

        if expected_state == Some("succeeded")
            && observed.iter().any(|fact| {
                fact.fact_key == "paystack.observed_error" && !fact.fact_value.is_null()
            })
        {
            apply_subcode(
                &mut classification,
                PaystackMismatchSubcode::ObservedErrorDiffersFromExpected,
            );
            classification.outcome = Some(ReconOutcome::Unmatched);
            classification.summary = Some(
                "observed Paystack error payload diverges from successful execution truth"
                    .to_owned(),
            );
            classification.machine_reason = Some(
                PaystackMismatchSubcode::ObservedErrorDiffersFromExpected
                    .as_str()
                    .to_owned(),
            );
            classification.exceptions.push(make_exception(
                ExceptionCategory::StateMismatch,
                ExceptionSeverity::High,
                ExceptionState::Open,
                "observed Paystack error payload differs from successful execution truth",
                PaystackMismatchSubcode::ObservedErrorDiffersFromExpected.as_str(),
                vec![evidence(
                    "observed_error",
                    Some("paystack.executions".to_owned()),
                    Some(subject.intent_id.clone()),
                    Some(subject.updated_at_ms),
                    json!({ "observed_error": observed_value(observed, "paystack.observed_error") }),
                )],
            ));
            return classification;
        }

        accumulate_partial_mismatch(
            &mut classification,
            matched,
            subject,
            "paystack.amount_minor",
            PaystackMismatchSubcode::AmountMismatch,
            ExceptionCategory::AmountMismatch,
            ExceptionSeverity::Critical,
            "Paystack amount differs from durable execution truth",
        );
        accumulate_partial_mismatch(
            &mut classification,
            matched,
            subject,
            "paystack.currency",
            PaystackMismatchSubcode::CurrencyMismatch,
            ExceptionCategory::StateMismatch,
            ExceptionSeverity::High,
            "Paystack currency differs from durable execution truth",
        );
        accumulate_partial_mismatch(
            &mut classification,
            matched,
            subject,
            "paystack.destination_reference",
            PaystackMismatchSubcode::DestinationMismatch,
            ExceptionCategory::DestinationMismatch,
            ExceptionSeverity::High,
            "Paystack destination differs from durable execution truth",
        );
        accumulate_partial_mismatch(
            &mut classification,
            matched,
            subject,
            "paystack.source_reference",
            PaystackMismatchSubcode::SourceReferenceMismatch,
            ExceptionCategory::StateMismatch,
            ExceptionSeverity::High,
            "Paystack source reference differs from durable execution truth",
        );
        accumulate_partial_mismatch(
            &mut classification,
            matched,
            subject,
            "paystack.connector_reference",
            PaystackMismatchSubcode::ConnectorReferenceMismatch,
            ExceptionCategory::StateMismatch,
            ExceptionSeverity::High,
            "Paystack connector reference differs from durable execution truth",
        );

        if classification.exceptions.is_empty() {
            classification.outcome = Some(match observed_state {
                Some("pending") => ReconOutcome::Matching,
                Some("succeeded") | Some("not_succeeded") => ReconOutcome::Matched,
                _ => ReconOutcome::Matching,
            });
            classification.summary = Some(match classification.outcome {
                Some(ReconOutcome::Matched) => {
                    "execution truth and observed Paystack state match".to_owned()
                }
                _ => "Paystack verification evidence is present and still converging".to_owned(),
            });
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

fn collect_observed(adapter_rows: &[Value]) -> Vec<ObservedFactDraft> {
    let mut out = Vec::new();
    let execution = adapter_rows
        .iter()
        .find(|row| row.get("row_kind").and_then(Value::as_str) == Some("execution"));
    let latest_webhook = adapter_rows
        .iter()
        .filter(|row| row.get("row_kind").and_then(Value::as_str) == Some("webhook"))
        .max_by_key(|row| {
            row.get("received_at_ms")
                .and_then(as_u64)
                .unwrap_or_default()
        });
    let mut distinct_references = HashSet::new();

    if let Some(row) = execution {
        let source_id = row
            .get("intent_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let observed_at_ms = row.get("updated_at_ms").and_then(as_u64);
        if let Some(reference) = execution_reference(row) {
            distinct_references.insert(reference.clone());
            push_observed(
                &mut out,
                "paystack.execution_reference",
                json!(reference),
                Some("paystack.executions"),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
                "strict",
            );
        }
        push_observed(
            &mut out,
            "paystack.verification_state",
            json!(execution_state(row)),
            Some("paystack.executions"),
            source_id.clone(),
            observed_at_ms,
            row.clone(),
            "strict",
        );
        if let Some(value) = row.get("amount_minor").and_then(Value::as_i64) {
            push_observed(
                &mut out,
                "paystack.amount_minor",
                json!(value),
                Some("paystack.executions"),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
                "strict",
            );
        }
        if let Some(value) = row.get("currency").and_then(Value::as_str) {
            push_observed(
                &mut out,
                "paystack.currency",
                json!(value),
                Some("paystack.executions"),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
                "strict",
            );
        }
        if let Some(value) = row.get("source_reference").and_then(Value::as_str) {
            push_observed(
                &mut out,
                "paystack.source_reference",
                json!(value),
                Some("paystack.executions"),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
                "strict",
            );
        }
        if let Some(value) = row.get("destination_reference").and_then(Value::as_str) {
            push_observed(
                &mut out,
                "paystack.destination_reference",
                json!(value),
                Some("paystack.executions"),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
                "strict",
            );
        }
        if let Some(value) = row.get("connector_reference").and_then(Value::as_str) {
            push_observed(
                &mut out,
                "paystack.connector_reference",
                json!(value),
                Some("paystack.executions"),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
                "strict",
            );
        }
        if let Some(value) = row.get("status").and_then(Value::as_str) {
            push_observed(
                &mut out,
                "paystack.provider_status",
                json!(value),
                Some("paystack.executions"),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
                "supplemental",
            );
        }
        if let Some(error) = execution_error(row) {
            push_observed(
                &mut out,
                "paystack.observed_error",
                error,
                Some("paystack.executions"),
                source_id,
                observed_at_ms,
                row.clone(),
                "supplemental",
            );
        }
    }

    if let Some(row) = latest_webhook {
        let source_id = row
            .get("event_key")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let observed_at_ms = row.get("received_at_ms").and_then(as_u64);
        let payload = row.get("payload").unwrap_or(&Value::Null);
        let data = payload.get("data").unwrap_or(payload);
        if execution.is_none() {
            if let Some(reference) = webhook_reference(row, data) {
                distinct_references.insert(reference.clone());
                push_observed(
                    &mut out,
                    "paystack.execution_reference",
                    json!(reference),
                    Some("paystack.webhook_events"),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                    "strict",
                );
            }
            if let Some(status) = webhook_status(data) {
                push_observed(
                    &mut out,
                    "paystack.verification_state",
                    json!(provider_state(status)),
                    Some("paystack.webhook_events"),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                    "strict",
                );
            }
            if let Some(value) = data.get("amount").and_then(Value::as_i64) {
                push_observed(
                    &mut out,
                    "paystack.amount_minor",
                    json!(value),
                    Some("paystack.webhook_events"),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                    "strict",
                );
            }
            if let Some(value) = extract_text(data, &["/currency"]) {
                push_observed(
                    &mut out,
                    "paystack.currency",
                    json!(value),
                    Some("paystack.webhook_events"),
                    source_id.clone(),
                    observed_at_ms,
                    row.clone(),
                    "strict",
                );
            }
        }
        if let Some(reference) = webhook_reference(row, data) {
            distinct_references.insert(reference);
        }
        if let Some(event_type) = row.get("event_type").and_then(Value::as_str) {
            push_observed(
                &mut out,
                "paystack.webhook_latest_event_type",
                json!(event_type),
                Some("paystack.webhook_events"),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
                "supplemental",
            );
        }
        if let Some(status) = webhook_status(data) {
            push_observed(
                &mut out,
                "paystack.webhook_latest_status",
                json!(status),
                Some("paystack.webhook_events"),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
                "supplemental",
            );
        }
        if let Some(error) = webhook_error(data) {
            push_observed(
                &mut out,
                "paystack.observed_error",
                error,
                Some("paystack.webhook_events"),
                source_id.clone(),
                observed_at_ms,
                row.clone(),
                "supplemental",
            );
        }
        push_observed(
            &mut out,
            "paystack.webhook_event_count",
            json!(1_u64),
            Some("paystack.webhook_events"),
            source_id,
            observed_at_ms,
            row.clone(),
            "supplemental",
        );
    }

    let latest_observed_at_ms = execution
        .and_then(|row| row.get("updated_at_ms").and_then(as_u64))
        .or_else(|| latest_webhook.and_then(|row| row.get("received_at_ms").and_then(as_u64)));
    if latest_observed_at_ms.is_some() {
        push_observed(
            &mut out,
            "paystack.distinct_reference_count",
            json!(distinct_references.len() as u64),
            Some("paystack.executions"),
            execution
                .and_then(|row| row.get("intent_id").and_then(Value::as_str))
                .map(ToOwned::to_owned),
            latest_observed_at_ms,
            json!({ "distinct_reference_count": distinct_references.len() }),
            "supplemental",
        );
    }

    out
}

fn accumulate_partial_mismatch(
    classification: &mut ReconClassification,
    matched: &ReconMatchResult,
    subject: &ReconSubject,
    fact_key: &str,
    subcode: PaystackMismatchSubcode,
    category: ExceptionCategory,
    severity: ExceptionSeverity,
    summary: &str,
) {
    if !matched
        .mismatches
        .iter()
        .any(|value| value.fact_key == fact_key)
    {
        return;
    }
    apply_subcode(classification, subcode);
    classification
        .outcome
        .get_or_insert(ReconOutcome::PartiallyMatched);
    classification
        .summary
        .get_or_insert_with(|| summary.to_owned());
    classification
        .machine_reason
        .get_or_insert_with(|| subcode.as_str().to_owned());
    classification.exceptions.push(make_exception(
        category,
        severity,
        ExceptionState::Open,
        summary,
        subcode.as_str(),
        vec![evidence(
            "fact_mismatch",
            Some("paystack.executions".to_owned()),
            Some(subject.intent_id.clone()),
            Some(subject.updated_at_ms),
            json!({ "mismatches": matched.mismatches }),
        )],
    ));
}

fn push_observed(
    out: &mut Vec<ObservedFactDraft>,
    fact_key: &str,
    fact_value: Value,
    source_table: Option<&str>,
    source_id: Option<String>,
    observed_at_ms: Option<u64>,
    mut metadata: Value,
    match_mode: &str,
) {
    if let Some(obj) = metadata.as_object_mut() {
        obj.entry("match_mode".to_owned())
            .or_insert_with(|| json!(match_mode));
    } else {
        metadata = json!({ "value": metadata, "match_mode": match_mode });
    }
    out.push(ObservedFactDraft {
        fact_type: "paystack".to_owned(),
        fact_key: fact_key.to_owned(),
        fact_value,
        source_kind: if source_table == Some("paystack.webhook_events") {
            "provider_webhook".to_owned()
        } else {
            "adapter_observation".to_owned()
        },
        source_table: source_table.map(ToOwned::to_owned),
        source_id,
        metadata,
        observed_at_ms,
    });
}

fn expected_reference(subject: &ReconSubject, context: &ReconContext) -> Option<String> {
    subject
        .adapter_execution_reference
        .clone()
        .or_else(|| subject.external_observation_key.clone())
        .or_else(|| {
            context.latest_receipt.as_ref().and_then(|value| {
                extract_text(
                    value,
                    &[
                        "/recon_linkage/connector_reference",
                        "/connector_outcome/reference",
                        "/recon_linkage/adapter_execution_reference",
                        "/adapter_execution_reference",
                        "/external_observation_key",
                        "/details/reference",
                        "/details/provider_reference",
                        "/details/remote_id",
                    ],
                )
            })
        })
        .or_else(|| {
            subject.expected_fact_snapshot.as_ref().and_then(|value| {
                extract_text(
                    value,
                    &[
                        "/connector/reference",
                        "/adapter_execution_reference",
                        "/external_observation_key",
                    ],
                )
            })
        })
        .or_else(|| {
            context.intent.as_ref().and_then(|value| {
                extract_text(value, &["/payload/reference", "/payload/payment_reference"])
            })
        })
}

fn expected_connector_reference(subject: &ReconSubject, context: &ReconContext) -> Option<String> {
    subject
        .expected_fact_snapshot
        .as_ref()
        .and_then(|value| extract_text(value, &["/connector/reference"]))
        .or_else(|| {
            context.latest_receipt.as_ref().and_then(|value| {
                extract_text(
                    value,
                    &[
                        "/connector_outcome/reference",
                        "/recon_linkage/connector_reference",
                    ],
                )
            })
        })
        .or_else(|| {
            context
                .intent
                .as_ref()
                .and_then(|value| extract_text(value, &["/payload/connector_reference"]))
        })
}

fn execution_reference(row: &Value) -> Option<String> {
    extract_text(
        row,
        &["/provider_reference", "/remote_id", "/connector_reference"],
    )
}

fn execution_state(row: &Value) -> &'static str {
    match row
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
    {
        "succeeded" => "succeeded",
        "pending" | "dispatching" => "pending",
        "failed_terminal" | "blocked" => "not_succeeded",
        _ => "unknown",
    }
}

fn provider_state(status: &str) -> &'static str {
    let value = status.trim().to_ascii_lowercase();
    if matches!(
        value.as_str(),
        "success" | "successful" | "completed" | "processed" | "approved"
    ) {
        "succeeded"
    } else if matches!(
        value.as_str(),
        "failed" | "failure" | "rejected" | "reversed" | "abandoned" | "cancelled" | "canceled"
    ) {
        "not_succeeded"
    } else if matches!(
        value.as_str(),
        "pending" | "processing" | "queued" | "received" | "ongoing" | "otp"
    ) {
        "pending"
    } else {
        "unknown"
    }
}

fn execution_error(row: &Value) -> Option<Value> {
    row.get("last_response_json")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| {
            let code = row.get("last_error_code").and_then(Value::as_str)?;
            Some(json!({
                "code": code,
                "message": row.get("last_error_message").and_then(Value::as_str),
            }))
        })
}

fn webhook_reference(row: &Value, data: &Value) -> Option<String> {
    extract_text(
        data,
        &["/reference", "/transaction_reference", "/transfer_code"],
    )
    .or_else(|| extract_text(row, &["/provider_reference", "/remote_id"]))
}

fn webhook_status(data: &Value) -> Option<&str> {
    data.get("status")
        .and_then(Value::as_str)
        .or_else(|| data.get("transfer_status").and_then(Value::as_str))
        .or_else(|| data.get("refund_status").and_then(Value::as_str))
}

fn webhook_error(data: &Value) -> Option<Value> {
    data.get("gateway_response")
        .filter(|value| !value.is_null())
        .cloned()
        .or_else(|| {
            data.get("status_reason")
                .filter(|value| !value.is_null())
                .cloned()
        })
}

fn expected_verification_state(subject: &ReconSubject) -> &'static str {
    if subject.canonical_state.eq_ignore_ascii_case("Succeeded") {
        "succeeded"
    } else if subject
        .canonical_state
        .eq_ignore_ascii_case("FailedTerminal")
        || subject.canonical_state.eq_ignore_ascii_case("Rejected")
        || subject.canonical_state.eq_ignore_ascii_case("DeadLettered")
    {
        "not_succeeded"
    } else {
        "pending"
    }
}

fn expected_window_ms(subject: &ReconSubject) -> u64 {
    match expected_verification_state(subject) {
        "succeeded" => 300_000,
        "not_succeeded" => 120_000,
        _ => 60_000,
    }
}

fn values_match(fact_key: &str, expected: &Value, observed: &Value) -> bool {
    match fact_key {
        "paystack.amount_minor" => expected.as_i64() == observed.as_i64(),
        "paystack.currency"
        | "paystack.execution_reference"
        | "paystack.source_reference"
        | "paystack.destination_reference"
        | "paystack.connector_reference"
        | "paystack.verification_state" => {
            normalize_text(expected.as_str()) == normalize_text(observed.as_str())
        }
        _ => expected == observed,
    }
}

fn mismatch_type(fact_key: &str) -> String {
    match fact_key {
        "paystack.amount_minor" => PaystackMismatchSubcode::AmountMismatch.as_str().to_owned(),
        "paystack.currency" => PaystackMismatchSubcode::CurrencyMismatch
            .as_str()
            .to_owned(),
        "paystack.destination_reference" => PaystackMismatchSubcode::DestinationMismatch
            .as_str()
            .to_owned(),
        "paystack.execution_reference" => PaystackMismatchSubcode::VerificationReferenceMissing
            .as_str()
            .to_owned(),
        "paystack.verification_state" => PaystackMismatchSubcode::PaymentStatusMismatch
            .as_str()
            .to_owned(),
        "paystack.source_reference" => PaystackMismatchSubcode::SourceReferenceMismatch
            .as_str()
            .to_owned(),
        "paystack.connector_reference" => PaystackMismatchSubcode::ConnectorReferenceMismatch
            .as_str()
            .to_owned(),
        _ => "value_mismatch".to_owned(),
    }
}

fn apply_subcode(classification: &mut ReconClassification, subcode: PaystackMismatchSubcode) {
    let value = subcode.as_str().to_owned();
    let mut codes: Vec<String> = classification
        .details
        .get("mismatch_subcodes")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if !codes.iter().any(|code| code == &value) {
        codes.push(value.clone());
    }
    classification
        .details
        .insert("mismatch_subcodes".to_owned(), codes.join(","));
    classification
        .details
        .entry("primary_mismatch_subcode".to_owned())
        .or_insert(value);
}

fn is_pre_execution_state(value: &str) -> bool {
    matches!(value, "Queued" | "Received" | "Validated" | "Leased")
}

fn is_advisory_expected(fact: &ExpectedFactDraft) -> bool {
    fact.derived_from.get("match_mode").and_then(Value::as_str) == Some("advisory")
}

fn is_supplemental_observed(fact: &ObservedFactDraft) -> bool {
    fact.metadata.get("match_mode").and_then(Value::as_str) == Some("supplemental")
}

fn expected_value<'a>(facts: &'a [ExpectedFactDraft], key: &str) -> Option<&'a Value> {
    facts
        .iter()
        .find(|value| value.fact_key == key)
        .map(|value| &value.fact_value)
}

fn observed_value<'a>(facts: &'a [ObservedFactDraft], key: &str) -> Option<&'a Value> {
    facts
        .iter()
        .find(|value| value.fact_key == key)
        .map(|value| &value.fact_value)
}

fn expected_u64(facts: &[ExpectedFactDraft], key: &str) -> Option<u64> {
    expected_value(facts, key).and_then(as_u64)
}

fn observed_u64(facts: &[ObservedFactDraft], key: &str) -> Option<u64> {
    observed_value(facts, key).and_then(as_u64)
}

fn extract_text(value: &Value, pointers: &[&str]) -> Option<String> {
    pointers.iter().find_map(|pointer| {
        value
            .pointer(pointer)
            .and_then(|candidate| match candidate {
                Value::String(text) => {
                    Some(text.trim().to_owned()).filter(|value| !value.is_empty())
                }
                Value::Number(number) => Some(number.to_string()),
                Value::Bool(flag) => Some(flag.to_string()),
                _ => None,
            })
    })
}

fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

fn as_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().map(|value| value.max(0) as u64))
}

fn current_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn subject(state: &str) -> ReconSubject {
        ReconSubject {
            subject_id: "reconsub_test".to_owned(),
            tenant_id: "tenant_demo".to_owned(),
            intent_id: "intent_demo".to_owned(),
            job_id: "job_demo".to_owned(),
            adapter_id: "adapter_paystack".to_owned(),
            canonical_state: state.to_owned(),
            platform_classification: "Success".to_owned(),
            latest_receipt_id: Some("receipt_demo".to_owned()),
            latest_transition_id: None,
            latest_callback_id: None,
            latest_signal_id: Some("signal_demo".to_owned()),
            latest_signal_kind: Some("finalized".to_owned()),
            execution_correlation_id: Some("corr_demo".to_owned()),
            adapter_execution_reference: Some("ref_demo".to_owned()),
            external_observation_key: Some("ref_demo".to_owned()),
            expected_fact_snapshot: Some(
                json!({ "version": 2, "connector": { "reference": "ref_demo" } }),
            ),
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
                    "amount": 2500,
                    "currency": "NGN",
                    "payment_reference": "ref_demo",
                    "destination_reference": "wallet_1"
                }
            })),
            latest_receipt: Some(json!({
                "connector_outcome": { "reference": "ref_demo" },
                "recon_linkage": { "connector_reference": "ref_demo" }
            })),
            ..ReconContext::default()
        }
    }

    #[tokio::test]
    async fn paystack_rule_pack_matches_successful_verification() {
        let pack = PaystackReconRulePack;
        let subject = subject("Succeeded");
        let context = context();
        let expected = pack.build_expected_facts(&subject, &context).await.unwrap();
        let observed = pack
            .collect_observed_facts(
                &subject,
                &context,
                &[json!({
                    "row_kind": "execution",
                    "intent_id": "intent_demo",
                    "status": "succeeded",
                    "provider_reference": "ref_demo",
                    "amount_minor": 2500,
                    "currency": "NGN",
                    "source_reference": "ref_demo",
                    "destination_reference": "wallet_1",
                    "connector_reference": "ref_demo",
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
    async fn paystack_rule_pack_flags_amount_mismatch() {
        let pack = PaystackReconRulePack;
        let subject = subject("Succeeded");
        let context = context();
        let expected = pack.build_expected_facts(&subject, &context).await.unwrap();
        let observed = pack
            .collect_observed_facts(
                &subject,
                &context,
                &[json!({
                    "row_kind": "execution",
                    "intent_id": "intent_demo",
                    "status": "succeeded",
                    "provider_reference": "ref_demo",
                    "amount_minor": 2600,
                    "currency": "NGN",
                    "source_reference": "ref_demo",
                    "destination_reference": "wallet_1",
                    "connector_reference": "ref_demo",
                    "updated_at_ms": 10
                })],
            )
            .await
            .unwrap();
        let matched = pack.match_facts(&subject, &expected, &observed);
        let classification = pack.classify(&subject, &context, &expected, &observed, &matched);
        let emission = pack.emit_recon_result(&subject, &matched, classification);
        assert_eq!(emission.outcome, ReconOutcome::PartiallyMatched);
        assert_eq!(emission.machine_reason, "amount_mismatch");
    }
}
