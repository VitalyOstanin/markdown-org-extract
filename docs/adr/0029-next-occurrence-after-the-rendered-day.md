# ADR-0029: `timestamp_next_after` — the occurrence after the day being drawn

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-17). Non-breaking JSON addition governed by
[ADR-0015](0015-json-schema-evolution.md); extends
[ADR-0023](0023-next-occurrence-field.md), which it does not replace. Narrowed
by [ADR-0031](0031-exceptions-to-a-repeating-entry.md) (2026-08-21) in the same
way as ADR-0023: the occurrence after this cell is the next one the series
actually has.

## Context

[ADR-0023](0023-next-occurrence-field.md) added `timestamp_next` and made
it deliberately single-valued per task: "when does this come round next"
relative to now, the same value in every cell the task lands in. That is
the right answer for a flat list and for the overdue rows an agenda
borrows into today.

It is the wrong answer under the cursor. A consumer reading the week of
17.08.2026 opens the row of a `+1d` task on the 18th and its repeat
tooltip says "repeats — next 17.08": a date behind the row it is attached
to. The reader is looking at a particular day, so "next" reads as "next
after this one", and the field cannot mean both things at once without
breaking the property ADR-0023 chose on purpose.

Deriving it in the consumer is what ADR-0023 already refused for
`timestamp_next`: the repeater grammar (`++1m` landing on month ends,
`.+1w`, `+3wd` against the RF holiday calendar) lives here and is tested
here. Two consumers means a third and a fourth implementation — Kotlin in
[`markdown-org-android`](https://github.com/VitalyOstanin/markdown-org-android)
and TypeScript in
[`markdown-org-vscode`](https://github.com/VitalyOstanin/markdown-org-vscode).

## Decision

### New optional field `timestamp_next_after`

Each `Task` gains an optional `timestamp_next_after: Option<String>`
(`YYYY-MM-DD`): the first occurrence strictly after the date this copy is
rendered on. For a `+1d` task drawn on the 18th it is the 19th, whatever
"now" happens to be.

It is filled where a repeating task is drawn on a day of its own —
`push_scheduled_occurrence`, so the `scheduled_timed` and
`scheduled_no_time` buckets. It is absent everywhere else, and absent for
non-repeating tasks.

### `overdue` and `upcoming` keep answering from now

The entries in those two buckets are not drawn on their own date: they
are copies of a task from elsewhere in time, borrowed into the reference
day to say "this was missed" or "this is coming". The question there is
unchanged — when does it come round next from now — and `timestamp_next`
already answers it. Filling `timestamp_next_after` for them would name
the occurrence after a day the reader is not looking at.

### Anchored on the task's own date, and on the same condition

The value is computed from the task's stored date, not from the date the
copy was rewritten to. This is the anchor argument of ADR-0023, which
applies here for the same reason: restarting a monthly repeater from an
occurrence `bracket_month` truncated to 28.02 loses the 31st for good.

The clock time plays no part. The field answers which day comes next, so
the search starts from midnight of the day after the cell; a task at
14:00 read at 22:00 still names tomorrow rather than skipping a day.

Computation is gated on `timestamp_next` already being present, so a
caller that did not ask for the now-relative field does not pay for this
one either. Both are printed only by the JSON renderer, and the guard is
what keeps `--format md` off the bill: on a month grid of a few thousand
repeating tasks, 121 ms without the field against 140 ms with it when the
gate is removed.

### Per occurrence, not per task

Unlike `timestamp_next`, this one cannot be computed once per task in
`annotate_next_occurrences`: its value is a function of the cell. It is
therefore computed per rendered occurrence, which is the cost ADR-0023
avoided — measured at roughly 0.25 µs per day copy, or +22 ms of `json`
over the 78 000 copies a 42-day grid of 2 000 tasks materialises.

## Consequences

Easier:

- A repeat tooltip on a dated row names a date ahead of that row, in
  both consumers, without either of them parsing repeaters.
- The two questions are separate fields, so neither answer had to be
  compromised: ADR-0023's "same in every cell" property survives intact.

Harder:

- Consumers now choose between two fields by bucket: `timestamp_next_after`
  on a dated row, `timestamp_next` on an overdue or upcoming one. The
  choice is documented in the README; a consumer that ignores the new
  field keeps its current behaviour.
- The cost is per rendered occurrence rather than per task, and grows
  with the window. The gate on `timestamp_next` keeps it inside the mode
  that prints it.
- One more coordinated release: minor bump here, `extractorVersion` bump
  in the consumers, per ADR-0015.

## References

- [ADR-0023](0023-next-occurrence-field.md) (`timestamp_next`, the
  now-relative field this one sits beside).
- [ADR-0015](0015-json-schema-evolution.md) (non-breaking JSON additions,
  consumer coordination via `extractorVersion`).
- [ADR-0009](0009-unified-date-window-semantics.md) (the date-less
  `tasks` mode, which carries neither field).
- Implementation: [`src/agenda.rs`](../../src/agenda.rs)
  (`next_after_day`, `push_scheduled_occurrence`),
  [`src/types.rs`](../../src/types.rs) (`Task::timestamp_next_after`).
