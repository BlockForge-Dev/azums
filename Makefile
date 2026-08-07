.PHONY: all check check-core test doc-test build fmt clippy

all: check test

check:
	cargo check --workspace

check-core:
	cargo check -p postgresflow-core

test:
	cargo test --workspace

doc-test:
	cargo test --doc --workspace

build:
	cargo build --workspace

fmt:
	cargo fmt --all

clippy:
	cargo clippy --workspace --all-targets -- -D warnings
