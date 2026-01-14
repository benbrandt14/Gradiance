.PHONY: all setup run test lint clean fmt

all: test lint

setup:
	@echo "Ensuring rustup is installed..."
	@command -v rustup >/dev/null 2>&1 || { echo >&2 "rustup is not installed. Please install it from https://rustup.rs/"; exit 1; }
	@echo "Installing stable toolchain..."
	@rustup install stable
	@rustup default stable
	@echo "Installing clippy and rustfmt..."
	@rustup component add clippy rustfmt

run:
	cargo run

test:
	cargo test

lint:
	cargo clippy -- -D warnings

clean:
	cargo clean

fmt:
	cargo fmt
