# ADR-0010: Rollback policy for published releases

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
  - [When to yank](#when-to-yank)
  - [When NOT to yank](#when-not-to-yank)
  - [How to roll back](#how-to-roll-back)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

`cargo yank` marks a published crate version as "do not use for new
projects" without deleting it. Existing `Cargo.lock` files keep
resolving the yanked version (the crate is still downloadable), but
new `cargo add` / `cargo install` for the affected version range
will refuse it. There is no `cargo unyank`-style erase; yanks are
reversible (`cargo yank --undo`) but cannot remove the published
artefact from the registry.

This project has hit one concrete edge case so far: v0.3.0 was
published while the `test (windows-latest)` CI job was red. The
failure was in the release-helper integration test
(`tests/release_check_changelog.rs`), not in any code the published
binary actually executes. End users running
`cargo install markdown-org-extract` or pulling the crate as a
dependency were unaffected. The maintainer chose to *not* yank
0.3.0 and to ship a 0.3.1 patch that fixed the CI matrix instead.
The 0.3.1 CHANGELOG documents that decision inline.

Without an explicit policy, the next incident will re-litigate the
same questions: does this warrant a yank, what do we write in the
CHANGELOG, do we re-tag the broken version, do we mark the GitHub
Release as pre-release. This ADR captures the answers up front.

## Decision

### When to yank

Yank a published version when **the binary or library shipped to
users is itself broken**. Concretely:

| Class                 | Example                                                                            | Yank? |
| --------------------- | ---------------------------------------------------------------------------------- | :---: |
| Regression            | A flag silently changes meaning, JSON envelope loses a field, panic on valid input | Yes   |
| Security              | Exploitable parse / IO bug, privilege-related path traversal                       | Yes   |
| Accidental breaking   | A semver-minor bump that breaks a documented contract                              | Yes   |
| Wrong artefact        | Wrong file uploaded to crates.io, vendored credential, debug build shipped         | Yes   |

### When NOT to yank

Do NOT yank when the published artefact is correct and only the
*release pipeline* or peripheral CI matrix was broken:

| Class                          | Example                                                          | Yank? |
| ------------------------------ | ---------------------------------------------------------------- | :---: |
| Red CI on unrelated platform   | `tests/release_check_changelog.rs` failing only on Windows CI    | No    |
| Documentation drift            | README example uses a removed flag spelling; binary is unaffected | No    |
| Tooling regression             | `scripts/check.sh` aggregator is broken; binary is unaffected     | No    |
| Stale CHANGELOG link           | Markdown anchor in CHANGELOG.md is wrong                          | No    |

The reasoning: yanks are visible to every consumer (warning on
`cargo update`), and yanking a release for a CI-only problem trains
downstream users to ignore yank warnings. The 0.3.0 precedent
applies — fix forward with a patch release and document the
discrepancy in that release's `### Context` section.

### How to roll back

When the answer above is "yes":

1. **Yank the version** on crates.io:
   `cargo yank --version <X.Y.Z>` (requires `CARGO_REGISTRY_TOKEN`
   or a logged-in `cargo login`).
2. **Update CHANGELOG.md**. Add a `### Yanked` subsection inside the
   `## [X.Y.Z]` entry (Keep a Changelog convention) that names the
   reason in one or two sentences and points at the follow-up
   release. Do NOT delete the original entry — the version still
   exists on crates.io, and the CHANGELOG reflects history.
3. **Mark the GitHub Release** as a pre-release (`gh release edit
   <tag> --prerelease`) and prepend a one-line warning to the
   release body. The git tag stays in place; do not force-push or
   delete the tag.
4. **Cut the follow-up patch release** (`X.Y.Z+1`) with the fix.
   The new release's CHANGELOG entry includes a `### Context`
   paragraph naming the yanked predecessor and the reason — the
   same shape the 0.3.1 entry uses for its CI fix.

When the answer is "no" (the precedent case): skip steps 1–3 and
go directly to step 4. Document the decision in the patch release's
`### Context` paragraph so the next maintainer can read why the
predecessor was not yanked.

`cargo yank --undo --version <X.Y.Z>` is reserved for the case
where the yank itself was a mistake (e.g. the wrong version was
yanked). It is not used to "take back" a deliberate yank after the
fact; if the yank turned out to be premature, the path forward is
still a follow-up release.

## Consequences

Easier:

- The next incident does not re-litigate yank-or-fix-forward; the
  table above answers it in seconds.
- Downstream users see consistent semantics for yanks: a yank in
  this project's history always means the published artefact was
  broken, never "CI was red on the side".
- The CHANGELOG record stays truthful: a yanked release keeps its
  entry plus a `### Yanked` annotation, so historical search and
  release-notes extraction continue to work.

Harder:

- A broken release that does not meet the yank criteria still ships
  in `cargo install <pkg>` resolution until the next patch lands.
  Accepted: a yank is a louder signal than a patch, and using it
  for CI-only problems would dilute that signal.

## References

- [`cargo yank`](https://doc.rust-lang.org/cargo/commands/cargo-yank.html)
  — official semantics.
- [Keep a Changelog § Yanked](https://keepachangelog.com/en/1.1.0/#yanked)
  — convention for marking yanked versions.
- `CHANGELOG.md` § `[0.3.1]` — concrete precedent for the
  "fix forward without yank" path.
- ADR-0006 (no registry-duplicate guard) — context on what the
  release pipeline does and does NOT check before publish.
