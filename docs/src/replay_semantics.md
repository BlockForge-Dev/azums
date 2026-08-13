# Replay Semantics

Replay creates a new logical job from a retained source job. It is an operational recovery primitive,
not a promise that previous external side effects are undone or made exactly-once.

Implementation:

- Portable API: [backend/mod.rs](../../crates/azums-core/src/backend/mod.rs)
- Memory implementation: [memory.rs](../../crates/azums-core/src/backend/memory.rs)
- PostgreSQL repository path: [repo.rs](../../crates/azums/src/jobs/repo.rs)
- Quickstart helper: [quickstart.rs](../../crates/azums/src/quickstart.rs)

Tests:

- Replay coverage: [replay.rs](../../crates/azums/tests/replay.rs)
- Beginner inspect and replay path: [m16_developer_experience.rs](../../crates/azums/tests/m16_developer_experience.rs)

## Guarantees

Guaranteed:

- Replay returns a new job ID.
- The replayed job records `replay_of_job_id` when the backend supports retained source history.
- Replay uses the original payload unless the caller routes it to another queue or run time through
  backend-specific options.

Backend-dependent:

- Whether archived jobs remain replayable.
- How long source payload and metadata are retained.
- Whether replay participates in a surrounding application transaction.

Not guaranteed:

- Replay does not undo the original job.
- Replay does not prove the original handler's external side effects failed.
- Replay does not provide exactly-once external side-effect semantics.
