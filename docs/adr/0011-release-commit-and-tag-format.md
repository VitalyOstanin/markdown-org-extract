# ADR-0011: Release commit and tag format

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
  - [Release commit](#release-commit)
  - [Release tag](#release-tag)
  - [Worked example](#worked-example)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

The project's release history shows three drift problems:

1. **Commit subject style varies.** Past release commits include
   `Bump version to 0.1.5 and fix dead code warnings`,
   `chore: bump version to 0.1.6`, `chore: release 0.2.1`,
   `release: 0.3.0`. Five different patterns over ten releases.
2. **Tag object type varies.** v0.1.2 through v0.1.6 are
   lightweight (a bare ref pointing at the commit). v0.2.0 onward
   are annotated tags with `--message` bodies. Lightweight tags
   lose the release date and the tagger identity that signed
   pushes carry.
3. **Annotated tag bodies are inconsistent.** v0.2.2 has an empty
   tag body (only the subject `v0.2.2`), v0.2.0 has a one-line
   summary, v0.3.0/v0.3.1 carry the conventional-commit subject
   `release: <ver>`. None of them mirror the corresponding
   CHANGELOG section, so `git show v0.X.Y` does not tell the
   reader what changed.

The result: `git log --oneline --grep release` is unreliable for
release archaeology, and a reader running `git show v0.2.2` learns
nothing about that release without opening `CHANGELOG.md` in
parallel. Both costs are small per release and compound across
years.

The release-pipeline upgrade in v0.3.1 already locked in the
`needs: [test, lint, msrv]` gate; this ADR locks in the
human-authored shape of the release commit and tag so the next
ten releases look like the same project as the last ten.

## Decision

### Release commit

The single commit that bumps the version (and any related
release prep like CHANGELOG re-flow) uses the conventional-commit
subject:

```
release: <X.Y.Z>
```

Examples that conform: `release: 0.3.0`, `release: 0.4.0`.
Examples that do NOT conform: `chore: release 0.3.0`,
`Bump version to 0.3.0`, `chore: bump version`.

The commit body is optional; if present, it is the unwrapped
contents of the `## [<X.Y.Z>] — YYYY-MM-DD` CHANGELOG section
(without the heading line). This makes `git log --format=%B`
self-describing for releases.

### Release tag

Tags are always **annotated** (`git tag -a v<X.Y.Z>`). Lightweight
tags are refused by convention; CI does not verify this today, so
this rule lives in the maintainer's checklist and in this ADR.

The tag subject is `v<X.Y.Z>` (matches the ref name).

The tag body is the unwrapped contents of the `## [<X.Y.Z>] —
YYYY-MM-DD` CHANGELOG section, without the heading line and
without surrounding blank lines. Concretely, this is the same text
that `.github/workflows/release.yml` extracts for the GitHub
Release notes:

```sh
awk -v ver="<X.Y.Z>" '
  $0 ~ "^## \\["ver"\\]" { flag=1; next }
  flag && /^## \[/ { exit }
  flag { print }
' CHANGELOG.md | sed -e '1{/^$/d}' -e '$,${/^$/d}'
```

Mirroring the tag body to the CHANGELOG section means `git show
v<X.Y.Z>` is a complete, offline-readable change description — no
need to open the website or the file.

### Worked example

For a hypothetical 0.4.0 release with a CHANGELOG section:

```
## [0.4.0] — 2026-06-10

### Added

- `--watch` mode that re-runs the agenda on file change.

### Fixed

- Holiday calendar lookup for 2027 (off-by-one on New Year).
```

The release commit:

```
release: 0.4.0

Added:
- `--watch` mode that re-runs the agenda on file change.

Fixed:
- Holiday calendar lookup for 2027 (off-by-one on New Year).
```

The release tag (annotated, body matches the CHANGELOG):

```
$ git tag -a v0.4.0 -m "v0.4.0

### Added

- \`--watch\` mode that re-runs the agenda on file change.

### Fixed

- Holiday calendar lookup for 2027 (off-by-one on New Year)."
```

### Scope

This ADR applies **from the next release forward**. Historical
commits and tags are not rewritten — that would break every
existing `Cargo.lock` resolving the tagged versions and every
external link to the GitHub Releases page.

## Consequences

Easier:

- `git log --oneline --grep '^release: '` reliably enumerates every
  release commit.
- `git show v<X.Y.Z>` is a self-contained change description; no
  need to cross-reference the CHANGELOG file at the tag's tree
  state.
- The CHANGELOG section, the GitHub Release notes (auto-extracted
  in `release.yml`), the tag body, and the commit body all read
  the same — one source of truth, three rendering surfaces.

Harder:

- The maintainer's release checklist gains one more "copy the
  CHANGELOG section into the tag body" step. Mitigated by the
  worked example above and by the fact that the same text already
  has to be in CHANGELOG.md for `scripts/check-changelog.sh` to
  pass.
- An automated check for "tag is annotated and body mirrors
  CHANGELOG" would be ideal but is not part of this ADR. Adding it
  would require either a release-prep script or an extra
  release.yml step that runs *before* `gh release create` reads
  the same data — feasible but out of scope here.

## References

- `.github/workflows/release.yml` — the publish workflow that
  extracts release notes from CHANGELOG.md via `awk`. The tag body
  convention chosen above is designed to match its extraction
  shape so the three artefacts stay in sync.
- ADR-0010 (rollback policy) — rollbacks add a `### Yanked`
  subsection to the same CHANGELOG section; this ADR is the
  upstream rule that keeps the section authoritative.
- [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) —
  source of the section structure (`### Added`, `### Fixed`, …)
  that the tag body mirrors.
- [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)
  — informs the `release: <version>` subject shape (`release` is
  used as the type in lieu of `chore` so a quick log scan can
  separate releases from maintenance commits).
