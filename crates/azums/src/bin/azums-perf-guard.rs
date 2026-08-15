use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::PathBuf,
};

#[derive(Debug, Deserialize)]
struct PerfReport {
    results: Vec<ScenarioReport>,
}

#[derive(Debug, Deserialize)]
struct ScenarioReport {
    backend: String,
    workload: String,
    throughput_jobs_per_sec: f64,
    latency: LatencyReport,
    resources: ResourceReport,
}

#[derive(Debug, Deserialize)]
struct LatencyReport {
    p50_ms: f64,
    p99_ms: f64,
}

#[derive(Debug, Deserialize)]
struct ResourceReport {
    allocations: Option<u64>,
    cpu: Option<String>,
    ram_bytes: Option<u64>,
}

#[derive(Debug)]
struct ScenarioAggregate {
    throughput_jobs_per_sec: f64,
    latency_p50_ms: f64,
    latency_p99_ms: f64,
    allocations: Option<f64>,
    ram_bytes: Option<f64>,
    cpu_measured: bool,
}

#[derive(Debug)]
struct Thresholds {
    throughput_regression: f64,
    latency_regression: f64,
    allocation_regression: f64,
    memory_regression: f64,
}

#[derive(Debug)]
struct Regression {
    key: String,
    metric: &'static str,
    baseline: f64,
    current: f64,
    change_pct: f64,
    threshold_pct: f64,
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 && args.len() != 5 {
        eprintln!(
            "usage: azums-perf-guard <baseline> <current> [<confirmation-baseline> <confirmation-current>]\n\
             thresholds: AZUMS_PERF_MAX_THROUGHPUT_REGRESSION=0.05, \
             AZUMS_PERF_MAX_LATENCY_REGRESSION=0.05, \
             AZUMS_PERF_MAX_ALLOCATION_REGRESSION=0.10, \
             AZUMS_PERF_MAX_MEMORY_REGRESSION=0.10"
        );
        std::process::exit(2);
    }

    let baseline = load_report(PathBuf::from(&args[1]))?;
    let current = load_report(PathBuf::from(&args[2]))?;
    let thresholds = Thresholds::from_env();
    let mut regressions = compare_reports(&baseline, &current, &thresholds);

    if args.len() == 5 && !regressions.is_empty() {
        let confirmation_baseline = load_report(PathBuf::from(&args[3]))?;
        let confirmation_current = load_report(PathBuf::from(&args[4]))?;
        let confirmation =
            compare_reports(&confirmation_baseline, &confirmation_current, &thresholds);
        let confirmed = confirmation
            .iter()
            .map(regression_id)
            .collect::<HashSet<_>>();

        regressions.retain(|regression| {
            let is_confirmed = confirmed.contains(&regression_id(regression));
            if !is_confirmed {
                println!(
                    "PERF_GUARD_OBSERVATION key={} metric={} change={:.2}% reason=confirmation-not-met",
                    regression.key,
                    regression.metric,
                    regression.change_pct * 100.0,
                );
            }
            is_confirmed
        });
    }

    if regressions.is_empty() {
        println!(
            "PERF_GUARD_OK compared={} regressions=0",
            aggregate_results(&current).len()
        );
        return Ok(());
    }

    for regression in &regressions {
        eprintln!(
            "PERF_REGRESSION key={} metric={} baseline={:.6} current={:.6} change={:.2}% threshold={:.2}%",
            regression.key,
            regression.metric,
            regression.baseline,
            regression.current,
            regression.change_pct * 100.0,
            regression.threshold_pct * 100.0,
        );
    }

    anyhow::bail!(
        "{} performance regression(s) exceeded thresholds",
        regressions.len()
    )
}

fn regression_id(regression: &Regression) -> (&str, &'static str) {
    (&regression.key, regression.metric)
}

fn load_report(path: PathBuf) -> anyhow::Result<PerfReport> {
    Ok(serde_json::from_slice(&fs::read(&path)?)?)
}

