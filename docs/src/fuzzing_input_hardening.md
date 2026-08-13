# Fuzzing & Input Hardening

M13 assumes public input is hostile or malformed.

Run the fuzz hardening suite:

```powershell
cargo test -p azums --test m13_fuzz_hardening
```

The suite is deterministic and CI-friendly. It uses byte-driven generated cases rather than an infinite external fuzzer, so failures are reproducible in normal `cargo test`.

## Fuzzed Boundaries

M13 fuzzes:

- job payloads
- job type strings
- queue names
- idempotency-key-like metadata
- stream names
- event types
- event payloads
- malformed serialized jobs
- malformed serialized events
- status parser inputs
- typed payload decoding
- lease and stream API boundaries

## Safety Invariants

Every generated case must preserve:

- no panic
- no unbounded allocation from generated input
- no infinite loop
- committed jobs remain readable
- listed jobs have parseable statuses
- leases are unique within a batch
- running jobs have a worker owner
- terminal jobs do not retain live leases
- published events are readable at their assigned sequence
- malformed serialized data rejects cleanly

## Scope

The automated suite runs against `MemoryBackend` because it gives fast coverage of the public model, parser, job, lease, and stream boundaries.

This is not a replacement for coverage-guided fuzzers such as `cargo-fuzz`. It is the always-on hardening layer that keeps boundary safety in CI.
