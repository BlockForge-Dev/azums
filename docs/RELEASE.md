# Release Process

## Versioning
Use semantic versioning:
- MAJOR: breaking API/behavior changes
- MINOR: backward-compatible features
- PATCH: fixes and small improvements

Current package versions are declared in:
- `crates/azums/Cargo.toml`
- `crates/worker/Cargo.toml`

## Automated Releases (release-plz)

This project uses [release-plz](https://release-plz.ieni.dev/) for automated releases.

### How it works

1. **On push to `main`:** The `release.yml` workflow runs `release-plz release-pr`, which:
   - Detects version-bump-worthy changes via conventional commits
   - Updates `CHANGELOG.md`
   - Bumps crate versions
   - Opens (or updates) a release PR

2. **On release PR merge:** `release-plz release` runs automatically, which:
   - Publishes to crates.io (`cargo publish`)
   - Creates a Git tag (`v0.x.y`)
   - Creates a GitHub Release

### Required Secrets

| Secret | Where | Purpose |
|--------|-------|---------|
| `GITHUB_TOKEN` | Automatic | PR creation and GitHub releases |
| `CARGO_REGISTRY_TOKEN` | Repository settings → Secrets | `cargo publish` to crates.io |

### Manual Publish (fallback)

If the CI pipeline is unavailable:

```bash
# 1. Bump version in crates/azums/Cargo.toml
# 2. Update CHANGELOG.md
# 3. Commit and tag
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin v0.2.0

# 4. Publish
cargo publish -p azums
```

## Pre-Release Checklist
- [ ] CI is green on main branch
- [ ] local checks pass:
  - `cargo check --workspace`
  - `cargo test --workspace --no-run`
  - `.\scripts\tests\ci.ps1` with DB env vars set
- [ ] README and docs updated
- [ ] CHANGELOG.md updated
- [ ] migrations reviewed for rollout safety
- [ ] benchmark notes captured for significant performance-impacting changes
- [ ] `cargo publish --dry-run -p azums` passes

## Published Crates

| Crate | Published | Notes |
|-------|-----------|-------|
| `azums` | ✅ crates.io | Core library + CLI |
| `worker` | ❌ not published | Application-specific, meant to be forked |

## Post-Release Verification

After a crates.io publish:

```bash
# Verify install works
cargo install azums

# Verify dependency usage in a fresh project
cargo init /tmp/azums-test
cd /tmp/azums-test
cargo add azums
cargo check
```

Then deploy to target environment and validate:
- enqueue + process + timeline flow works
- Monitor retry/DLQ rates for regressions.

## Rollback Plan
If release causes regressions:
1. Stop rollout.
2. Roll back application image/version.
3. If migration is incompatible, apply explicit down/repair plan before traffic restoration.
4. Capture incident summary and add follow-up actions.
5. If a crates.io publish was bad, yank the version: `cargo yank --version 0.x.y azums`
