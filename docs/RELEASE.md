# Release Process

## Versioning

Azums uses semantic versioning before and after 1.0:

- MAJOR: breaking API or documented behavior changes
- MINOR: backward-compatible features
- PATCH: fixes and small improvements

All workspace crate versions and internal Azums dependency requirements must be updated together.
Azums 1.0 is governed separately by the [M21 Stable Release Gate](src/stable_release.md); a pre-1.0
release does not declare the API or execution semantics permanently stable.

## Automated Publication

Crates.io publication runs only from a `v*` Git tag. A branch-based manual dispatch is rejected.

The release workflow:

1. Verifies that the tag version matches every publishable crate manifest.
2. Runs `cargo test --workspace --locked` on the tagged commit.
3. Publishes crates in dependency order.
4. Waits for each crate version to become visible in the crates.io index before publishing a
   dependent crate.
5. Verifies that every expected crate version is available from crates.io.

The publish script is restartable. It skips a crate only when that exact version is already visible
on crates.io. Authentication, packaging, network, and registry errors fail the workflow.

## Required Secret

| Secret | Where | Purpose |
|---|---|---|
| `CARGO_REGISTRY_TOKEN` | Repository settings -> Actions secrets | Publish crates to crates.io |

## Release Procedure

1. Update every workspace package version and internal Azums dependency requirement.
2. Update `Cargo.lock` and `CHANGELOG.md`.
3. Run the release checks locally.
4. Commit and push to `main`.
5. Wait for CI, documentation, and benchmark workflows to pass on the exact commit.
6. Create and push an annotated version tag.

```bash
bash scripts/publish-crates.sh --check 0.2.1
cargo test --workspace --locked
cargo publish -p azums-core --dry-run --locked

git tag -a v0.2.1 -m "Release v0.2.1"
git push origin v0.2.1
```

Pushing the tag starts `.github/workflows/release.yml`. Do not publish individual downstream crates
out of order.

## Publication Order

1. `azums-core`
2. `azums-redis`
3. `azums`
4. `azums-postgres`
5. `azums-axum`
6. `azums-actix`
7. `azums-poem`
8. `azums-rocket`

`worker` and `azums-dashboard` are workspace applications and are not published.

## Pre-Release Checklist

- [ ] Exact release commit is green in CI, documentation, and benchmarks.
- [ ] `cargo test --workspace --locked` passes.
- [ ] `bash scripts/publish-crates.sh --check <version>` passes.
- [ ] `cargo publish -p azums-core --dry-run --locked` passes.
- [ ] Every publishable manifest uses the release version.
- [ ] Every internal Azums dependency uses the release version.
- [ ] `Cargo.lock` and `CHANGELOG.md` are updated.
- [ ] Migrations are reviewed for rollout safety.
- [ ] Release notes do not overstate pre-1.0 semantic stability.

## Post-Release Verification

After the release workflow succeeds, verify the registry from a fresh project:

```bash
cargo search azums --limit 10
cargo init /tmp/azums-test
cd /tmp/azums-test
cargo add azums@0.2.1
cargo check
```

Also verify that the crates.io pages and docs.rs builds exist for all published crates.

## Recovery

Publication is permanent and cannot be overwritten. If the workflow stops after publishing only
some crates, rerun it from the same tag; already-visible versions are verified and skipped.

If a published release is defective:

1. Stop application rollout.
2. Revert applications to the previous dependency version.
3. Yank affected crate versions with `cargo yank` when new consumers should not select them.
4. Prepare a new patch version; never try to overwrite the published version.
