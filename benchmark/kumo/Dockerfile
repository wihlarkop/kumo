FROM rust:latest AS chef
RUN cargo install cargo-chef --locked
WORKDIR /workspace

# --- Plan: generate a dependency recipe from the workspace manifests ---
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY kumo-derive/ kumo-derive/
COPY examples/ examples/
COPY tests/ tests/
COPY benchmark/kumo/ benchmark/kumo/
RUN cargo chef prepare --recipe-path recipe.json

# --- Cook: compile only dependencies (layer is cached until Cargo.lock changes) ---
FROM chef AS builder
COPY --from=planner /workspace/recipe.json recipe.json
RUN cargo chef cook --release -p kumo-benchmark --recipe-path recipe.json

# --- Build: compile kumo-benchmark itself (only reruns when src changes) ---
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY kumo-derive/ kumo-derive/
COPY examples/ examples/
COPY tests/ tests/
COPY benchmark/kumo/ benchmark/kumo/
RUN cargo build --release -p kumo-benchmark

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /workspace/target/release/kumo-benchmark /usr/local/bin/
CMD ["kumo-benchmark"]
