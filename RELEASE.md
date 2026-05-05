# Release Checklist

Use this checklist after the `kumo` crate ownership transfer is complete.

## Preflight

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo test -p kumo --test derive_macro --features derive
cargo check --workspace --all-targets --features "postgres sqlite mysql llm claude openai gemini ollama jsonpath xpath browser persistence redis-frontier derive otel cloud cloud-s3 cloud-gcs cloud-azure"
cargo audit
cargo publish -p kumo-derive --dry-run
cargo publish -p kumo --dry-run
```

The `stealth` feature requires CMake and NASM because it builds BoringSSL.
Check it separately on a machine with those tools installed:

```bash
cargo check --features stealth
cargo check --features stealth,browser
```

## Publish Order

`kumo` depends on `kumo-derive` for the `derive` feature, so publish the
proc-macro crate first when its version has not already been published.

```bash
git tag kumo-derive-v0.1.2
git push origin kumo-derive-v0.1.2
```

Wait until crates.io accepts the new `kumo-derive` version, then publish the
main crate:

```bash
git tag kumo-v0.1.0
git push origin kumo-v0.1.0
```

The publish workflow also supports manual dry-runs from GitHub Actions.

## After Publish

- Confirm `https://crates.io/crates/kumo` renders correctly.
- Confirm `https://docs.rs/kumo` builds successfully.
- Create a GitHub release for the published tag.
- Update docs and README links if the published crate name changes.
