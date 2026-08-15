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

#[test]
fn m15_perf_guard_uses_worker_medians_to_reject_noise() -> anyhow::Result<()> {
    let exe = env!("CARGO_BIN_EXE_azums-perf-guard");
    let dir = std::env::temp_dir().join(format!("azums-m15-median-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir)?;

    let baseline = worker_report(&[1000.0; 6], &[1.0; 6], &[2.0; 6]);
    let one_noisy_worker = worker_report(
        &[500.0, 1000.0, 1000.0, 1000.0, 1000.0, 1000.0],
        &[2.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        &[4.0, 2.0, 2.0, 2.0, 2.0, 2.0],
    );
    let workload_regression = worker_report(&[900.0; 6], &[1.10; 6], &[2.20; 6]);
    let tail_only_observation = worker_report(&[1000.0; 6], &[1.0; 6], &[3.0; 6]);

    let baseline_path = dir.join("baseline.json");
    let noisy_path = dir.join("noisy.json");
    let regressed_path = dir.join("regressed.json");
    let tail_only_path = dir.join("tail-only.json");
    fs::write(&baseline_path, serde_json::to_vec_pretty(&baseline)?)?;
    fs::write(&noisy_path, serde_json::to_vec_pretty(&one_noisy_worker)?)?;
    fs::write(
        &regressed_path,
        serde_json::to_vec_pretty(&workload_regression)?,
    )?;
    fs::write(
        &tail_only_path,
        serde_json::to_vec_pretty(&tail_only_observation)?,
    )?;

    assert!(
        Command::new(exe)
            .arg(&baseline_path)
            .arg(&noisy_path)
            .status()?
            .success(),
        "one noisy worker sample must not fail the workload median"
    );
    assert!(
        !Command::new(exe)
            .arg(&baseline_path)
            .arg(&regressed_path)
            .status()?
            .success(),
        "a workload-wide median regression must fail"
    );
    assert!(
        Command::new(exe)
            .arg(&baseline_path)
            .arg(&tail_only_path)
            .status()?
            .success(),
        "one latency percentile without p50 confirmation must remain an observation"
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

fn worker_report(throughput: &[f64; 6], p50_ms: &[f64; 6], p99_ms: &[f64; 6]) -> serde_json::Value {
    let workers = [1, 2, 4, 8, 16, 32];
    let results = workers
        .into_iter()
        .enumerate()
        .map(|(index, workers)| {
            json!({
                "backend": "memory",
                "workload": "small_jobs",
                "workers": workers,
                "throughput_jobs_per_sec": throughput[index],
                "latency": {
                    "p50_ms": p50_ms[index],
                    "p95_ms": p99_ms[index],
                    "p99_ms": p99_ms[index],
                    "p999_ms": p99_ms[index]
                },
                "resources": {
                    "wall_ms": 1.0,
                    "cpu": null,
                    "ram_bytes": null,
                    "allocations": null,
                    "disk_io_bytes": null,
                    "network_io_bytes": null,
                    "notes": []
                }
            })
        })
        .collect::<Vec<_>>();
    json!({ "results": results })
}
