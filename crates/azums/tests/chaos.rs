#[path = "chaos/mod.rs"]
mod chaos_support;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn m11_memory_randomized_chaos_ci_matrix() -> anyhow::Result<()> {
    let scenarios = std::env::var("AZUMS_CHAOS_CI_SCENARIOS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(256);
    let seed = chaos_support::seed_from_env("AZUMS_CHAOS_SEED", 0xA11CE_2026);

    chaos_support::memory::run_randomized_scenarios(scenarios, seed).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn m11_sqlite_contention_chaos_ci_matrix() -> anyhow::Result<()> {
    let seed = chaos_support::seed_from_env("AZUMS_CHAOS_SQLITE_SEED", 0xA11CE_5117E);

    chaos_support::sqlite::run_contention_scenario(seed, 80, 8).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "opt-in long chaos run; set AZUMS_CHAOS_SCENARIOS=10000 or higher"]
async fn m11_memory_randomized_chaos_10000_plus() -> anyhow::Result<()> {
    let scenarios = std::env::var("AZUMS_CHAOS_SCENARIOS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10_000);
    assert!(
        scenarios >= 10_000,
        "M11 long chaos run must execute at least 10,000 scenarios"
    );
    let seed = chaos_support::seed_from_env("AZUMS_CHAOS_SEED", 0xA11CE_10_000);

    chaos_support::memory::run_randomized_scenarios(scenarios, seed).await
}
