# Contributing to Azums

Thank you for considering contributing to `azums`! We welcome bug reports, feature proposals, documentation improvements, and pull requests.

## 🛠️ Development Setup

Ensure you have Rust 1.75+ installed. Clone the repository and run workspace checks:

```bash
git clone https://github.com/BlockForge-Dev/azums.git
cd azums

# Run workspace compilation checks
cargo check --workspace

# Run all workspace unit and integration tests
cargo test --workspace

# Run doc tests
cargo test --doc --workspace
```

Or using `just` / `make`:

```bash
just check
just test
```

## 📐 Code Style & Conventions

- Format code using `cargo fmt --all`.
- Ensure all public functions and traits have `# Examples` doc comments.
- Run `cargo clippy --workspace --all-targets -- -D warnings` to verify lints.

## 🔀 Pull Request Process

1. Create a descriptive feature branch (`git checkout -b feat/my-feature`).
2. Add automated tests covering new behavior.
3. Update `CHANGELOG.md` under `[Unreleased]` with a brief description.
4. Open a Pull Request on GitHub.

Thank you for helping make `azums` the gold standard for Rust job queues!

