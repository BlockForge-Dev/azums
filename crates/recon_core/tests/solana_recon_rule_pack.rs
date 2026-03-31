use recon_core::{
    ReconContext, ReconOutcome, ReconRulePack, ReconSubject, SolanaReconRulePack,
};
use serde_json::json;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn subject(state: &str, updated_at_ms: u64) -> ReconSubject {
    ReconSubject {
        subject_id: "reconsub_fixture".to_owned(),
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
        expected_fact_snapshot: Some(json!({ "version": 2 })),
        dirty: true,
        recon_attempt_count: 0,
        recon_retry_count: 0,
        created_at_ms: updated_at_ms.saturating_sub(1_000),
        updated_at_ms,
        scheduled_at_ms: Some(updated_at_ms),
        next_reconcile_after_ms: Some(updated_at_ms),
        last_reconciled_at_ms: None,
        last_recon_error: None,
        last_run_state: None,
    }
}

fn context() -> ReconContext {
    ReconContext {
        intent: Some(json!({
            "kind": "solana.transfer.v1",
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

async fn run_pack(
    subject: &ReconSubject,
    rows: &[serde_json::Value],
) -> (ReconOutcome, String, std::collections::BTreeMap<String, String>) {
    let pack = SolanaReconRulePack;
    let context = context();
    let expected = pack.build_expected_facts(subject, &context).await.unwrap();
    let observed = pack
        .collect_observed_facts(subject, &context, rows)
        .await
        .unwrap();
    let matched = pack.match_facts(subject, &expected, &observed);
    let classification = pack.classify(subject, &context, &expected, &observed, &matched);
    let emission = pack.emit_recon_result(subject, &matched, classification);
    (emission.outcome, emission.machine_reason, emission.details)
}

#[tokio::test]
async fn finalized_success_matches_with_execution_reference() {
    let updated_at_ms = now_ms();
    let subject = subject("Succeeded", updated_at_ms);
    let (outcome, machine_reason, details) = run_pack(
        &subject,
        &[json!({
            "attempt_id": "attempt_1",
            "intent_id": "intent_demo",
            "status": "finalized",
            "signature": "sig_demo",
            "final_signature": "sig_demo",
            "intent_type": "transfer",
            "from_addr": "source_123",
            "to_addr": "dest_123",
            "amount": 42,
            "asset": "SOL",
            "program_id": "system_program",
            "action": "transfer",
            "updated_at_ms": updated_at_ms
        })],
    )
    .await;

    assert_eq!(outcome, ReconOutcome::Matched);
    assert_eq!(machine_reason, "matched");
    assert_eq!(details.get("rule_pack").map(String::as_str), Some("solana.v1"));
}

#[tokio::test]
async fn stale_pending_is_classified_as_pending_too_long() {
    let subject = subject("Succeeded", now_ms().saturating_sub(400_000));
    let (outcome, machine_reason, details) = run_pack(
        &subject,
        &[json!({
            "attempt_id": "attempt_2",
            "intent_id": "intent_demo",
            "status": "sent",
            "intent_type": "transfer",
            "from_addr": "source_123",
            "to_addr": "dest_123",
            "amount": 42,
            "asset": "SOL",
            "program_id": "system_program",
            "action": "transfer",
            "updated_at_ms": subject.updated_at_ms
        })],
    )
    .await;

    assert_eq!(outcome, ReconOutcome::Stale);
    assert_eq!(machine_reason, "pending_too_long");
    assert!(details
        .get("mismatch_subcodes")
        .map(|value| value.contains("pending_too_long"))
        .unwrap_or(false));
}

#[tokio::test]
async fn duplicate_signatures_require_manual_review() {
    let updated_at_ms = now_ms();
    let subject = subject("Succeeded", updated_at_ms);
    let (outcome, machine_reason, _details) = run_pack(
        &subject,
        &[
            json!({
                "attempt_id": "attempt_3",
                "intent_id": "intent_demo",
                "status": "sent",
                "signature": "sig_demo",
                "updated_at_ms": updated_at_ms
            }),
            json!({
                "attempt_id": "attempt_4",
                "intent_id": "intent_demo",
                "status": "sent",
                "signature": "sig_other",
                "updated_at_ms": updated_at_ms
            }),
        ],
    )
    .await;

    assert_eq!(outcome, ReconOutcome::ManualReviewRequired);
    assert_eq!(machine_reason, "duplicate_signal");
}

#[tokio::test]
async fn amount_mismatch_is_partially_matched() {
    let updated_at_ms = now_ms();
    let subject = subject("Succeeded", updated_at_ms);
    let (outcome, machine_reason, _details) = run_pack(
        &subject,
        &[json!({
            "attempt_id": "attempt_5",
            "intent_id": "intent_demo",
            "status": "finalized",
            "signature": "sig_demo",
            "final_signature": "sig_demo",
            "intent_type": "transfer",
            "from_addr": "source_123",
            "to_addr": "dest_123",
            "amount": 99,
            "asset": "SOL",
            "program_id": "system_program",
            "action": "transfer",
            "updated_at_ms": updated_at_ms
        })],
    )
    .await;

    assert_eq!(outcome, ReconOutcome::PartiallyMatched);
    assert_eq!(machine_reason, "amount_mismatch");
}

#[tokio::test]
async fn observed_error_differs_from_expected_success() {
    let updated_at_ms = now_ms();
    let subject = subject("Succeeded", updated_at_ms);
    let (outcome, machine_reason, _details) = run_pack(
        &subject,
        &[json!({
            "attempt_id": "attempt_6",
            "intent_id": "intent_demo",
            "status": "finalized",
            "signature": "sig_demo",
            "final_signature": "sig_demo",
            "intent_type": "transfer",
            "from_addr": "source_123",
            "to_addr": "dest_123",
            "amount": 42,
            "asset": "SOL",
            "program_id": "system_program",
            "action": "transfer",
            "final_err_json": {
                "InstructionError": [0, "Custom"]
            },
            "updated_at_ms": updated_at_ms
        })],
    )
    .await;

    assert_eq!(outcome, ReconOutcome::Unmatched);
    assert_eq!(machine_reason, "observed_error_differs_from_expected");
}
