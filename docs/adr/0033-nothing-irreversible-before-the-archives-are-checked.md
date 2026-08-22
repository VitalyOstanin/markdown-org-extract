# ADR-0033: Nothing irreversible happens before the archives are checked

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-22).

## Context

A release does two things that cannot be undone in the same sense.
`cargo publish` hands crates.io a version that stays there: the only
retreat is `cargo yank`, which hides the version from new resolution
without removing it, or a follow-up patch release
([ADR-0010](0010-rollback-policy.md)). Everything else a
release does — a GitHub Release, its assets — can be deleted and made
again.

Until this decision the workflow did the irreversible thing first.
`publish` ran the checks, published the crate and created the Release;
`package-binaries` then built one archive per target, verified each
against the downstream-packager contract
([`scripts/verify-archive.sh`](../../scripts/verify-archive.sh)) and
uploaded them. A failure in that second job — a packaging script broken
on one runner, an archive whose layout no longer matches what the README
promises, a missing third-party notice
([ADR-0024](0024-third-party-license-notices-in-archives.md)) — arrived
after the version was already public. The release could not be
retracted, only yanked or patched, and the version number was spent.

The order was not a considered one: `package-binaries` was written after
`publish` existed and was attached to it with `needs: [publish]` because
`gh release upload` needs a Release to upload to.

## Decision

The pipeline is ordered so that every check that can refuse a release
runs before the one step that cannot be taken back.

| № | Job                | Runs after                   | What it may do                                                          |
|---|--------------------|------------------------------|-------------------------------------------------------------------------|
| 1 | `test`, `lint`, `msrv`, `audit` | nothing         | read the repository                                                     |
| 2 | `verify`           | those four                   | resolve and validate the tag, check the version against `Cargo.toml`, `Cargo.lock` and the README snippet, check the CHANGELOG and the tag body, run the release smoke test, `cargo publish --dry-run` |
| 3 | `package-binaries` | `verify`                     | build and verify one archive per target, keep each as a workflow artifact |
| 4 | `publish`          | `verify`, `package-binaries` | `cargo publish`, and nothing else                                        |
| 5 | `release`          | `verify`, `publish`          | create the GitHub Release and upload the archives to it                  |

- **The tag is resolved once.** `verify` validates it and exports it as a
  job output; the jobs after it read `needs.verify.outputs`, so there is
  one place where "what are we releasing" is decided.
- **The archives travel as workflow artifacts.** The Release does not
  exist while they are being built, so they are uploaded to the run and
  attached to the Release afterwards.
- **The irreversible step stands alone.** `cargo publish` is a job of its
  own, and the Release and its assets are made by the job after it. A
  failure in that tail is re-runnable on its own; had it been a step of the
  publishing job, a re-run would begin with `cargo publish` over a version
  crates.io already holds — there is no duplicate guard
  ([ADR-0006](0006-no-registry-duplicate-guard.md)) — and never reach the
  upload.
- **A dry run exercises everything up to publication.** `workflow_dispatch`
  with `dry_run` skips `publish` and, with it, `release`: the checks run and
  all four archives are built, verified and kept as artifacts. That is a
  rehearsal of the pipeline against the tag it will really be run on, short
  of the two jobs that write anything outside the run.

## Consequences

- A packaging failure now costs a re-run of the tag, not a yanked
  version. The crate reaches crates.io only when four archives already
  exist and match the contract.
- A release takes longer to reach crates.io: publication waits for the
  slowest of four cross-platform builds. That is the price of the
  ordering, paid on every release.
- Publication is one job further from the tag push, so a failure in
  `package-binaries` leaves nothing behind at all — no crate version, no
  Release, no partial set of assets.
- A release now takes five jobs where it took two, and the Release exists
  for a few seconds without its assets — the window between `publish` and
  `release` finishing.
- The jobs after `verify` no longer re-derive the tag, so
  `scripts/release-validate-tag.sh` runs once per release rather than
  once per job.

## References

- [`.github/workflows/release.yml`](../../.github/workflows/release.yml)
- [ADR-0006: Do not duplicate registry duplicate-publish guards](0006-no-registry-duplicate-guard.md)
- [ADR-0010: Rollback policy for published releases](0010-rollback-policy.md)
- [ADR-0024: Third-party licence notices ship inside the release archives](0024-third-party-license-notices-in-archives.md)
