# ADR-0007: No test counts in README

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

It is a common reflex to advertise test coverage in the README with
phrases like "(N tests)", "12 tests covering X", or "extensive test
suite (250+ tests)". Two things tend to happen next:

1. The numbers go stale fast. Every new task adds tests; nobody
   remembers to update the README in lockstep.
2. The block becomes update debt without telling the reader
   anything actionable -- the actual current count is one `cargo
   test` away.

The README in this project had already drifted: it advertised
"(9 tests)", "(6 tests)", "(2 tests)" while the real counts at the
time of task 039 were 11, 24, and 3 respectively. The task
proposed refreshing the numbers; the maintainer chose to remove
them instead.

## Decision

The README and other project documentation describe test coverage
with a bulleted "what is covered" list and **no** numeric counters.

Concretely:

- Avoid phrases like "(N tests)", "12 tests for X", "with N
  snapshot tests".
- When listing what is covered, use behavioural descriptions:
  "covers the golden path for `--agenda day/week/month`",
  "covers `conflicts_with` between `--holidays` and the scan
  flags".
- The reader runs `cargo test` to see the current count if they
  care.

Reviewer tasks of the form "refresh the test counters in the
README" are closed with a pointer to this ADR.

## Consequences

Easier:

- A "what is covered" list stays true as the suite grows; the
  README does not have to update on every PR that touches
  tests.
- Less update debt across documentation: a single source of
  truth (`cargo test`) for the current count.
- The reader gets information that is useful (what we cover)
  instead of marketing (how many).

Harder:

- The README loses a familiar coverage signal that some
  readers expect. The trade-off is accepted in favour of
  staying truthful over time.

## References

- README test coverage discussion (`README.md`): see the
  "Testing" or equivalent section.
- Originating proposal: task 039 in the project task log
  (proposed refreshing stale counts; maintainer chose
  removal).
