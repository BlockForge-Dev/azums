#[test]
fn m19_production_docs_cover_required_handoff_topics() {
    let readiness = include_str!("../../../docs/src/production_readiness.md");
    let deployment = include_str!("../../../docs/src/production_deployment.md");
    let runbook = include_str!("../../../docs/src/failure_recovery_runbook.md");
    let readiness_lower = readiness.to_lowercase();
    let deployment_lower = deployment.to_lowercase();
    let runbook_lower = runbook.to_lowercase();

    for required in [
        "Security Audit",
        "Dependency audit",
        "Unsafe code",
        "Serialization safety",
        "Secrets handling",
        "Authorization boundaries",
        "Payload limits",
        "Resource exhaustion",
        "Reliability Audit",
        "Recovery",
        "Graceful shutdown",
        "Database failures",
        "Worker failures",
        "Network failures",
        "Operations Audit",
        "Migrations",
        "Upgrade paths",
        "Rollback",
        "Compatibility",
        "Configuration validation",
    ] {
        assert!(
            readiness_lower.contains(&required.to_lowercase()),
            "production readiness audit must cover {required}"
        );
    }

    for required in [
        "Choose The Backend",
        "Configure Runtime",
        "Run Migrations",
        "Deploy Workers",
        "Protect Admin Access",
        "Observe The System",
        "Release And Roll Back",
    ] {
        assert!(
            deployment_lower.contains(&required.to_lowercase()),
            "deployment guide must cover {required}"
        );
    }

    for required in [
        "First Five Minutes",
        "Queue Depth Growing",
        "Jobs Stuck Running",
        "DLQ Spike",
        "Database Failure",
        "Redis Disconnect Or Restart",
        "Bad Migration Or Incompatible Upgrade",
        "Suspected Duplicate Side Effect",
        "Recovery Rule",
    ] {
        assert!(
            runbook_lower.contains(&required.to_lowercase()),
            "runbook must cover {required}"
        );
    }
}
