#[test]
fn m21_stable_release_gate_defines_1_0_as_semantics_stability() {
    let gate = include_str!("../../../docs/src/stable_release.md");

    for required in [
        "Azums 1.0 is not a feature-count release",
        "Current status: **1.0 is not declared by this document.**",
        "Stable Semantics",
        "Stable API Surface",
        "Backend Boundary",
        "Required Release Gates",
        "Release Blockers",
        "Stable Release Declaration",
        "Backend-dependent behavior remains governed by BackendCapabilities",
        "Unspecified behavior remains outside the compatibility contract",
    ] {
        assert!(
            gate.contains(required),
            "stable release gate must include {required}"
        );
    }
}

#[test]
fn m21_stable_release_gate_names_core_semantics_that_must_remain_stable() {
    let gate = include_str!("../../../docs/src/stable_release.md");

    for semantic in [
        "at-least-once job execution",
        "does not guarantee exactly-once external side effects",
        "Invalid lifecycle transitions",
        "Attempt history",
        "expired leases",
        "Retry and DLQ",
        "idempotency_key",
        "Transactional enqueue",
        "eligibility time",
        "Stream append",
        "Replay creates new work",
        "Cancellation",
        "Memory, SQLite, PostgreSQL, and Redis",
    ] {
        assert!(
            gate.contains(semantic),
            "stable release gate must classify {semantic}"
        );
    }
}

#[test]
fn m21_stable_release_gate_is_linked_from_the_architecture_book_summary() {
    let summary = include_str!("../../../docs/src/SUMMARY.md");

    assert!(
        summary.contains("M21 Stable Release Gate"),
        "architecture book summary must link the M21 gate"
    );
}
