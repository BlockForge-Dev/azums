use serde_json::json;
use std::{fs, process::Command};

#[test]
fn m15_perf_guard_passes_matching_reports_and_fails_meaningful_regressions() -> anyhow::Result<()> {
    let exe = env!("CARGO_BIN_EXE_azums-perf-guard");
    let dir = std::env::temp_dir().join(format!("azums-m15-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir)?;

    let baseline = report(1000.0, 1.0, 2.0, Some(1000), Some(10_000));
    let same = report(1000.0, 1.0, 2.0, Some(1000), Some(10_000));
    let regressed = report(940.0, 1.07, 2.20, Some(1110), Some(11_100));

    let baseline_path = dir.join("baseline.json");
    let same_path = dir.join("same.json");
    let regressed_path = dir.join("regressed.json");
    fs::write(&baseline_path, serde_json::to_vec_pretty(&baseline)?)?;
    fs::write(&same_path, serde_json::to_vec_pretty(&same)?)?;
    fs::write(&regressed_path, serde_json::to_vec_pretty(&regressed)?)?;

    let ok = Command::new(exe)
        .arg(&baseline_path)
        .arg(&same_path)
        .status()?;
    assert!(ok.success(), "matching perf reports should pass");

    let failed = Command::new(exe)
        .arg(&baseline_path)
        .arg(&regressed_path)
        .status()?;
    assert!(
        !failed.success(),
        "meaningful throughput, latency, allocation, and memory regressions should fail"
    );

    let _ = fs::remove_dir_all(dir);
    Ok(())
}

fn report(
    throughput: f64,
    p50_ms: f64,
    p99_ms: f64,
    allocations: Option<u64>,
    ram_bytes: Option<u64>,
) -> serde_json::Value {
    json!({
        "results": [{
            "backend": "memory",
            "workload": "small_jobs",
            "workers": 4,
            "throughput_jobs_per_sec": throughput,
            "latency": {
                "p50_ms": p50_ms,
                "p95_ms": p99_ms,
                "p99_ms": p99_ms,
                "p999_ms": p99_ms
            },
            "resources": {
                "wall_ms": 1.0,
                "cpu": null,
                "ram_bytes": ram_bytes,
                "allocations": allocations,
                "disk_io_bytes": null,
                "network_io_bytes": null,
                "notes": []
            }
        }]
    })
}
