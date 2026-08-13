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
| `AZUMS_PERF_MAX_LATENCY_REGRESSION` | `0.05` | Fail when p50 or p99 latency increases by more than 5%. |
| `AZUMS_PERF_MAX_ALLOCATION_REGRESSION` | `0.10` | Fail when measured allocations increase by more than 10%. |
| `AZUMS_PERF_MAX_MEMORY_REGRESSION` | `0.10` | Fail when measured memory increases by more than 10%. |

## CI Semantics

The guard compares matching `(backend, workload, workers)` scenarios.

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

The default GitHub workflow stores the latest generated M14 report as an artifact and runs the guard when a baseline artifact is available.
