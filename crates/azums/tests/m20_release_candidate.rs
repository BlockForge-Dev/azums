#[test]
fn m20_release_candidate_report_maps_guarantees_to_tests_and_status() {
    let report = include_str!("../../../docs/src/release_candidate.md");

    for required in [
        "Full test suite",
        "Full integration suite",
        "Full chaos suite",
        "Full fuzz suite",
        "Full property suite",
        "Full benchmark suite",
        "Documentation build",
        "Dependency audit",
        "API compatibility checks",
        "Guarantee To Test Matrix",
        "No known violation of a documented guarantee",
    ] {
        assert!(
            report.contains(required),
            "release candidate report must include {required}"
        );
    }

    assert!(
        report.contains("PASS"),
        "release candidate report must include passing gates"
    );
    assert!(
        !report.contains("BLOCKED"),
        "release candidate report should not retain old blocked gates after rerun"
    );
}
