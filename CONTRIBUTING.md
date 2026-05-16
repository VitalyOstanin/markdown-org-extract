# Contributing to markdown-org-extract

Thanks for considering a contribution! This document captures the conventions
this project follows so that pull requests can be reviewed and merged quickly.

## Table of contents

- [Development environment](#development-environment)
- [Code style](#code-style)
- [Running tests](#running-tests)
- [Pull requests](#pull-requests)
- [Release process](#release-process)

## Development environment

- Rust toolchain: stable channel; minimum supported version is **1.70**
  (as declared in `Cargo.toml` via `rust-version`).
- The build script (`build.rs`) reads `holidays_ru.json` at compile time and
  embeds the data into the binary. Edit that file to add new holiday data.

## Code style

- `cargo fmt --all` before submitting.
- `cargo clippy --all-targets --all-features -- -D warnings` should be clean.
- Public APIs require `///` doc-comments; module-level intent goes into
  `//!` doc-comments at the top of each file.
- Avoid `unwrap()` and `expect()` in non-test code unless the invariant is
  documented inline. Prefer `?` and the `AppError` variants.

## Running tests

```sh
cargo test
```

The repository has both unit tests (inside `src/`) and integration tests
(`tests/cli.rs`). Integration tests invoke the compiled binary against
fixture files under `examples/` — they are intentionally lightweight and
don't need network or external services.

## Pull requests

- One topic per pull request. Smaller PRs review faster.
- Update `CHANGELOG.md` under `## [Unreleased]` for any user-visible change.
- Don't bump the version in `Cargo.toml` from a pull request — the release
  workflow does that.

## Release process

1. Update `CHANGELOG.md`: move entries from `[Unreleased]` to a new dated
   section with the version.
2. Bump `version` in `Cargo.toml`.
3. Tag the commit: `git tag vX.Y.Z && git push --tags`.
4. The `Release` workflow runs `cargo test`, `cargo clippy`, and
   `cargo fmt --check`, then publishes to crates.io.