fn compare_reports(
    baseline: &PerfReport,
    current: &PerfReport,
    thresholds: &Thresholds,
) -> Vec<Regression> {
    let baseline = aggregate_results(baseline);
    let current = aggregate_results(current);

    let mut regressions = Vec::new();
    for (key, current_result) in current {
        let Some(baseline_result) = baseline.get(&key) else {
            println!("PERF_GUARD_SKIP key={key} reason=missing-baseline");
            continue;
        };

        compare_lower_is_worse(
            &mut regressions,
            &key,
            "throughput_jobs_per_sec",
            baseline_result.throughput_jobs_per_sec,
            current_result.throughput_jobs_per_sec,
            thresholds.throughput_regression,
        );
        let mut latency_regressions = Vec::new();
        compare_higher_is_worse(
            &mut latency_regressions,
            &key,
            "latency.p50_ms",
            baseline_result.latency_p50_ms,
            current_result.latency_p50_ms,
            thresholds.latency_regression,
        );
        compare_higher_is_worse(
            &mut latency_regressions,
            &key,
            "latency.p99_ms",
            baseline_result.latency_p99_ms,
            current_result.latency_p99_ms,
            thresholds.latency_regression,
        );
        if latency_regressions.len() == 2 {
            regressions.extend(latency_regressions);
        } else {
            for observation in latency_regressions {
                println!(
                    "PERF_GUARD_OBSERVATION key={} metric={} change={:.2}% reason=latency-quorum-not-met",
                    observation.key,
                    observation.metric,
                    observation.change_pct * 100.0,
                );
            }
        }

        compare_optional_higher_is_worse(
            &mut regressions,
            &key,
            "resources.allocations",
            baseline_result.allocations,
            current_result.allocations,
            thresholds.allocation_regression,
        );
        compare_optional_higher_is_worse(
            &mut regressions,
            &key,
            "resources.ram_bytes",
            baseline_result.ram_bytes,
            current_result.ram_bytes,
            thresholds.memory_regression,
        );

        if !baseline_result.cpu_measured || !current_result.cpu_measured {
            println!("PERF_GUARD_SKIP key={key} metric=resources.cpu reason=not-measured");
        }
    }

    regressions
}

fn aggregate_results(report: &PerfReport) -> HashMap<String, ScenarioAggregate> {
    let mut groups: HashMap<String, Vec<&ScenarioReport>> = HashMap::new();
    for result in &report.results {
        groups.entry(group_key(result)).or_default().push(result);
    }

    groups
        .into_iter()
        .map(|(key, results)| {
            let aggregate =
                ScenarioAggregate {
                    throughput_jobs_per_sec: median(
                        results.iter().map(|result| result.throughput_jobs_per_sec),
                    ),
                    latency_p50_ms: median(results.iter().map(|result| result.latency.p50_ms)),
                    latency_p99_ms: median(results.iter().map(|result| result.latency.p99_ms)),
                    allocations: optional_median(results.iter().filter_map(|result| {
                        result.resources.allocations.map(|value| value as f64)
                    })),
                    ram_bytes: optional_median(
                        results.iter().filter_map(|result| {
                            result.resources.ram_bytes.map(|value| value as f64)
                        }),
                    ),
                    cpu_measured: results.iter().all(|result| result.resources.cpu.is_some()),
                };
            (key, aggregate)
        })
        .collect()
}

fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

fn optional_median(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values = values.collect::<Vec<_>>();
    (!values.is_empty()).then(|| median(values.into_iter()))
}

fn compare_lower_is_worse(
    regressions: &mut Vec<Regression>,
    key: &str,
    metric: &'static str,
    baseline: f64,
    current: f64,
    threshold: f64,
) {
    if baseline <= 0.0 {
        println!("PERF_GUARD_SKIP key={key} metric={metric} reason=zero-baseline");
        return;
    }
    let change = (baseline - current) / baseline;
    if change > threshold {
        regressions.push(Regression {
            key: key.to_string(),
            metric,
            baseline,
            current,
            change_pct: change,
            threshold_pct: threshold,
        });
    }
}

fn compare_higher_is_worse(
    regressions: &mut Vec<Regression>,
    key: &str,
    metric: &'static str,
    baseline: f64,
    current: f64,
    threshold: f64,
) {
    if baseline <= 0.0 {
        println!("PERF_GUARD_SKIP key={key} metric={metric} reason=zero-baseline");
        return;
    }
    let change = (current - baseline) / baseline;
    if change > threshold {
        regressions.push(Regression {
            key: key.to_string(),
            metric,
            baseline,
            current,
            change_pct: change,
            threshold_pct: threshold,
        });
    }
}

fn compare_optional_higher_is_worse(
    regressions: &mut Vec<Regression>,
    key: &str,
    metric: &'static str,
    baseline: Option<f64>,
    current: Option<f64>,
    threshold: f64,
) {
    match (baseline, current) {
        (Some(baseline), Some(current)) => {
            compare_higher_is_worse(regressions, key, metric, baseline, current, threshold);
        }
        _ => {
            println!("PERF_GUARD_SKIP key={key} metric={metric} reason=not-measured");
        }
    }
}

fn group_key(result: &ScenarioReport) -> String {
    format!("{}|{}", result.backend, result.workload)
}

impl Thresholds {
    fn from_env() -> Self {
        Self {
            throughput_regression: env_parse("AZUMS_PERF_MAX_THROUGHPUT_REGRESSION", 0.05),
            latency_regression: env_parse("AZUMS_PERF_MAX_LATENCY_REGRESSION", 0.05),
            allocation_regression: env_parse("AZUMS_PERF_MAX_ALLOCATION_REGRESSION", 0.10),
            memory_regression: env_parse("AZUMS_PERF_MAX_MEMORY_REGRESSION", 0.10),
        }
    }
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
