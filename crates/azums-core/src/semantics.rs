use serde::{Deserialize, Serialize};

/// A public Azums behavior whose contract is stable and explicitly classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticBehavior {
    AtLeastOnceExecution,
    RunAtEligibility,
    LeaseExclusivity,
    CrashRecoveryAfterLeaseExpiry,
    MonotonicConsumerGroupOffsets,
    SafeStreamPruning,
    Durability,
    TransactionalEnqueue,
    DistributedWorkers,
    NotificationDelivery,
    WakeUpLatency,
    Backpressure,
    StreamRetention,
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
}

impl SemanticBehavior {
    /// Exhaustive inventory used by contract tests and documentation tooling.
    pub const ALL: [Self; 23] = [
        Self::AtLeastOnceExecution,
        Self::RunAtEligibility,
        Self::LeaseExclusivity,
        Self::CrashRecoveryAfterLeaseExpiry,
        Self::MonotonicConsumerGroupOffsets,
        Self::SafeStreamPruning,
        Self::Durability,
        Self::TransactionalEnqueue,
        Self::DistributedWorkers,
        Self::NotificationDelivery,
        Self::WakeUpLatency,
        Self::Backpressure,
        Self::StreamRetention,
        Self::ExactlyOnceExecution,
        Self::ExactlyOnceExternalSideEffects,
        Self::ExactRunAtExecution,
        Self::CompletionOrdering,
        Self::GlobalOrdering,
        Self::WorkerFairness,
        Self::ArbitraryExternalTransactions,
        Self::PermanentRetention,
        Self::AutomaticScaling,
        Self::ConsumerGroupWorkBalancing,
    ];
}

/// Stability class of a documented Azums behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticClassification {
    Guaranteed,
    BackendDependent,
    Unspecified,
}

/// Machine-readable answer to "what does Azums guarantee for this behavior?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticContract {
    pub behavior: SemanticBehavior,
    pub classification: SemanticClassification,
    pub contract: &'static str,
    pub supported_alternative: Option<&'static str>,
}

/// Returns the canonical product contract for every public semantic behavior.
pub const fn semantic_contract(behavior: SemanticBehavior) -> SemanticContract {
    use SemanticBehavior::*;
    use SemanticClassification::*;

    let (classification, contract, supported_alternative) = match behavior {
        AtLeastOnceExecution => (Guaranteed, "A committed runnable job may execute more than once but is not silently discarded.", None),
        RunAtEligibility => (Guaranteed, "A scheduled job is not eligible for leasing before its backend clock reaches run_at.", None),
        LeaseExclusivity => (Guaranteed, "A job has at most one unexpired worker lease at a time.", None),
        CrashRecoveryAfterLeaseExpiry => (Guaranteed, "Abandoned work becomes runnable after lease expiry and recovery.", None),
        MonotonicConsumerGroupOffsets => (Guaranteed, "Acknowledgement never moves a consumer-group offset backward.", None),
        SafeStreamPruning => (Guaranteed, "Explicit pruning does not pass the lowest known consumer-group offset.", None),
        Durability => (BackendDependent, "Persistence strength is declared by StorageBackend::semantic_capabilities().durability.", None),
        TransactionalEnqueue => (BackendDependent, "Atomicity scope is declared by StorageBackend::semantic_capabilities().transactional_enqueue_scope.", None),
        DistributedWorkers => (BackendDependent, "Multi-process lease coordination requires distributed_workers = true.", None),
        NotificationDelivery => (BackendDependent, "Notifications are hints with behavior declared by StorageBackend::semantic_capabilities().notification_delivery.", Some("Read or lease durable backend state after every wake-up and on a polling fallback.")),
        WakeUpLatency => (BackendDependent, "Azums provides no backend-independent notification latency bound.", Some("Configure worker polling and measure queue claim latency.")),
        Backpressure => (BackendDependent, "Overload behavior is declared by BackendCapabilities::backpressure.", None),
        StreamRetention => (BackendDependent, "Retention is declared by StorageBackend::semantic_capabilities().stream_retention and backend configuration.", None),
        ExactlyOnceExecution => (Unspecified, "Handlers may execute more than once after retries, crashes, lease expiry, or replay.", Some("Use idempotent handlers and an application deduplication record.")),
        ExactlyOnceExternalSideEffects => (Unspecified, "Azums cannot atomically control arbitrary external side effects.", Some("Use provider idempotency keys or a transactional outbox in the same database.")),
        ExactRunAtExecution => (Unspecified, "run_at is an eligibility boundary, not an exact execution timestamp.", Some("Use deadline_at for a latest-start bound and observe scheduling latency.")),
        CompletionOrdering => (Unspecified, "Parallel workers may complete jobs in any order.", Some("Use one worker with batch size one when serial completion is required.")),
        GlobalOrdering => (Unspecified, "Azums defines no total order across queues or streams.", Some("Route related work through one FIFO queue or one stream.")),
        WorkerFairness => (Unspecified, "Azums does not guarantee equal or fair work distribution among workers.", Some("Observe worker throughput and enforce deployment-level limits.")),
        ArbitraryExternalTransactions => (Unspecified, "Azums does not provide atomic transactions across unrelated services.", Some("Use same-database enqueue, an outbox, or an application saga.")),
        PermanentRetention => (Unspecified, "No backend can promise retention independent of operator deletion, media loss, or configuration.", Some("Use backups, replication, archival, and explicit retention policy.")),
        AutomaticScaling => (Unspecified, "Azums does not provision or remove worker processes.", Some("Scale from exported queue-depth and latency metrics.")),
        ConsumerGroupWorkBalancing => (Unspecified, "Consumer groups persist offsets but do not assign events to members.", Some("Add application-owned partition assignment or use distinct groups.")),
    };

    SemanticContract {
        behavior,
        classification,
        contract,
        supported_alternative,
    }
}
