# CI and Release

This repository is designed to be:

- **warning-free** (build, clippy, rustdoc)
- **fully tested**
- **auto-publishable** to crates.io via GitHub Actions

## CI workflow

The CI workflow runs on pushes and PRs and performs:

- formatting check (`cargo fmt`)
- builds in `no_std` mode
- builds in `std` mode
- tests (`cargo test --features std`)
- clippy with warnings denied
- docs build

## Publish workflow

The publish workflow triggers on tags matching `v*` (e.g. `v0.1.0`):

- reruns the full checks (fmt/build/test/clippy/docs)
- runs `cargo publish`

### Required GitHub secret

Set repository secret:

- `CARGO_REGISTRY_TOKEN`: your crates.io API token

## Tagging

Typical release:

1. Update `Cargo.toml` version
2. Commit
3. Tag: `vX.Y.Z`
4. Push tag to GitHub
5. Workflow publishes to crates.io

