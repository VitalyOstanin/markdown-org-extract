# ADR-0031: Exceptions to a repeating entry, in the iCalendar shape

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-21).

## Context

A repeating timestamp is one line describing an endless series:
`<2026-08-20 Thu 15:00 +1w>` says the class is at three every Thursday.
There is nowhere in that line to say that this Thursday it is at six, and
Org-mode offers nothing for it either: the manual's section on repeated
tasks describes shifting the base date on completion and the warning
period, and names no way to exclude or move a single occurrence
(`doc/org-manual.org`, "Repeated tasks"). Its own answer is to stop
having a series — `org-clone-subtree-with-time-shift` writes N copies
with the repeater removed from each (`lisp/org.el`). The single
exclusion Org does have, the `org-class` diary sexp
(`lisp/org-agenda.el`), skips whole ISO weeks and holidays for a sexp
entry, not one occurrence of a `SCHEDULED` or `DEADLINE`. Its iCalendar
bridge does not close the gap either: org-caldav gained repeating events
in 2024 and states that complex recurrences are unsupported, and `EXDATE`
occurs in `ox-icalendar.el` only inside a comment.

The need is ordinary — a weekly class moved once, a meeting cancelled on
a holiday — and every calendar system solves it the same way. iCalendar
(RFC 5545) identifies an occurrence by the start it would have had
(`RECURRENCE-ID`), replaces it with an event carrying the same `UID` plus
that identifier, and deletes occurrences outright with `EXDATE`; Google
Calendar exposes the same model as `instances` with `originalStartTime`.
The other family of answers materialises every occurrence as a record of
its own (Taskwarrior's template and instances, Obsidian Tasks writing the
next line on completion, `rec:` in todo.txt tooling).

This project cannot take the second family without changing what a
repeating entry is, and it does not need to: it already has a place to
put per-task keys — the `org-properties` block of
[ADR-0020](0020-task-properties-org-properties-block.md), where the `ID`
key is already the stable per-task identifier.

The compatibility argument that would otherwise block an invented key is
weaker here than it looks. These notes are Markdown;
[ADR-0002](0002-supported-org-mode-subset.md) asks that a timestamp line
still parse in Emacs, not that `org-agenda` run over this directory. A
property Org does not know costs recognisability, not interoperability.

## Decision

Exceptions are expressed as properties, in the iCalendar shape.

| Piece                       | Property                                     | On                     |
|-----------------------------|----------------------------------------------|------------------------|
| The series                  | the repeating timestamp, unchanged            | the series entry       |
| An occurrence cancelled     | `EXDATE: 2026-08-20, 2026-08-27`             | the series entry       |
| An occurrence replaced      | `SERIES_ID: <id of the series>` and `RECURRENCE_ID: 2026-08-20 15:00` | the replacing entry |

Rules:

- **`EXDATE`** holds one or more `YYYY-MM-DD` dates, separated by commas
  and/or whitespace. Each names an occurrence the series does not have.
  An unparsable date is skipped and reported through the same capped
  warning path as a malformed timestamp.
- **`RECURRENCE_ID`** holds `YYYY-MM-DD`, optionally followed by `HH:MM`.
  It names the occurrence this entry replaces — the date the occurrence
  *would have had*, not the date this entry carries.
- **`SERIES_ID`** holds the `ID` of the series entry. The pair
  (`SERIES_ID`, date of `RECURRENCE_ID`) is what the resolver matches;
  this is RFC 5545's (`UID`, `RECURRENCE-ID`) pair, spelled in
  `UPPER_SNAKE_CASE` because that is the convention ADR-0020 fixed.
- A **replacement suppresses the occurrence it names** without an
  `EXDATE` beside it, which is how RFC 5545 has it: `EXDATE` is for an
  occurrence that is gone, a replacement is for one that moved.
- Suppression is at **day granularity**, because the agenda draws at most
  one occurrence of a series per day. An hourly repeater excluded for a
  date loses that whole day; the time in `RECURRENCE_ID` is carried for
  the reader and for export, not matched on.
- The three keys are parsed into optional fields on `Task` —
  `excluded_dates`, `recurrence_id`, `series_id` — under the
  schema-evolution rule of [ADR-0015](0015-json-schema-evolution.md),
  so a consumer that predates them ignores them and reads the raw keys
  from `properties` as before.
- The replacing entry is an **ordinary entry** everywhere else: it has
  its own state, body, clocks and priority, and appears on its own date
  through the ordinary path.

## Consequences

- The series stays one heading. What a reader edits when moving one
  occurrence is a separate entry, so its `DONE`, its notes and its clock
  do not leak into the series.
- The resolver reads a second source: every pass that answers "does this
  series occur on this day" — the per-day buckets, `timestamp_next` and
  `timestamp_next_after` — has to consult the exclusion set first. The
  set is built once per run.
- Building the set needs the whole task list, so an exception only
  applies within a scan that holds both entries. Two notes in different
  scanned roots still work, since the scan is one run; a replacement in a
  file excluded by a filter silently stops replacing, and the series
  occurrence returns.
- A core older than this change ignores the properties and shows both the
  series occurrence and the replacement on that day. That is the same
  degradation ADR-0015 accepts for every additive field.
- The extension's Google Calendar export gains a direct mapping —
  `EXDATE` to `EXDATE`, (`SERIES_ID`, `RECURRENCE_ID`) to (`UID`,
  `RECURRENCE-ID`) — instead of having to invent one.
- `RANGE=THISANDFUTURE`, iCalendar's third piece, is deliberately left
  out. It splits a series in two, which is a different operation from
  moving one occurrence, and Org has no `UNTIL` to express the truncated
  half with. If it is ever wanted, it arrives as its own ADR.

## References

- [ADR-0002](0002-supported-org-mode-subset.md) — the Org subset, and
  what "still parses in Emacs" means here.
- [ADR-0015](0015-json-schema-evolution.md) — how the new fields
  are added without breaking consumers.
- [ADR-0020](0020-task-properties-org-properties-block.md) — the
  `org-properties` block and the `ID` key.
- [ADR-0023](0023-next-occurrence-field.md) and
  [ADR-0029](0029-next-occurrence-after-the-rendered-day.md) — the two
  next-occurrence fields the exclusion set now feeds.
- [RFC 5545](https://datatracker.ietf.org/doc/html/rfc5545), sections
  3.8.5.1 (`EXDATE`) and 3.8.4.4 (`RECURRENCE-ID`).
- [Google Calendar: recurring events](https://developers.google.com/workspace/calendar/api/guides/recurringevents)
  — `instances` and `originalStartTime`, the same model in an API.
