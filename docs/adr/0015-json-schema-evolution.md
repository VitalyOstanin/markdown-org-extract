# ADR-0015: JSON schema evolution and consumer coordination

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted. Amended by [ADR-0021](0021-accept-canceled-spelling.md)
(2026-05-29): the cancelled task_type value reflects the original file
spelling, so its value set spans CANCELLED and CANCELED. Non-breaking.

## Context

The CLI prints structured task data to stdout as JSON (see
[ADR-0001](0001-standalone-cli-for-org-in-markdown.md) for the
standalone-CLI contract). The current consumer is
[`markdown-org-vscode`](https://github.com/VitalyOstanin/markdown-org-vscode),
a VS Code extension that calls the CLI as a subprocess and renders
the JSON in the editor. The extension records the CLI version it was
tested against in its `package.json` under
`x-markdown-org.extractorVersion`; at runtime it checks the actual
CLI version and decides whether to proceed.

As new features land (e.g. ADR-0014 adds a bracket-form marker to
every timestamp), the JSON output gains new fields. The question is
how the contract evolves and how that evolution is communicated to
consumers without overcomplicating either side.

Three options were considered:

1. **No explicit schema versioning.** Treat new field additions as
   non-breaking; rely on consumers to ignore unknown keys. Use the
   CLI version (via `--version` and `extractorVersion` in the
   consumer) as the implicit coordination channel.
2. **Per-record `schema_version` field.** Every JSON record carries
   an explicit integer. Consumers branch on it.
3. **A separate published JSON schema file** with versioned releases,
   independent of the CLI binary version.

Option 2 adds plumbing on every read and every write for every JSON
emitter, with no concrete consumer asking for it today. Option 3
multiplies artefacts (CLI binary + schema file) for a project with a
single consumer. Option 1 keeps the surface minimal and matches the
fact that the consumer is already version-aware.

The decision is also constrained by [ADR-0006](0006-no-registry-duplicate-guard.md)'s
spirit: don't add a guard the ecosystem already provides. The
producer-consumer pair already has `--version` and
`extractorVersion`; adding `schema_version` would be a second,
parallel mechanism for the same purpose.

## Decision

### No explicit schema version field

The CLI's JSON output does **not** carry a `schema_version` (or
similar) field. Adding such a field later -- if the situation
changes -- would itself be a non-breaking addition under the rules
below.

### Non-breaking additions

Adding a new field to a JSON record is considered **non-breaking**.
Consumers MUST ignore unknown fields. This is consistent with how the
project already treats the optional `clocks` and `total_clock_time`
fields added in ADR-0003 (existing consumers that read only the older
fields keep working).

Non-breaking additions ship in a minor release (semver `0.X.Y` → `0.X+1.0`
while pre-1.0; `X.Y.Z` → `X.Y+1.0` from 1.0 onward) and are noted in
`CHANGELOG.md` under `### Added`.

The CANCELLED `task_type` enum addition exercises this rule (release
0.8.0, 2026-05-29). It is a non-breaking addition because the
consumer (`markdown-org-vscode`) ships graceful fallback for unknown
`task_type` values in the same coordinated release.

### Breaking changes

The following count as **breaking** and require:

- A major version bump (semver `X.Y.Z` → `X+1.0.0` from 1.0 onward;
  pre-1.0 the project uses a `0.X.Y` → `0.X+1.0` bump and flags the
  break explicitly in CHANGELOG).
- An explicit `### Changed` or `### Removed` entry in `CHANGELOG.md`
  with the migration recipe.
- A coordinated bump of `x-markdown-org.extractorVersion` in
  `markdown-org-vscode` `package.json`, with the extension's runtime
  check updated to accept the new minimum.

What counts as breaking:

- Removing a field that was previously emitted under any condition.
- Renaming a field.
- Changing the type of a field (e.g. `string` → `array`).
- Changing the semantics of a field (e.g. `date` previously meant
  "start date", now means "due date").
- Changing the wrapping structure of the output (e.g. wrapping a
  top-level array in an object, splitting one record across multiple
  records).

What does **not** count as breaking:

- Adding a new field, optional or always-present.
- Adding new variants to an enum-like string field, provided the
  consumer is documented to handle unknown variants gracefully (no
  throw, fallback to "no status" or equivalent neutral semantics).
  The CANCELLED `task_type` addition (2026-05-29) is the worked
  example; the consumer side `markdown-org-vscode` ships the matching
  graceful-fallback handling. When in doubt: treat as breaking.
- Tightening the input parsing (rejecting forms that previously
  emitted spurious output) as long as the JSON shape for valid input
  is unchanged.

### Coordination with the consumer

The single current consumer (`markdown-org-vscode`) records the
expected CLI version in `package.json` under
`x-markdown-org.extractorVersion`. The extension reads the CLI's
`--version` at startup and branches on it.

When a release of this CLI adds new fields, the consumer's pull
request that adopts them also bumps `extractorVersion`. The
coordination is bilateral and explicit; no separate schema-versioning
channel is introduced.

### When to reconsider

This ADR is to be revisited if any of the following becomes true:

- A second independent consumer appears that cannot follow the
  `extractorVersion` mechanism (e.g. a third-party tool that hard-codes
  a JSON schema URL).
- Multiple incompatible JSON shapes need to be supported in the same
  release (e.g. a `--legacy-output` flag).
- The CLI gains a stable v1.0 contract that needs an external,
  machine-readable schema for integration with non-Rust tooling.

Until then, the rule stands.

## Consequences

Easier:

- No schema-versioning plumbing in the emitter; no version-branching
  on the reader side beyond what's already there for the CLI version.
- The rule for what counts as breaking is mechanical and matches what
  the project already does in CHANGELOG.
- Adding ADR-0014's bracket-form marker is a non-breaking field
  addition under this policy -- no special handling required.

Harder:

- Consumers that don't read `extractorVersion` (currently none) would
  need to be told to do so. The README of any future consumer must
  mention the mechanism.
- A regression that silently drops a field would be a breaking change
  under this policy. The release CI must include a JSON-shape
  smoke-test on the example fixtures to catch silent drops; this is a
  CI hardening follow-up tracked separately.

## References

- Consumer coordination point:
  [`markdown-org-vscode` `package.json`](https://github.com/VitalyOstanin/markdown-org-vscode/blob/master/package.json)
  (`x-markdown-org.extractorVersion` field).
- CLI JSON output entry points:
  [`src/render.rs`](../../src/render.rs),
  [`src/types.rs`](../../src/types.rs).
- Related ADRs:
  [ADR-0001](0001-standalone-cli-for-org-in-markdown.md) (the
  standalone-CLI / JSON-on-stdout contract this policy applies to),
  [ADR-0003](0003-clock-metadata-support.md) (the precedent of
  adding optional fields non-breakingly: `clocks`,
  `total_clock_time`),
  [ADR-0014](0014-active-and-inactive-timestamps.md) (the first
  schema change to be governed by this policy),
  [ADR-0006](0006-no-registry-duplicate-guard.md) (the
  "don't duplicate ecosystem mechanisms" spirit applied to
  versioning).
