# Kumo Release Process

This document is for maintainers. The normal release path is tag-driven: merge
the release PR, push the release tag, then let GitHub Actions publish to
crates.io and create the GitHub Release.

## Release `kumo`

1. Start from a clean, updated `main`.

   ```bash
   git checkout main
   git pull --ff-only
   ```

2. Create one branch for the release work.

   ```bash
   git checkout -b <type>/<short-release-task>
   ```

3. Include release metadata in the same PR as the feature or fix.

   - Bump the root `Cargo.toml` package version.
   - Let `cargo check` or `cargo metadata` update the root `Cargo.lock` package
     version.
   - Add the matching entry under `## kumo` in `docs/src/changelog.md`.
   - Update docs and examples when user-facing behavior changes.

4. Open a PR and wait for CI.

   ```bash
   git push -u origin <branch>
   gh pr create --base main --head <branch>
   ```

5. Squash merge after CI is green.

6. Pull the updated `main`, create the release tag, and push the tag.

   ```bash
   git checkout main
   git pull --ff-only
   git tag kumo-vX.Y.Z
   git push origin kumo-vX.Y.Z
   ```

7. Let GitHub Actions publish the crate and create the GitHub Release.

   Do not run `cargo publish` manually during the normal path. The tag-driven
   publish workflow is the source of truth.

8. Verify the result.

   ```bash
   gh run list --repo wihlarkop/kumo --limit 5
   gh release view kumo-vX.Y.Z --repo wihlarkop/kumo
   cargo info kumo
   ```

## Release `kumo-derive`

Use the same process, but bump `kumo-derive/Cargo.toml`, update the
`kumo-derive` section in `docs/src/changelog.md`, and tag with:

```bash
git tag kumo-derive-vX.Y.Z
git push origin kumo-derive-vX.Y.Z
```

If the root `kumo` crate should depend on the new derive version, release
`kumo-derive` first, then update the root `kumo-derive` dependency in a
separate `kumo` release PR.

## Recovery Rules

- If crates.io already has the version, the publish workflow treats that as
  success and continues.
- If the GitHub Release already exists, the publish workflow skips release
  creation and exits successfully.
- If crates.io publish fails before the crate is available, fix the workflow or
  code and rerun the failed workflow.
- Use manual `cargo publish` only as a recovery action when the tag workflow is
  broken and the fix cannot be applied before publishing.
- If a wrong tag was pushed before publishing, delete the remote tag, delete the
  local tag, fix the commit, then recreate and push the tag.

## Tag Format

- Main crate: `kumo-vX.Y.Z`
- Derive crate: `kumo-derive-vX.Y.Z`

The publish workflow only responds to those two tag patterns.
