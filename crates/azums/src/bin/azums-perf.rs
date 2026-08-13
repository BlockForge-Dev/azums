use azums::{
    make_sqlite_pool, quickstart, Job, NewJob, QuickstartFlow, SqliteBackend, StorageBackend,
};
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use std::{
    env,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{fs, task::JoinSet};
use uuid::Uuid;

const WORKERS: [usize; 6] = [1, 2, 4, 8, 16, 32];

#[derive(Debug, Clone, Copy)]
enum Workload {
    SmallJobs,
    LargePayloads,
    BatchJobs,
    MixedPriorities,
    HighContention,
    IdleQueue,
}

impl Workload {
    fn all() -> &'static [Workload] {
        &[
            Workload::SmallJobs,
            Workload::LargePayloads,
            Workload::BatchJobs,
            Workload::MixedPriorities,
            Workload::HighContention,
            Workload::IdleQueue,
        ]
    }

    fn as_str(self) -> &'static str {
        match self {
            Workload::SmallJobs => "small_jobs",
            Workload::LargePayloads => "large_payloads",
            Workload::BatchJobs => "batch_jobs",
            Workload::MixedPriorities => "mixed_priorities",
            Workload::HighContention => "high_contention",
            Workload::IdleQueue => "idle_queue",
        }
    }
}

#[derive(Debug, Serialize)]
struct PerfReport {
    generated_at_unix_ms: u128,
    config: PerfConfig,
    environment: EnvironmentReport,
    results: Vec<ScenarioReport>,
}

#[derive(Debug, Clone, Serialize)]
struct PerfConfig {
    jobs_per_scenario: usize,
    iterations: usize,
    batch_size: i64,
    backends: Vec<String>,
    output_dir: String,
}

