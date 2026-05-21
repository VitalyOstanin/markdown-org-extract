# ADR-0009: Unified date-window semantics for agenda

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

The CLI exposes four date-shaped arguments that interact with the
agenda window:

- `--date <YYYY-MM-DD>` -- target date for `--agenda day/week/month`.
- `--from <YYYY-MM-DD>` -- start of an explicit range.
- `--to <YYYY-MM-DD>` -- end of an explicit range.
- `--current-date <YYYY-MM-DD>` -- override of "today" for overdue
  calculation and for reproducible runs.

Before this ADR, the interactions between them had several quiet
edge cases:

- `--from` alone (or `--to` alone) was silently dropped: the code
  required both at once, so a partial bound fell through to
  `--date` or to "today" without telling the user.
- `--from` / `--to` were silently ignored under `--agenda day` --
  no `conflicts_with` declaration and no runtime check, so a
  consumer asking for "day agendas from X to Y" got a single day
  agenda instead, with no diagnostic.
- The `--date` argument was silently ignored under `--tasks` and
  under `--agenda tasks` (the tasks-mode does not use dates at
  all). `--from` / `--to` were rejected under `--tasks` but not
  under `--agenda tasks`.
- The README documented independent defaults for `--from` /
  `--to` ("default: Monday of current week"), which the code did
  not implement.

These behaviours surfaced together while triaging tasks #2 and #14
from the project task log and during the review at
[`docs/reviews/2026-05-21-1811-review.md`](../reviews/2026-05-21-1811-review.md).

## Decision

The agenda module exposes a single, uniform window model across
`day`, `week`, and `month` modes. The four date arguments take
explicit roles and explicit priorities.

**Roles:**

- `--date` -- window anchor (which date the agenda is "about").
- `--current-date` -- "today" for overdue / upcoming and the
  default for a missing edge of `--from` / `--to`.
- `--from` / `--to` -- alternative explicit window (range);
  override `--date`.

**Derived value:**

```
current_date := --current-date  if set
             else today in --tz
```

**Window resolution per mode** (`day` / `week` / `month`):

| Arguments         | day                                | week                                  | month                                  |
| ----------------- | ---------------------------------- | ------------------------------------- | -------------------------------------- |
| `--from X --to Y` | N day-agendas over `[X..Y]`        | N day-agendas over `[X..Y]`           | N day-agendas over `[X..Y]`            |
| `--from X` only   | N day-agendas over `[X..current_date]` | N day-agendas over `[X..current_date]` | N day-agendas over `[X..current_date]` |
| `--to Y` only     | N day-agendas over `[current_date..Y]` | N day-agendas over `[current_date..Y]` | N day-agendas over `[current_date..Y]` |
| `--date X`        | one day-agenda for `X`             | day-agendas for the week containing `X` | day-agendas for the month containing `X` |
| none of the above | one day-agenda for `current_date`  | day-agendas for the current week      | day-agendas for the current month      |

**Priorities:**

- `--from` / `--to` (either edge) > `--date` for window selection.
- `--current-date` > `today` for overdue baseline and for the
  missing-edge default.
- `--date` and `--current-date` are independent in purpose:
  `--date` sets the window, `--current-date` sets the overdue
  baseline. When `--date` is omitted, the window of `day` mode is
  `current_date`; the window of `week` / `month` mode is the
  week / month containing `current_date`.

**Validation:**

- `from <= to` after the missing edge is filled in from
  `current_date`. A violation is reported as `AppError::DateRange`.
- In tasks mode (`--tasks` or `--agenda tasks`) all four date
  arguments are rejected. The `conflicts_with = "tasks"` on
  `--from` / `--to` only covers the bool `--tasks` flag, so
  `--agenda tasks` enforces this through a runtime check in
  `main`.

**Overdue / upcoming visibility:**

The existing rule is preserved: overdue and upcoming entries are
attached only to the day where `day_date == current_date`. If
`current_date` falls outside the window, no day inside the window
carries overdue / upcoming entries. This is treated as a feature:
the user is asking to look at a window `[X..Y]` and the overdue
context from a different point in time is not forced into it.

## Consequences

Easier:

- Day, week, and month modes share a single window-resolution path.
  The difference between them shrinks to the default window when
  no `--from` / `--to` / `--date` is given.
- Consumers that need "the next two weeks day by day" pass
  `--agenda day --from <today> --to <today+13>` once instead of
  invoking the binary 14 times.
- The "missing edge" case has a defined, non-silent answer.
- Tasks mode is unambiguously date-free: all date arguments are
  rejected with a diagnostic instead of being silently ignored.

Harder:

- The README's previous "default: Monday of current week"
  description for `--from` / `--to` is wrong under this model and
  has to be rewritten. The new help text spells out the roles of
  `--date` and `--current-date` explicitly.
- Overdue visibility now depends on whether `current_date` is in
  the window. Consumers that batch-build long historical windows
  see no overdue inside those windows, which is the intended
  behaviour but may need to be documented for end users.

## References

- Argument parsing: [`src/cli.rs`](../../src/cli.rs)
- Window resolution and per-day build: [`src/agenda.rs`](../../src/agenda.rs)
- Originating task: #2 in the project task log
  ("Унифицированная семантика окна").
- Supersedes earlier discussion of "X self-anchors" as the
  missing-edge default; that alternative was rejected in favour of
  `current_date` to keep semantics centred on a single time
  reference.
- Related: [ADR-0001](0001-standalone-cli-for-org-in-markdown.md)
  for the JSON wire contract this window model feeds.
