# ADR-0028: A week has a first day, and a month has a grid

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-17). Extends
[ADR-0009](0009-unified-date-window-semantics.md), which fixed how the
date-window arguments interact, with the weekday a week begins on and with
a window upstream does not have. Extended in turn by
[ADR-0030](0030-explicit-window-in-the-month-grid.md), which settles what
the grid does with an explicit `--from`/`--to` window.

## Context

Two questions arrived together, and they turn out to be one.

**Which weekday does a week begin on?** Until now this crate answered
"Monday", with the answer written into the arithmetic:
`get_week_for_date` subtracted `num_days_from_monday` and had no parameter
to say otherwise. Upstream is configurable and says so:
`org-agenda-start-on-weekday` ([`lisp/org-agenda.el`][org-agenda-el] line
1181) takes a weekday number, defaults to 1 (Monday), and takes `nil` for
"start on the current day". Upstream applies it to windows of 7 or 14
days and to nothing else (line 4430). So the fixed Monday was a departure
from the Org semantics this crate exists to reproduce
([ADR-0012](0012-verify-org-semantics-against-upstream.md)).

**Which days does a calendar month show?** Both clients of this crate —
the editor extension and the Android application — draw a month as a
calendar: seven columns, one per weekday, and whole weeks, so the first
and last rows borrow days from the months on either side. Upstream draws
no such thing. `org-agenda` with a month span emits the days of the month
as a list, and the calendar of Emacs receives entries through the
`org-diary` sexp instead of through the agenda. The clients therefore
computed the borrowed days themselves, each in its own language, and the
two implementations had already drifted: the extension took the first day
of the week from a setting, the application hard-coded Monday.

The link between the two questions is that the borrowed days cannot be
computed without knowing which weekday a week begins on. Leaving the grid
to the clients means the same parameter lives in three places — here for
the week window, and once per client for the grid — and nothing keeps the
three in step.

## Decision

Both answers live in this crate.

1. `AgendaDates::week_start` (`--week-start`) names the weekday a week
   begins on: a weekday name, or `today` for upstream's `nil`. The flag
   absent means Monday, which is what every window produced before it
   existed. It reaches the week-shaped windows, as upstream does: the
   `Week` scope and the columns of the grid below.
2. `AgendaScope::MonthGrid` (`--agenda month-grid`) is the whole weeks
   that a calendar month falls in — the month plus what its first and
   last weeks borrow — laid out from `week_start`.

The grid refuses `--week-start today`. A calendar draws one column per
weekday; a week that begins wherever the reader stands has no columns, and
silently reading it as Monday would be the quiet mis-answer that ADR-0009
rejects for the date arguments.

The grid is not a fixed six rows. A month whose edges already fall on the
edges of a week borrows nothing — February 2027 read from a Monday is four
rows — and a client that padded to six would draw a week that is not
there.

## Consequences

- Clients stop computing which days a month shows: they ask for the
  window and lay out what came back, seven to a row. The rule exists
  once, and a fix to it reaches both clients with the version they pin.
- The first day of the week becomes a setting a client can offer, and one
  that changes the week view and the calendar together rather than only
  the calendar.
- The wire format gains no field: the grid is a longer array of the same
  day objects ([ADR-0015](0015-json-schema-evolution.md)). A consumer
  that does not ask for `month-grid` sees no change.
- `--agenda month` stays what it was: the calendar month, no borrowed
  days. Anything reading a month as "the days of this month" keeps
  working.
- Two windows now describe a month, and a caller has to know which one it
  wants. The names carry that: `month` is the month, `month-grid` is the
  grid a month is drawn on.

## References

- [`org-agenda-start-on-weekday`][org-agenda-el] — upstream's first day of
  the week, and its `nil`.
- [ADR-0009](0009-unified-date-window-semantics.md) — the date-window
  model this extends.
- [ADR-0012](0012-verify-org-semantics-against-upstream.md) — why the
  fixed Monday counted as a defect.
- [ADR-0015](0015-json-schema-evolution.md) — why a longer array is not a
  schema change.

[org-agenda-el]: https://git.savannah.gnu.org/cgit/emacs/org-mode.git/tree/lisp/org-agenda.el
