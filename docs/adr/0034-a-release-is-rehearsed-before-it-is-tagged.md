# ADR-0034: A release is rehearsed on a branch, before it is tagged

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-22).

## Context

[ADR-0033](0033-nothing-irreversible-before-the-archives-are-checked.md)
put every check ahead of publication and called a manual run with
`dry_run` a rehearsal. It is one, for a tag that already exists. It is
not one for the tag about to be made, and that is the tag worth
rehearsing.

Two things stand in the way. The workflow file comes from the default
branch while the tree comes from the tag, so a check added since a tag
was cut runs against a tree that predates it: the run of `v0.18.0` on
2026-08-22 failed on a README snippet reading `0.14` — the check was
newer than the tag, and no tag of the past can satisfy it. And a tag of
the future cannot be made to try: pushing it is what starts the real
release, so the rehearsal and the thing rehearsed are the same act.

What is left is a rehearsal that can only ever confirm what has already
shipped. The checks that refuse a release — the version in `Cargo.toml`
against `Cargo.lock` and the README snippet, the CHANGELOG being ready,
the archives building, `cargo publish --dry-run` — all read the tree.
Only two read the tag: its format, and the annotated body mirroring the
CHANGELOG.

## Decision

The manual trigger takes an empty tag, and that means a rehearsal on the
branch the run was started from.

The version being rehearsed is the one `Cargo.toml` carries, and a tag is
synthesised from it so every later step reads one shape of answer. What
the tag alone can settle is skipped: the annotated body has nothing to
mirror yet, and the ancestry check guards publication, which a rehearsal
never reaches. `publish` and, through it, `release` do not run at all —
not because `dry_run` says so, but because there is no tag to publish.

Everything else runs as it does for a real release: the four gating jobs,
the version agreeing with the lock and the README, the CHANGELOG being
ready for that version, the release-profile smoke test, all four archives
built and checked against the downstream contract, and
`cargo publish --dry-run`.

A rehearsal therefore answers one question: if this tree were tagged now,
would the release go through. It is meant to be run on a tree already
prepared for the release — the version bumped, the CHANGELOG section
moved over, the README snippet updated — because that is the tree the tag
will be cut from.

## Consequences

The failure that used to be found by pushing a tag is now found before
one exists. Nothing has to be untagged, and no version is burnt on a
forgotten README line.

A rehearsal on an unprepared tree fails on the CHANGELOG, and correctly:
it is answering about the version in the manifest, which is the one
already released. That reads as a false alarm to anyone expecting it to
guess the next version. It does not guess — the version is stated by
`Cargo.toml`, and stating it is part of preparing the release.

Two things stay unrehearsed, and both are the tagging itself: the format
of the tag and its annotated body. They are checked by the real run, on
the tag, where they first exist.

A rehearsal costs a full run — four gating jobs and four cross-platform
archives — for a result that publishes nothing. It is a deliberate spend
before a release, not something to run on every push.

## References

- [ADR-0033: Nothing irreversible happens before the archives are checked](0033-nothing-irreversible-before-the-archives-are-checked.md)
- [ADR-0011: Release commit and tag format](0011-release-commit-and-tag-format.md)
- [ADR-0017: No branch protection on master; pre-commit hook policy](0017-no-branch-protection-on-master.md)
