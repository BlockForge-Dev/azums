# Solana Reconciliation Rule Pack V1

## Purpose

Milestone 5 proves the generic reconciliation framework with the first real adapter-aware rule pack.

The framework remains adapter-neutral.

Solana-specific logic stays inside:

- `SolanaReconRulePack`
- `SolanaObservationResolver`

## Expected Facts

The Solana rule pack now builds durable expected facts for:

- `solana.source`
- `solana.destination`
- `solana.asset`
- `solana.amount`
- `solana.program`
- `solana.action`
- `solana.execution_reference`
- `solana.finality`

It also emits advisory expectation facts for:

- `solana.confirmation_expectation`
- `solana.terminal_window_ms`

Advisory facts are persisted for operator visibility but excluded from strict fact matching.

## Observed Facts

The Solana observation resolver now materializes observed facts from adapter-local durable evidence:

- signature and final signature
- execution reference
- source and destination
- amount
- asset
- program and action
- finality and confirmation status
- provider used
- blockhash used
- simulation outcome
- observed error payload
- distinct signature count

The primary adapter evidence source remains:

- `solana.tx_attempts`
- `solana.tx_intents`

## Mismatch Subcodes

The Solana rule pack now emits explicit subcodes in recon details and machine reasons:

- `signature_missing`
- `amount_mismatch`
- `destination_mismatch`
- `pending_too_long`
- `duplicate_signal`
- `onchain_state_unresolved`
- `observed_error_differs_from_expected`

Additional Solana-specific subcodes currently used for richer operator diagnostics:

- `source_mismatch`
- `program_mismatch`
- `action_mismatch`

## Generic Category Mapping

The rule pack maps Solana-specific subcodes into the generic exception taxonomy:

- `signature_missing` -> `observation_missing`
- `amount_mismatch` -> `amount_mismatch`
- `destination_mismatch` -> `destination_mismatch`
- `pending_too_long` -> `delayed_finality`
- `duplicate_signal` -> `duplicate_signal`
- `onchain_state_unresolved` -> `state_mismatch` or `external_state_unknown` depending on posture
- `observed_error_differs_from_expected` -> `state_mismatch`

## Durable Evidence

The rule pack now leaves durable operator evidence through:

- recon expected facts
- recon observed facts
- recon run state transitions
- recon receipts
- recon evidence snapshots

## Fixture-backed Coverage

Fixture-backed recon tests now cover:

- finalized success match
- stale pending classification
- duplicate signature/manual review classification
- amount mismatch partial match
- observed error divergence from expected success

## Benchmark

Benchmark command:

```powershell
$env:RECON_BENCH_ITERATIONS="500"
cargo run --manifest-path crates/recon_core/Cargo.toml --example benchmark_solana_recon --quiet
```

Latest local result in this repo state:

- `iterations=500`
- `total_ms=545.068`
- `avg_us=1090.137`
