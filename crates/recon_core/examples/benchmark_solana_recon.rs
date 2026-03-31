use recon_core::{ReconContext, ReconRulePack, ReconSubject, SolanaReconRulePack};
use serde_json::json;
use std::time::Instant;

fn subject(now_ms: u64) -> ReconSubject {
    ReconSubject {
        subject_id: "bench_subject".to_owned(),
        tenant_id: "tenant_demo".to_owned(),
        intent_id: "intent_demo".to_owned(),
        job_id: "job_demo".to_owned(),
        adapter_id: "adapter_solana".to_owned(),
        canonical_state: "Succeeded".to_owned(),
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
        created_at_ms: now_ms.saturating_sub(1_000),
        updated_at_ms: now_ms,
        scheduled_at_ms: Some(now_ms),
        next_reconcile_after_ms: Some(now_ms),
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

fn observation_rows(now_ms: u64) -> Vec<serde_json::Value> {
    vec![json!({
        "attempt_id": "attempt_bench",
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
        "updated_at_ms": now_ms
    })]
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let iterations = std::env::var("RECON_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1_000);
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0);
    let subject = subject(now_ms);
    let context = context();
    let rows = observation_rows(now_ms);
    let pack = SolanaReconRulePack;

    let started = Instant::now();
    for _ in 0..iterations {
        let expected = pack.build_expected_facts(&subject, &context).await.unwrap();
        let observed = pack
            .collect_observed_facts(&subject, &context, &rows)
            .await
            .unwrap();
        let matched = pack.match_facts(&subject, &expected, &observed);
        let classification = pack.classify(&subject, &context, &expected, &observed, &matched);
        let _ = pack.emit_recon_result(&subject, &matched, classification);
    }
    let elapsed = started.elapsed();
    let per_iteration_us = (elapsed.as_secs_f64() * 1_000_000.0) / iterations as f64;

    println!(
        "solana_recon_benchmark iterations={} total_ms={:.3} avg_us={:.3}",
        iterations,
        elapsed.as_secs_f64() * 1_000.0,
        per_iteration_us
    );
}
