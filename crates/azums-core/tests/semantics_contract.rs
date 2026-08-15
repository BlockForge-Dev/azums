use azums_core::{
    semantic_contract, BackendCapabilities, ConsumerGroupCapability, DurabilityCapability,
    NotificationCapability, RetentionCapability, SemanticBehavior, SemanticClassification,
    TransactionalEnqueueCapability,
};
use std::collections::HashSet;

#[test]
fn every_semantic_behavior_has_one_complete_contract() {
    let mut seen = HashSet::new();

    for behavior in SemanticBehavior::ALL {
        assert!(
            seen.insert(behavior),
            "duplicate semantic behavior: {behavior:?}"
        );
        let contract = semantic_contract(behavior);
        assert_eq!(contract.behavior, behavior);
        assert!(!contract.contract.trim().is_empty());

        if contract.classification == SemanticClassification::Unspecified {
            assert!(
                contract.supported_alternative.is_some(),
                "non-guarantee {behavior:?} must name a supported alternative"
            );
        }
    }

    assert_eq!(seen.len(), SemanticBehavior::ALL.len());
}

#[test]
fn requested_backend_boundaries_are_machine_readable() {
    use SemanticBehavior::*;
    use SemanticClassification::BackendDependent;

    for behavior in [
        Durability,
        TransactionalEnqueue,
        DistributedWorkers,
        NotificationDelivery,
        WakeUpLatency,
        Backpressure,
        StreamRetention,
    ] {
        assert_eq!(semantic_contract(behavior).classification, BackendDependent);
    }

    let memory = BackendCapabilities::memory();
    let memory = memory.semantics().expect("built-in memory profile");
    assert_eq!(memory.durability, DurabilityCapability::ProcessLocal);
    assert_eq!(memory.job_retention, RetentionCapability::ProcessLifetime);
    assert_eq!(
        memory.notification_delivery,
        NotificationCapability::ProcessLocalHint
    );

    let sqlite = BackendCapabilities::sqlite();
    let sqlite = sqlite.semantics().expect("built-in SQLite profile");
    assert_eq!(sqlite.durability, DurabilityCapability::Persistent);
    assert_eq!(
        sqlite.transactional_enqueue_scope,
        TransactionalEnqueueCapability::SameDatabase
    );

    let postgres = BackendCapabilities::postgres();
    let postgres = postgres.semantics().expect("built-in PostgreSQL profile");
    assert_eq!(
        postgres.consumer_group_coordination,
        ConsumerGroupCapability::OffsetsOnly
    );
    assert_eq!(
        postgres.stream_retention,
        RetentionCapability::ExplicitPruning
    );

    let redis = BackendCapabilities::redis();
    let redis = redis.semantics().expect("built-in Redis profile");
    assert_eq!(
        redis.durability,
        DurabilityCapability::ConfigurationDependent
    );
    assert_eq!(
        redis.stream_retention,
        RetentionCapability::BackendConfigured
    );
    assert_eq!(
        redis.transactional_enqueue_scope,
        TransactionalEnqueueCapability::BackendOperationOnly
    );
}

#[test]
fn novel_custom_backend_combinations_are_not_guessed() {
    let mut custom = BackendCapabilities::memory();
    custom.notifications = false;
    assert_eq!(custom.semantics(), None);
}

#[test]
fn impossible_distributed_system_claims_remain_explicit_non_guarantees() {
    use SemanticBehavior::*;
    use SemanticClassification::Unspecified;

    for behavior in [
        ExactlyOnceExecution,
        ExactlyOnceExternalSideEffects,
        ExactRunAtExecution,
        CompletionOrdering,
        GlobalOrdering,
        WorkerFairness,
        ArbitraryExternalTransactions,
        PermanentRetention,
        AutomaticScaling,
        ConsumerGroupWorkBalancing,
    ] {
        let contract = semantic_contract(behavior);
        assert_eq!(contract.classification, Unspecified);
        assert!(contract.supported_alternative.is_some());
    }
}
