# ADR-0023: `timestamp_next` — resolved next occurrence for repeaters

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-07-25). Non-breaking JSON addition governed by
[ADR-0015](0015-json-schema-evolution.md).

## Context

A repeating task's stored timestamp is its current active occurrence,
which stays in the past (overdue) until the task is marked done. The
consumer [`markdown-org-vscode`](https://github.com/VitalyOstanin/markdown-org-vscode)
shows a repeat tooltip that read "next `<stored date>`" — a label that
is wrong whenever the stored occurrence is already overdue (it names a
past date as "next").

The consumer first fixed this in TypeScript by re-deriving the next
occurrence in the webview: parsing the repeater grammar and stepping the
calendar. That duplicates logic this CLI already owns — `parse_repeater`
plus `closest_date(base, reference, DatePreference::Future, repeater)` is
exactly "the closest occurrence on or after a reference date", with
tested handling of day/week/month/year/hour/workday units. Two
implementations of the same semantics drift (the hour unit, for one, is
deliberately projected onto a daily grid here — see `closest_date`).

The producer already knows "now": the agenda reference date is
`--current-date` or today, and the local wall-clock time is available via
`Utc::now()` in the configured timezone. So the computation belongs here.

## Decision

### New optional field `timestamp_next`

Each `Task` gains an optional `timestamp_next: Option<String>`
(`YYYY-MM-DD`). For a task that carries a repeater it holds the closest
still-upcoming occurrence relative to "now":

- a date before today rolls forward to the first occurrence today-or-later;
- an occurrence on today stays today when the task has no clock time, or
  when its time is still ahead of the local now;
- an occurrence on today whose clock time has already passed rolls to the
  following occurrence.

It is absent (serialised via `skip_serializing_if`) for non-repeating
tasks and when the date or repeater cannot be parsed. Computed by
`agenda::next_occurrence`, a thin wrapper over `closest_date`.

The three repeater modifiers (`+`, `++`, `.+`) describe how the *stored*
stamp advances on completion, which is the editor's concern; they select
the same calendar grid here, so all three yield the same value.

### Anchored on the task's own timestamp, computed once

The field is filled on the input tasks, before the agenda is built, and
travels into every cell the task lands in. Two properties follow, both
load-bearing:

- **The anchor survives.** `push_scheduled_occurrence` /
  `push_overdue_occurrence` rewrite `timestamp_date` on the copy they
  render. Computing from that rewritten value would restart the grid from
  a truncated date, and a monthly repeater anchored on the 31st would
  drift to the 30th and never return (`bracket_month` truncates 31.01 to
  28.02 and keeps that day-of-month afterwards).
- **The value is single-valued per task.** The field answers "when does
  this come round next", so the same task must not report a different
  "next" in each day cell it appears in.

Computing once per task rather than once per rendered occurrence is also
what keeps the cost proportional to the input: a month window can
materialise the same task tens of times.

### Agenda modes only; `tasks` mode stays date-less

`timestamp_next` is emitted only in the date-windowed modes
(`--agenda day/week/month`), which have a concrete reference date. The
`--tasks` mode is deliberately date-less (see
[ADR-0009](0009-unified-date-window-semantics.md)); injecting a
now-relative field there would make its output non-deterministic and
break the byte-exact wire-contract snapshots. Consumers that need "next"
in a flat list request an agenda mode.

### Reference instant

The reference is the local wall-clock now, in the `--tz` timezone, and is
independent of the requested window: `--date`/`--from`/`--to` move which
days are rendered, not what "now" means. Under a `--current-date`
override (tests, pinning) the time is unknown, so midnight is used —
making the date-level rolling deterministic while the sub-day
"time already passed today" branch only engages against a real clock.
The reference date and this instant are read from a single `Utc::now()`,
so a run crossing midnight cannot mix a date from one day with an instant
from the next.

## Consequences

Easier:

- One implementation of repeater advancement, in Rust, already tested.
  The consumer drops its parallel TypeScript rolling and only formats the
  provided date.
- The sub-day correctness ("14:00 task viewed at 22:00 is past") lives
  where the wall clock and timezone already are.

Harder:

- `--tasks` mode does not carry `timestamp_next`; a consumer that wants it
  in a flat list must use an agenda mode. Documented in the README.
- One more coordinated release: minor bump here, `extractorVersion` bump
  in the consumer, per ADR-0015.

## References

- [ADR-0015](0015-json-schema-evolution.md) (non-breaking JSON additions,
  consumer coordination via `extractorVersion`).
- [ADR-0009](0009-unified-date-window-semantics.md) (the date-less
  `tasks` mode this field deliberately skips).
- Implementation: [`src/agenda.rs`](../../src/agenda.rs)
  (`next_occurrence`, `annotate_next_occurrences`, `set_next_occurrence`),
  [`src/types.rs`](../../src/types.rs) (`Task::timestamp_next`),
  [`src/timestamp/repeater.rs`](../../src/timestamp/repeater.rs)
  (`closest_date`).