#[derive(Debug, Serialize)]
struct EnvironmentReport {
    os: String,
    arch: String,
    rust_profile: String,
    cpu_count_hint: usize,
    resource_notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ScenarioReport {
    backend: String,
    workload: String,
    workers: usize,
    iterations: usize,
    jobs: usize,
    throughput_jobs_per_sec: f64,
    latency: LatencyReport,
    resources: ResourceReport,
    notes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LatencyReport {
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    p999_ms: f64,
}

#[derive(Debug, Serialize)]
struct ResourceReport {
    wall_ms: f64,
    cpu: Option<String>,
    ram_bytes: Option<u64>,
    allocations: Option<u64>,
    disk_io_bytes: Option<u64>,
    network_io_bytes: Option<u64>,
    notes: Vec<String>,
}

struct BenchFlow {
    flow: QuickstartFlow,
    queue: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let config = PerfConfig::from_env();
    fs::create_dir_all(&config.output_dir).await?;

    let mut results = Vec::new();
    for backend in &config.backends {
        for workload in Workload::all() {
            for workers in WORKERS {
                match run_scenario(backend, *workload, workers, &config).await {
                    Ok(report) => {
                        println!(
                            "BENCH_RESULT backend={} workload={} workers={} throughput={:.2} jobs/sec p99={:.4}ms",
                            report.backend,
                            report.workload,
                            report.workers,
                            report.throughput_jobs_per_sec,
                            report.latency.p99_ms
                        );
                        results.push(report);
                    }
                    Err(err) => {
                        eprintln!(
                            "BENCH_SKIPPED backend={backend} workload={} workers={workers} reason={err:#}",
                            workload.as_str()
                        );
                    }
                }
            }
        }
    }

    let report = PerfReport {
        generated_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        config: config.clone(),
        environment: EnvironmentReport::capture(),
        results,
    };

    let json_path = PathBuf::from(&config.output_dir).join("m14_report.json");
    let md_path = PathBuf::from(&config.output_dir).join("m14_report.md");
    fs::write(&json_path, serde_json::to_vec_pretty(&report)?).await?;
    fs::write(&md_path, render_markdown(&report)).await?;

    println!("Wrote {}", json_path.display());
    println!("Wrote {}", md_path.display());
    Ok(())
}

async fn run_scenario(
    backend: &str,
    workload: Workload,
    workers: usize,
    config: &PerfConfig,
) -> anyhow::Result<ScenarioReport> {
    let mut iteration_latencies = Vec::with_capacity(config.iterations);
    let mut total_jobs = 0usize;
    let started = Instant::now();
    let mut notes = Vec::new();

    for iteration in 0..config.iterations {
        let bench_flow = make_flow(backend, workload, iteration).await?;
        let job_count = if matches!(workload, Workload::IdleQueue) {
            0
        } else {
            config.jobs_per_scenario
        };

        enqueue_workload(&bench_flow, workload, job_count).await?;
        let iter_started = Instant::now();
        process_workload(&bench_flow, workers, config.batch_size, job_count).await?;
        iteration_latencies.push(iter_started.elapsed());
        total_jobs += job_count;
    }

    if total_jobs == 0 {
        notes
            .push("idle queue measures empty lease latency rather than job throughput".to_string());
    }

    let wall = started.elapsed();
    let throughput = if total_jobs == 0 {
        0.0
    } else {
        total_jobs as f64 / wall.as_secs_f64()
    };

    Ok(ScenarioReport {
        backend: backend.to_string(),
        workload: workload.as_str().to_string(),
        workers,
        iterations: config.iterations,
        jobs: total_jobs,
        throughput_jobs_per_sec: throughput,
        latency: LatencyReport::from_durations(&iteration_latencies),
        resources: ResourceReport::from_wall(wall),
        notes,
    })
}

async fn make_flow(
    backend: &str,
    workload: Workload,
    iteration: usize,
) -> anyhow::Result<BenchFlow> {
    let queue = format!("m14-{}-{}-{}", backend, workload.as_str(), Uuid::new_v4());
    let flow = match backend {
        "memory" => quickstart("memory").await?.with_queue(queue.clone()),
        "sqlite" => {
            let db_url = format!(
                "sqlite://file:m14_{}_{}?mode=memory&cache=shared",
                iteration,
                Uuid::new_v4()
            );
            let pool = make_sqlite_pool(&db_url).await?;
            let backend = SqliteBackend::new(pool);
            backend.run_migrations().await?;
            QuickstartFlow::new(Arc::new(backend)).with_queue(queue.clone())
        }
        "postgres" => {
            let url = env::var("DATABASE_URL").or_else(|_| env::var("TEST_DATABASE_URL"))?;
            quickstart(&url).await?.with_queue(queue.clone())
        }
        "redis" => {
            let url = env::var("REDIS_URL")?;
            quickstart(&url).await?.with_queue(queue.clone())
        }
        other => anyhow::bail!("unsupported backend '{other}'"),
    };

    Ok(BenchFlow { flow, queue })
}

async fn enqueue_workload(
    bench_flow: &BenchFlow,
    workload: Workload,
    job_count: usize,
) -> anyhow::Result<()> {
    let payload = match workload {
        Workload::LargePayloads => json!({
            "blob": "x".repeat(16 * 1024),
            "created_at": Utc::now().to_rfc3339(),
        }),
        _ => json!({ "x": 1 }),
    };

    let mut batch = Vec::with_capacity(job_count.min(512));
    for idx in 0..job_count {
        let mut job = Job::new("m14_job", payload.clone()).queue(&bench_flow.queue);
        if matches!(workload, Workload::MixedPriorities) {
            job = job.priority((idx % 17) as i32 - 8);
        }
        if matches!(workload, Workload::HighContention) {
            job = job.idempotency_key(format!("m14-contention-{idx}"));
        }

        if matches!(workload, Workload::BatchJobs) {
            batch.push(job);
            if batch.len() >= 512 {
                let jobs: Vec<NewJob> = batch.drain(..).map(Into::into).collect();
                bench_flow.flow.enqueue_batch(jobs).await?;
            }
        } else {
            bench_flow.flow.enqueue(job).await?;
        }
    }

    if !batch.is_empty() {
        let jobs: Vec<NewJob> = batch.into_iter().map(Into::into).collect();
        bench_flow.flow.enqueue_batch(jobs).await?;
    }

    Ok(())
}

async fn process_workload(
    bench_flow: &BenchFlow,
    workers: usize,
    batch_size: i64,
    job_count: usize,
) -> anyhow::Result<()> {
    if job_count == 0 {
        for worker_idx in 0..workers {
            let worker = format!("m14-worker-{worker_idx}");
            let leased = bench_flow
                .flow
                .backend()
                .lease_jobs_batch(&bench_flow.queue, &worker, 30, batch_size)
                .await?;
            assert!(leased.is_empty(), "idle queue returned work");
        }
        return Ok(());
    }

    let mut handles = JoinSet::new();
    for worker_idx in 0..workers {
        let backend = bench_flow.flow.backend().clone();
        let queue = bench_flow.queue.clone();
        handles.spawn(async move {
            let worker = format!("m14-worker-{worker_idx}");
            let mut processed = 0usize;
            loop {
                let leased = backend
                    .lease_jobs_batch(&queue, &worker, 30, batch_size)
                    .await?;
                if leased.is_empty() {
                    break;
                }

                let dataset_ids: Vec<String> =
                    leased.iter().map(|job| job.dataset_id.clone()).collect();
                let job_ids: Vec<Uuid> = leased.iter().map(|job| job.id).collect();
                let attempts = backend
                    .start_attempts_batch(&dataset_ids, &job_ids, &worker)
                    .await?;
                for (job_id, attempt_id, _) in attempts {
                    backend
                        .mark_succeeded(job_id, attempt_id, &worker, 0)
                        .await?;
                    processed += 1;
                }
            }
            anyhow::Ok(processed)
        });
    }

    let mut processed = 0usize;
    while let Some(result) = handles.join_next().await {
        processed += result??;
    }
    assert_eq!(processed, job_count, "workers did not process all jobs");
    Ok(())
}

impl PerfConfig {
    fn from_env() -> Self {
        Self {
            jobs_per_scenario: env_parse("AZUMS_PERF_JOBS", 10_000),
            iterations: env_parse("AZUMS_PERF_ITERATIONS", 5),
            batch_size: env_parse("AZUMS_PERF_BATCH_SIZE", 64),
            backends: env::var("AZUMS_PERF_BACKENDS")
                .unwrap_or_else(|_| "memory,sqlite".to_string())
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
            output_dir: env::var("AZUMS_PERF_OUTPUT_DIR")
                .unwrap_or_else(|_| "target/azums-perf".to_string()),
        }
    }
}

impl EnvironmentReport {
    fn capture() -> Self {
        Self {
            os: env::consts::OS.to_string(),
            arch: env::consts::ARCH.to_string(),
            rust_profile: if cfg!(debug_assertions) {
                "debug".to_string()
            } else {
                "release".to_string()
            },
            cpu_count_hint: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(0),
            resource_notes: vec![
                "CPU, allocation, disk I/O, and network I/O counters are not collected by the std-only harness; use platform profilers alongside the JSON output for those dimensions.".to_string(),
                "RAM is reported as null unless a platform-specific collector is added.".to_string(),
            ],
        }
    }
}

impl LatencyReport {
    fn from_durations(values: &[Duration]) -> Self {
        let mut samples: Vec<f64> = values
            .iter()
            .map(|duration| duration.as_secs_f64() * 1_000.0)
            .collect();
        samples.sort_by(|a, b| a.total_cmp(b));
        Self {
            p50_ms: percentile(&samples, 0.50),
            p95_ms: percentile(&samples, 0.95),
            p99_ms: percentile(&samples, 0.99),
            p999_ms: percentile(&samples, 0.999),
        }
    }
}

impl ResourceReport {
    fn from_wall(wall: Duration) -> Self {
        Self {
            wall_ms: wall.as_secs_f64() * 1_000.0,
            cpu: None,
            ram_bytes: None,
            allocations: None,
            disk_io_bytes: None,
            network_io_bytes: None,
            notes: vec![
                "wall-clock timing captured by std::time::Instant".to_string(),
                "resource counters intentionally nullable when not measured".to_string(),
            ],
        }
    }
}

fn percentile(samples: &[f64], p: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let rank = ((samples.len() - 1) as f64 * p).ceil() as usize;
    samples[rank.min(samples.len() - 1)]
}

fn env_parse<T>(name: &str, default: T) -> T
where
    T: std::str::FromStr,
{
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<T>().ok())
        .unwrap_or(default)
}

fn render_markdown(report: &PerfReport) -> String {
    let mut out = String::new();
    out.push_str("# Azums M14 Performance Report\n\n");
    out.push_str(&format!(
        "- generated_at_unix_ms: {}\n- jobs_per_scenario: {}\n- iterations: {}\n- batch_size: {}\n- backends: {}\n- profile: {}\n\n",
        report.generated_at_unix_ms,
        report.config.jobs_per_scenario,
        report.config.iterations,
        report.config.batch_size,
        report.config.backends.join(", "),
        report.environment.rust_profile,
    ));

    out.push_str(
        "| Backend | Workload | Workers | Jobs/sec | p50 ms | p95 ms | p99 ms | p99.9 ms |\n",
    );
    out.push_str("|---|---|---:|---:|---:|---:|---:|---:|\n");
    for result in &report.results {
        out.push_str(&format!(
            "| {} | {} | {} | {:.2} | {:.4} | {:.4} | {:.4} | {:.4} |\n",
            result.backend,
            result.workload,
            result.workers,
            result.throughput_jobs_per_sec,
            result.latency.p50_ms,
            result.latency.p95_ms,
            result.latency.p99_ms,
            result.latency.p999_ms,
        ));
    }

    out.push_str("\n## Conditions\n\n");
    out.push_str(
        "- Throughput includes enqueue plus lease/start-attempt/ACK drain for each scenario.\n",
    );
    out.push_str(
        "- Percentiles are calculated across scenario iterations, not per-job handler latency.\n",
    );
    out.push_str(
        "- External backends are included only when their environment variables are configured.\n",
    );
    out.push_str("- Resource fields are explicit and nullable; missing CPU/RAM/I/O counters are not inferred.\n");
    out
}
