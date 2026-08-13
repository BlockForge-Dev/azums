use std::process::Command;

#[test]
fn m14_perf_binary_emits_reproducible_reports() -> anyhow::Result<()> {
    let exe = env!("CARGO_BIN_EXE_azums-perf");
    let output_dir = std::env::temp_dir().join(format!("azums-m14-{}", uuid::Uuid::new_v4()));

    let status = Command::new(exe)
        .env("AZUMS_PERF_BACKENDS", "memory")
        .env("AZUMS_PERF_JOBS", "8")
        .env("AZUMS_PERF_ITERATIONS", "1")
        .env("AZUMS_PERF_OUTPUT_DIR", &output_dir)
        .status()?;

    assert!(status.success(), "azums-perf smoke run failed");

    let json_path = output_dir.join("m14_report.json");
    let markdown_path = output_dir.join("m14_report.md");
    assert!(json_path.exists(), "missing JSON benchmark report");
    assert!(markdown_path.exists(), "missing Markdown benchmark report");

    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(&json_path)?)?;
    let results = report
        .get("results")
        .and_then(|value| value.as_array())
        .expect("results must be a JSON array");
    assert!(
        !results.is_empty(),
        "benchmark smoke run should produce scenario results"
    );
    assert!(
        results.iter().all(|result| result
            .get("latency")
            .and_then(|latency| latency.get("p99_ms"))
            .is_some()),
        "every scenario must include p99 latency"
    );

    let _ = std::fs::remove_dir_all(output_dir);
    Ok(())
}
