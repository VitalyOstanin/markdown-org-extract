# ADR-0038: A moved occurrence is written inside the series

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-09-05). Supersedes the replacement half of
[ADR-0031](0031-exceptions-to-a-repeating-entry.md) — `SERIES_ID` and
`RECURRENCE_ID` — for what this project writes; both are still read, and files
already carrying them keep working. The `EXDATE` half of ADR-0031 is untouched:
an occurrence that is gone is still cancelled the way it was. Non-breaking JSON
addition under [ADR-0015](0015-json-schema-evolution.md).

## Context

ADR-0031 answered "this Thursday the class is at six" the way iCalendar does:
a second entry, carrying the `ID` of the series and the start the occurrence
would have had. That shape maps directly onto RFC 5545 and onto Google
Calendar, and it gives the moved occurrence a state, a body and clocks of its
own.

Using it on real notes showed what it costs the reader, and the cost is not in
the model but in the file:

- The replacement is written at the end of the file, which is where two
  devices can both append without conflicting. A reader looking at the series
  sees no sign that one of its occurrences moved; the answer is thousands of
  lines away, under whatever heading happens to be last.
- Its heading is copied from the series, and a heading copied to the end of a
  file lands under a different parent: `## English` written after a `# crates.io`
  section becomes a subsection of that section. Nothing reads headings for
  hierarchy here, but the file is read by people, and to a person it now says
  something false.
- Two entries with the same title, one of them a stub of the other, is what
  the reader has to keep in mind when editing either.

The occurrence being moved is not a separate thing the notes are about. It is
the series, on a different day. Everything else about it — what it is, what
state it is in, what was written under it — is the series' and stays there.

## Decision

A moved occurrence is a line of the series entry:

````text
## English on Mondays at 15:00
`CREATED: [2025-12-08 Mon 01:06]`
`<2025-12-08 Mon 15:00 +1w>`
`MOVED: 2026-09-07 -> <2026-09-09 Wed 13:00>`
````

Rules:

- The keyword is `MOVED:`, in the `UPPER_SNAKE_CASE` of
  [ADR-0020](0020-task-properties-org-properties-block.md), on a planning line
  of the entry — the same inline-code line every timestamp of these notes is
  written on.
- Before the arrow is the occurrence, as `YYYY-MM-DD`: the day the series draws
  it on, which is what identifies one repeat of an endless series. After the
  arrow is where it is held instead, as an active timestamp.
- The target may carry a **weekday**, a **time**, and a **time range**. It may
  not carry a **repeater** or a **warning cookie**: one occurrence does not
  repeat, and how far ahead a `DEADLINE` warns belongs to the series. Either
  refuses the line — it is reported and the move does not happen, rather than
  being read with the offending token dropped.
- An entry may hold as many `MOVED` lines as it has moved occurrences. Two
  naming the same occurrence are a file that cannot be resolved, so the second
  is refused and the first stands.
- The occurrence named before the arrow is **not** drawn on its own day, for
  the same reason a replacement suppressed it under ADR-0031, and it carries
  the same consequence for arrears
  ([ADR-0032](0032-what-a-missing-occurrence-owes.md)): it moved rather than
  went, so the debt travels with it.
- The moves are parsed into `Task::moved_occurrences`, a list of
  `{from, to, time?, end_time?}`, under the schema-evolution rule of ADR-0015.
- `SERIES_ID` and `RECURRENCE_ID` are still read exactly as ADR-0031 describes.
  Files written before this decision, and by any other tool that follows RFC
  5545's shape, keep resolving.

## Consequences

- The reader sees a moved occurrence where the series is, which is the whole
  point. Nothing is appended to the end of the file, and no heading is copied
  to a place it does not belong to.
- A moved occurrence loses what a separate entry gave it: its own state, body,
  priority and clocks. Marking one occurrence `DONE`, or writing a note under
  it, is no longer expressible — that is the trade this decision makes, and the
  reason ADR-0031's shape is still read rather than dropped.
- One entry now yields agenda cells on days its repeater does not name. Every
  pass that asks "does this series occur on this day" consults the moves as
  well as the exclusions.
- A move does not need the series to carry an `ID`. The pair that needed one —
  `SERIES_ID` matching an `ID` — existed to point across entries, and a line
  inside the entry points at nothing.
- The Google Calendar export still has a direct mapping: a `MOVED` line is
  (`UID`, `RECURRENCE-ID`) with the series' identifier and the day before the
  arrow, and the instance it writes is the timestamp after it.
- A move reaches only as far as the entry it is written in, which is a
  narrower and more predictable reach than ADR-0031's "as far as the scan".
  The silence that decision accepted — a replacement in a file the scan never
  read — cannot happen to a move.

## References

- [ADR-0031](0031-exceptions-to-a-repeating-entry.md) — the shape this one
  replaces for writing, and still reads.
- [ADR-0032](0032-what-a-missing-occurrence-owes.md) — what a missing
  occurrence owes, which a move answers the same way a replacement did.
- [ADR-0015](0015-json-schema-evolution.md) — how `moved_occurrences` is added
  without breaking consumers.
- [ADR-0020](0020-task-properties-org-properties-block.md) — the key
  convention the keyword follows.
- [RFC 5545](https://datatracker.ietf.org/doc/html/rfc5545), section 3.8.4.4
  (`RECURRENCE-ID`) — the model the export still speaks.
