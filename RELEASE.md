# Release Checklist

Use this checklist before publishing a new `kumo` release.

## Preflight

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo test -p kumo --test derive_macro --features derive
cargo check --example production_crawler --features persistence
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

`kumo` depends on `kumo-derive` for the `derive` feature. Publish the
proc-macro crate first only when its version changed and has not already been
published.

```bash
git tag kumo-derive-v0.1.2
git push origin kumo-derive-v0.1.2
```

Wait until crates.io accepts any new `kumo-derive` version, then tag and
publish the main crate:

```bash
git tag kumo-v0.2.10
git push origin kumo-v0.2.10
```

The publish workflow also supports manual dry-runs from GitHub Actions.

## Release Notes Template

For `kumo-v0.2.10`, call out:

- release-facing documentation cleanup for `0.2.x`
- Rust MSRV documentation aligned with `rust-version = "1.88"`
- new `production_crawler` example covering robots.txt, per-domain politeness,
  retry policy, `Retry-After`, `StatusRetry`, persistent `FileFrontier`, metrics,
  and JSONL storage
- crate metadata cleanup for docs.rs/crates.io

## After Publish

- Confirm `https://crates.io/crates/kumo` renders correctly.
- Confirm `https://docs.rs/kumo` builds successfully.
- Create a GitHub release for the published tag.
- Update docs and README links if the published crate name changes.
