# ADR-0006: Do not duplicate registry duplicate-publish guards

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

crates.io treats published versions as immutable: a `cargo publish`
for an already-existing `name@version` is rejected by the registry
server-side at step 5 of 5 of the publish flow, with the message
`crate version 'X.Y.Z' is already uploaded`.

It is tempting to add a CI step before `cargo publish` that probes
crates.io itself to check whether the current version is already
published, and to skip the publish step (with a warning) when it
is. Task 024 in the project's task log proposed exactly this.

Three problems with that approach:

1. **A custom check duplicates a server-side guarantee.** The
   registry's own error message is clear and unambiguous; a second
   guard does not buy more safety.
2. **A custom check introduces false-positive failure modes.** A
   5xx response from the crates.io API or a transient network
   failure can block a valid release that would otherwise succeed.
3. **"Skip + warning on conflict" produces a falsely-green
   release.** The workflow finishes successfully on a re-run that
   did not actually publish anything; the maintainer believes the
   release happened when it did not.

Saving 5--10 minutes of CI on erroneous re-runs is not worth the
risk of a falsely-green release or of blocking a real one.

## Decision

The release workflow relies on the registry to reject duplicate
versions. It does **not** add a separate pre-publish probe against
the crates.io API.

The same rule applies by analogy to other registries (npm, PyPI,
Docker Hub) and other release-side duplicate guards: if the
registry itself rejects duplicates with a clear error, do not
duplicate the check on the CI side.

Reviewer tasks of the form "add a crates.io version-already-
published probe" or its equivalents for other registries are closed
with a pointer to this ADR.

## Consequences

Easier:

- The release YAML stays small. There is no API call, no auth
  token for read access, no JSON parsing in shell, no
  branch-on-status-code logic to maintain.
- The native registry error is the single source of truth for
  "this version exists already". Maintainers see the real error,
  not a synthesised one.

Harder:

- Re-running the release workflow against an already-published
  version costs the full CI time before failing at the publish
  step. This is accepted in exchange for the safety properties
  above.
- If the registry ever changes its error format or removes the
  duplicate-publish guarantee, this ADR has to be superseded.

## References

- Cargo Book on publishing: [doc.rust-lang.org/cargo/reference/publishing.html](https://doc.rust-lang.org/cargo/reference/publishing.html)
- Release workflow: [`.github/workflows/release.yml`](../../.github/workflows/release.yml)
- Originating proposal: task 024 in the project task log.
