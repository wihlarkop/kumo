# Contributing to kumo

Thank you for your interest in contributing!

## Getting Started

```bash
git clone https://github.com/wihlarkop/kumo
cd kumo
cargo build
cargo test
```

## Running Tests

```bash
# Unit + integration tests
cargo test

# With a specific feature
cargo test --features sqlite

# Derive macro tests
cargo test --features derive --test derive_macro
```

Before release-sensitive changes, also run:

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo check --workspace --all-targets --features "postgres sqlite mysql llm claude openai gemini ollama jsonpath xpath browser persistence redis-frontier derive otel cloud cloud-s3 cloud-gcs cloud-azure"
```

## Running Examples

```bash
cargo run --example quotes
cargo run --example books
```

See [`examples/README.md`](examples/README.md) for the full list.

## Pull Requests

1. Fork the repo and create a branch from `main`
2. Make your changes with tests where applicable
3. Run `cargo clippy --all-targets -- -D warnings` and `cargo fmt`
4. Open a pull request — describe what changed and why

## Reporting Bugs

Use the [bug report template](https://github.com/wihlarkop/kumo/issues/new?template=bug_report.md).

Please report security vulnerabilities privately. See [SECURITY.md](SECURITY.md).

## Requesting Features

Use the [feature request template](https://github.com/wihlarkop/kumo/issues/new?template=feature_request.md).

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
