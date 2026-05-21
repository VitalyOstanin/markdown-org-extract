# ADR-0005: No community meta-docs until a community exists

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

Open-source projects routinely accumulate community-facing
meta-documentation: `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
`SECURITY.md`, issue templates under `.github/ISSUE_TEMPLATE/`, a
pull-request template `PULL_REQUEST_TEMPLATE.md`. The genre exists
because real external contributor traffic forces the project to
write down its release process, branching model, and reporting
channels.

This project does not yet have that traffic. There is a single
author and an agent collaborator; review and merge happen locally;
the release process lives in
[`.github/workflows/release.yml`](../../.github/workflows/release.yml)
and in [`CLAUDE.md`](../../CLAUDE.md).

The previous `CONTRIBUTING.md` was removed precisely because it had
drifted: it described an older release process incompletely and in
places incorrectly. The workflow moved on; the document did not.

## Decision

Community-facing meta-documentation is not created until a real
community exists.

Specifically, the following files are **not** in the repository and
are **not** to be created without an explicit request from the
maintainer:

- `CONTRIBUTING.md`
- `CODE_OF_CONDUCT.md`
- `SECURITY.md`
- `.github/ISSUE_TEMPLATE/` and any files under it
- `.github/PULL_REQUEST_TEMPLATE.md`

Project conventions (TDD, code style, release process) live in:

- [`CLAUDE.md`](../../CLAUDE.md) -- the canonical place for
  per-project agent and human contributor rules.
- [`.github/workflows/`](../../.github/workflows/) -- the canonical
  description of release and CI behaviour.
- This `docs/adr/` directory -- the canonical place for
  architectural and policy decisions with context.

Reviewer tasks of the form "add CONTRIBUTING.md" or "create
SECURITY.md" are closed with a pointer to this ADR.

If a real external contributor community appears later, the
meta-documentation is recreated from the actual current workflow,
not restored from the deleted version.

## Consequences

Easier:

- No stale meta-documentation to keep in sync. The agent does not
  have to update three separate places when the release process
  changes.
- New project rules land in a single discoverable place
  ([`CLAUDE.md`](../../CLAUDE.md) for rules, this directory for
  decisions).
- The repository surface stays small and on-topic.

Harder:

- A first-time external contributor sees a repository without the
  signals they may be used to (no `CONTRIBUTING.md`, no issue
  templates). The README has to compensate by pointing at the
  workflow and at this ADR.
- A future re-creation of these files is a deliberate task, not a
  copy-paste; the old versions are gone.

## References

- Project rules: [`CLAUDE.md`](../../CLAUDE.md)
- Release workflow: [`.github/workflows/release.yml`](../../.github/workflows/release.yml)
- Originating decision: removal of the previous `CONTRIBUTING.md`
  for being out of date with the actual release process.
