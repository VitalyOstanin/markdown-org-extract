# ADR-0004: TDD is mandatory for code changes

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

A small CLI with a stable JSON wire contract is easy to break in
subtle ways: a regex change shifts what is parsed; a clap flag
rewrite drops a `conflicts_with` declaration; a render tweak loses a
trailing newline. Such regressions are cheap to ship and expensive
to discover, because the consumer (e.g. `markdown-org-vscode`)
typically only sees them at runtime, on a user's data.

The project does not have a manual QA pass; the test suite is the
only enforcement of behaviour. So either every change comes with
tests that exercise the actual behaviour, or the suite drifts away
from the code and stops serving as a regression net.

Precedent: task 021 in the project's task log added
`conflicts_with` between `--holidays` and `--from` / `--to`. The
task was initially closed without integration tests on
`conflicts_with`; the declaration could have silently broken under
any later clap-argument refactor without a single test failing.
Tests were added after the fact at the user's request.

## Decision

Every code change is accompanied by tests that exercise the actual
behaviour. Specifically:

1. If the affected behaviour had no tests yet, add them under TDD
   (red → green → refactor): first a failing test, then the
   smallest code change that makes it pass, then refactoring if
   needed.
2. For bug fixes, first write a test that reproduces the bug (red)
   and only then write the fix (green). A test without a fix
   proves the bug existed; a fix without a test does not prove the
   bug will not return.
3. For CLI flags and validation, cover both the golden path and
   the conflicts / error cases through `assert_cmd` in
   [`tests/cli.rs`](../../tests/cli.rs).
4. For the parser, agenda, formatting, and other logic -- unit
   tests in `#[cfg(test)] mod tests` next to the code, plus
   snapshot tests on byte-exact output where appropriate.
5. Tests must exercise behaviour with concrete inputs and concrete
   expected outputs, not merely check that the code compiles or
   that a function exists.
6. How to run: `cargo test`. Before closing a task, run the full
   suite green, not just the new tests.

Documentation-only changes, README typo fixes, and similar non-code
edits are exempt.

## Consequences

Easier:

- The test suite stays in lockstep with the code. `cargo test` is
  the project's single source of truth on whether the binary still
  behaves as advertised.
- Refactors are safe because the existing tests pin the existing
  behaviour. Rewriting clap or the parser does not require a
  manual smoke test.
- New contributors (human or agent) have a clear signal of done:
  green tests on the new behaviour.

Harder:

- Every task carries a test-writing cost. For a one-line clap
  attribute change, the test boilerplate can dwarf the change
  itself. The trade-off is accepted because the alternative is a
  silent regression on the next refactor.
- TDD discipline is not enforced by tooling; it is enforced by the
  agent and by review. A drift here is a process bug, not a code
  bug.

## References

- Integration tests for the CLI surface: [`tests/cli.rs`](../../tests/cli.rs)
- Snapshot golden-output tests: [`tests/`](../../tests/)
- Project root rules: [`CLAUDE.md`](../../CLAUDE.md)
- Originating incident: task 021 in the project task log
  (`--holidays` / `--from` / `--to` `conflicts_with` without tests).
