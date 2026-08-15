# Performance Regression Protection

M15 adds an automatic guard so significant slowdowns become visible.

Run a current report:

```powershell
cargo run -p azums --release --bin azums-perf
```

Compare against a baseline:

```powershell
cargo run -p azums --bin azums-perf-guard -- baseline/m14_report.json target/azums-perf/m14_report.json
```

## Thresholds

| Variable | Default | Meaning |
|---|---:|---|
| `AZUMS_PERF_MAX_THROUGHPUT_REGRESSION` | `0.05` | Fail when jobs/sec drops by more than 5%. |
| `AZUMS_PERF_MAX_LATENCY_REGRESSION` | `0.05` | Fail when both p50 and p99 latency increase by more than 5%. |
| `AZUMS_PERF_MAX_ALLOCATION_REGRESSION` | `0.10` | Fail when measured allocations increase by more than 10%. |
| `AZUMS_PERF_MAX_MEMORY_REGRESSION` | `0.10` | Fail when measured memory increases by more than 10%. |

## CI Semantics

The report preserves every `(backend, workload, workers)` scenario. The guard groups matching
`(backend, workload)` scenarios and compares the median value across the configured worker counts.
This keeps per-worker measurements observable while preventing one noisy hosted-runner sample from
being treated as a statistically meaningful regression. Latency requires p50 and p99 confirmation;
a threshold breach in only one percentile is emitted as `PERF_GUARD_OBSERVATION`.

Tracked automatically:

- throughput
- p50 latency
- p99 latency
- allocations when measured
- memory when measured
- CPU visibility when measured by an external collector

If a resource counter is `null`, the guard prints `PERF_GUARD_SKIP` for that metric instead of inventing data.

## Baselines

Baselines are JSON reports emitted by `azums-perf`. A valid regression check must disclose:

- baseline report source
- current report source
- benchmark command
- backend URLs or skipped backends
- threshold overrides
- whether allocations, CPU, and memory counters were measured

The default GitHub workflow checks out the previous revision and measures baseline and current code
on the same runner. Baseline and current use separate Postgres databases and Redis logical databases,
so retained rows or keys cannot bias the second measurement. The current report is then stored as an
artifact for inspection and dashboard history; the cross-machine artifact is not used as the blocking
comparison baseline.
