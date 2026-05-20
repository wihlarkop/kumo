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

`kumo` depends on `kumo-derive` for the `derive` feature. Use tag-driven
publishing only: merge the release PR, push the matching tag, then let GitHub
Actions publish to crates.io. Do not manually run `cargo publish` for the same
version when the tag workflow will publish it.

Publish the proc-macro crate first only when its version changed and has not
already been published.

```bash
git tag kumo-derive-v0.1.3
git push origin kumo-derive-v0.1.3
```

Wait until crates.io accepts any new `kumo-derive` version, then tag and
publish the main crate:

```bash
git tag kumo-v0.2.11
git push origin kumo-v0.2.11
```

The publish workflow also supports manual dry-runs from GitHub Actions.

## Release Notes Template

For `kumo-v0.2.11`, call out:

- main `kumo` crate now depends on `kumo-derive 0.1.3` for the `derive`
  feature
- derive users get the latest compile-time diagnostics and `attr` + `re`
  extraction behavior through `kumo`
- publish workflow now tolerates crates.io "already exists" races after a
  version is already available

## After Publish

- Confirm `https://crates.io/crates/kumo` renders correctly.
- Confirm `https://docs.rs/kumo` builds successfully.
- Create a GitHub release for the published tag.
- Update docs and README links if the published crate name changes.
