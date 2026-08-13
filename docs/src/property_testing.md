# Property-Based Testing

M12 adds generated tests for Azums behavior that should hold across many state combinations, not just hand-picked examples.

Run the property suite:

```powershell
cargo test -p azums --test m12_property_based
```

The default suite uses fixed `proptest` case counts so CI is deterministic. When a property fails, `proptest` shrinks the generated input to a smaller failing program.

## Generated Inputs

The M12 suite generates:

- random job sequences
- random lifecycle transition pairs
- random retries
- random priorities
- random schedules around the current time
- random duplicate enqueue operations
- random worker identities and batch sizes
- random SQLite transaction commit/rollback decisions

## Properties

| Property | Test evidence |
|---|---|
| No illegal state transition | `m12_generated_lifecycle_state_transitions_are_exact` |
| No duplicate valid lease | `m12_generated_lifecycle_programs_preserve_core_invariants` |
| Attempts never decrease | `m12_generated_lifecycle_programs_preserve_core_invariants` |
| Completed/DLQ/cancelled jobs remain terminal | `m12_generated_lifecycle_programs_preserve_core_invariants` |
| Idempotency keys produce one logical job | `m12_generated_lifecycle_programs_preserve_core_invariants` |
| Scheduled jobs are only leased when eligible | `m12_generated_lifecycle_programs_preserve_core_invariants` |
| Rollback produces no durable job | `m12_sqlite_generated_rollbacks_leave_no_durable_job` |

## Scope

The generated lifecycle program runs against `MemoryBackend` because it can execute many combinations quickly and deterministically while exposing in-memory attempt history for invariant checks.

The rollback property runs against SQLite because it exercises a real transactional backend without requiring external services.

PostgreSQL and Redis already have targeted integration coverage for these primitives. Their generated property profiles are backend-dependent because they require service lifecycle control and test isolation that a normal cargo test run may not have.

## Non-Goals

Property tests do not guarantee exhaustive proof across infinite inputs. They raise confidence by generating and shrinking many combinations under the same documented semantics used by the example tests.
