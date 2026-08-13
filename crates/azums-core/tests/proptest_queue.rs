use azums_core::{MemoryBackend, NewJob, StorageBackend};
use chrono::Utc;
use proptest::prelude::*;
use serde_json::json;

#[derive(Debug, Clone)]
enum QueueOp {
    Enqueue(String, u32),
    LeaseAndComplete,
    LeaseAndFail,
}

fn queue_op_strategy() -> impl Strategy<Value = QueueOp> {
    prop_oneof![
        (any::<String>(), any::<u32>()).prop_map(|(name, val)| QueueOp::Enqueue(name, val)),
        Just(QueueOp::LeaseAndComplete),
        Just(QueueOp::LeaseAndFail),
    ]
}

proptest! {
    #[test]
    fn prop_test_queue_state_invariants(ops in prop::collection::vec(queue_op_strategy(), 1..50)) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let backend = MemoryBackend::new();
            let mut total_enqueued = 0;
            let mut total_succeeded = 0;
            let mut total_dlq = 0;

            for op in ops {
                match op {
                    QueueOp::Enqueue(job_type, val) => {
                        let new_job = NewJob {
                            queue: "default".to_string(),
                            job_type,
                            payload_json: json!({"val": val}),
                            idempotency_key: None,
                            run_at: Utc::now(),
                            priority: 0,
                            max_attempts: 1, // Fail immediately goes to DLQ
                        };
                        backend.enqueue(new_job).await.unwrap();
                        total_enqueued += 1;
                    }
                    QueueOp::LeaseAndComplete => {
                        let leased = backend.dequeue_and_lease("default", "w1", 10, 1).await.unwrap();
                        if let Some(job) = leased.first() {
                            let attempts = backend.start_attempts_batch(&["default".to_string()], &[job.id], "w1").await.unwrap();
                            if let Some((jid, att_id, _)) = attempts.first() {
                                backend.complete_job(*jid, *att_id, "w1", 5).await.unwrap();
                                total_succeeded += 1;
                            }
                        }
                    }
                    QueueOp::LeaseAndFail => {
                        let leased = backend.dequeue_and_lease("default", "w1", 10, 1).await.unwrap();
                        if let Some(job) = leased.first() {
                            let attempts = backend.start_attempts_batch(&["default".to_string()], &[job.id], "w1").await.unwrap();
                            if let Some((jid, att_id, att_no)) = attempts.first() {
                                backend.fail_job(*jid, *att_id, "w1", 5, "PROP_TEST_FAIL", "ERR", "msg", *att_no).await.unwrap();
                                total_dlq += 1;
                            }
                        }
                    }
                }

                // Invariant Check: Total enqueued == Succeeded + DLQ + (Remaining in RAM)
                let remaining = backend.list_jobs(None, None, 100, None, None).await.unwrap();
                let queued_count = remaining.iter().filter(|j| j.status == "queued").count();
                let running_count = remaining.iter().filter(|j| j.status == "running").count();

                assert_eq!(
                    total_enqueued,
                    total_succeeded + total_dlq + queued_count + running_count,
                    "State invariant violated! Enqueued: {total_enqueued}, Succeeded: {total_succeeded}, DLQ: {total_dlq}, Queued: {queued_count}, Running: {running_count}"
                );
            }
        });
    }
}
