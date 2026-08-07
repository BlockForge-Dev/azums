# Justfile for workspace tasks

# Default recipe: run cargo check across all workspace crates
default: check

# Check compilation across all workspace crates
check:
    cargo check --workspace

# Run core crate check for ultra-fast verification
check-core:
    cargo check -p postgresflow-core

# Run unit and integration tests across the workspace
test:
    cargo test --workspace

# Run doc tests across the workspace
doc-test:
    cargo test --doc --workspace

# Build dev binaries and libraries
build:
    cargo build --workspace

# Check formatting
fmt-check:
    cargo fmt --all -- --check

# Format code
fmt:
    cargo fmt --all

# Run clippy lints
clippy:
    cargo clippy --workspace --all-targets -- -D warnings
