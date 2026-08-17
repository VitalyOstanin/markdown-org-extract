# ADR-0030: An explicit window in the month grid is grown to whole weeks

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-17). Settles a case
[ADR-0028](0028-week-start-and-the-month-grid.md) left open, and applies
the priority [ADR-0009](0009-unified-date-window-semantics.md) fixed
between the window arguments.

## Context

[ADR-0028](0028-week-start-and-the-month-grid.md) added
`AgendaScope::MonthGrid`: the whole weeks a calendar month falls in, laid
out from the first day of the week. It described the window the scope
derives from an anchor day — `--date`, or today — and said nothing about
the other way a window can arrive.

[ADR-0009](0009-unified-date-window-semantics.md) gives `--from`/`--to`
priority over `--date` in every dated scope, so the grid accepted an
explicit window too, and used it verbatim. That produced output the scope
promises not to produce: `--agenda month-grid --from 2026-08-05 --to
2026-08-11` returned seven days beginning on a Wednesday. A client laying
the answer out seven to a row — which is the whole reason the scope
exists — draws a row whose columns are Wednesday through Tuesday, headed
by weekday names that no longer match the cells beneath them.

Three answers were available:

1. Return the window as given. The invariant "a grid is whole weeks from
   `week_start`" then holds only when the window came from an anchor day,
   and every client has to re-check the array it got.
2. Refuse `--from`/`--to` in this scope. That keeps the invariant, but
   breaks the uniform window model of ADR-0009 — a client that passes the
   same window arguments to every scope would have to special-case this
   one — and it removes a window that is genuinely useful: two months of
   grid for a scrolling calendar.
3. Grow the given window to the weeks it touches.

## Decision

An explicit `--from`/`--to` window in `MonthGrid` scope is grown outward
to the whole weeks it touches, beginning on `week_start`: the start moves
back to the first day of its week, the end forward to the last day of
its. A window already aligned on both edges is returned unchanged, so
nothing is padded to a fixed number of rows — the rule ADR-0028 set for
the anchor-derived window holds here too.

The scope therefore has one invariant whatever picked its window: the
answer is a whole number of weeks, and its first day is `week_start`.
`--agenda month` and `--agenda week` are untouched; a week agenda is a
list of days and reads correctly however it is bounded.

## Consequences

- A client draws the array in rows of seven without inspecting it, from
  any window: `month-grid` for the current month, `month-grid` with
  `--from`/`--to` for a scrolling multi-month calendar.
- The window that comes back can be wider than the one asked for, by up
  to six days at each edge. A caller that needs the days exactly as
  bounded wants `--agenda day` or `--agenda week`, which give them.
- The behaviour changed within the same scope after `0.17.0` was tagged
  locally and before it was published, so no released version returned
  the ragged window.

## References

- [ADR-0028](0028-week-start-and-the-month-grid.md) — the grid scope and
  the first day of the week it is laid out from.
- [ADR-0009](0009-unified-date-window-semantics.md) — the priority of
  `--from`/`--to` over `--date` this decision keeps.
- Implementation: [`src/agenda.rs`](../../src/agenda.rs)
  (`grow_to_whole_weeks`, `resolve_period_window`).
