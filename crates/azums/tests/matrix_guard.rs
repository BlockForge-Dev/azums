use std::fs;

#[tokio::test]
async fn matrix_file_exists_and_mentions_all_laws() {
    // When running `cargo test -p azums`, the working dir is often `crates/azums`.
    // When running from repo root, it's `tests/MATRIX.md` or `crates/azums/tests/MATRIX.md`.
    let candidates = ["tests/MATRIX.md", "crates/azums/tests/MATRIX.md"];
    let s = candidates
        .iter()
        .find_map(|p| fs::read_to_string(p).ok())
        .unwrap_or_else(|| {
            panic!("MATRIX.md missing: create crates/azums/tests/MATRIX.md (and/or tests/MATRIX.md when running from crate dir)");
        });

    for needle in ["Law 1", "Law 2", "Law 3", "Law 4", "Law 5"] {
        assert!(s.contains(needle), "MATRIX.md missing section: {needle}");
    }

    assert!(
        s.contains("Reliability"),
        "MATRIX.md should include Reliability section"
    );
    assert!(
        s.contains("Load"),
        "MATRIX.md should include Load & Cost section"
    );
    assert!(
        s.contains("tests/"),
        "MATRIX.md should reference real test files (tests/...)"
    );
}
