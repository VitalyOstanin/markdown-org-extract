# ADR-0032: What a missing occurrence owes, and what comes after it

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-21).

## Context

[ADR-0031](0031-exceptions-to-a-repeating-entry.md) says which occurrences a
series does not have, and by which of two routes: `EXDATE` cancels one, a
replacing entry moves one. It says what that means for the day the occurrence
would have fallen on — nothing is drawn there — and stops.

The agenda asks two further questions about the same series, and neither is
answered by "the day is empty":

- **What is owed.** The arrears bucket carries the last occurrence that is
  past and still standing. When that occurrence is missing, the walk steps
  back to the one before it.
- **What is coming.** The day cells, the deadlines inside their warning
  window, `timestamp_next` and `timestamp_next_after` all name the next
  occurrence. When that occurrence is missing, the walk steps forward.

The first implementation treated the two routes as one and answered both
questions from a single set of dates. That produced two results worth naming.
A series whose moved occurrence was walked past came out looking *older* than
before the move: `SCHEDULED: <2026-08-06 Thu +1w>` with the 20th moved to the
22nd was owed the 13th on the 24th — eleven days back — where before the move
it was owed four. And a repeating `DEADLINE` whose first occurrence was
cancelled fell out of the warning bucket entirely while `timestamp_next`, in
the same payload, named the occurrence that followed it: the JSON said the
series was alive and contradicted itself one field later.

## Decision

The two routes are kept apart from the parser to the agenda, and each
question is answered from the part that concerns it.

| Question                                  | Cancelled (`EXDATE`)              | Replaced (`SERIES_ID` + `RECURRENCE_ID`) |
|-------------------------------------------|-----------------------------------|------------------------------------------|
| Is the occurrence drawn on its day?       | no                                | no — the replacing entry is drawn instead |
| Is the series owed for it?                | no; the debt is the earlier one   | no; the debt is the replacing entry's     |
| Does the walk carry on past it?           | backward: yes; forward: yes       | backward: no, it ends; forward: yes       |

- **A cancelled occurrence never was.** Nothing is owed for it, and the debt
  is whichever earlier occurrence still stands, however many are cancelled in
  a row.
- **A replaced occurrence took place**, on the date the replacing entry
  carries. The series owes nothing for it, and the arrears walk ends there
  rather than reaching further back: the debt exists, it is simply not the
  series' — it belongs to the entry that replaced the occurrence, which
  carries its own state and appears in the arrears on its own date.
- **Every forward answer names the next occurrence that stands.** A missing
  one moves the answer along; it never empties it. That holds for the day
  cells, for a `DEADLINE` inside its warning window, and for both
  next-occurrence fields, so the buckets and the fields of one payload agree.

## Consequences

- `ExcludedOccurrences` carries two sets, and the only question that reads
  them apart is the arrears walk. Everything else asks "is this date
  missing" and gets one answer.
- An `EXDATE` written beside a replacement — redundant under ADR-0031, but
  legal — is read as a replacement: the occurrence is somewhere, so the
  series is not owed for it.
- A series every one of whose past occurrences is cancelled is owed nothing,
  which is the same answer as a series that never ran.
- Moving an occurrence no longer changes what the series is owed for the
  occurrences before it. Cancelling one still does, and that is the
  difference the two routes exist to express.
- The distinction is invisible in the JSON: `excluded_dates`,
  `recurrence_id` and `series_id` are unchanged by this decision. What
  changes is which entries land in `overdue` and `upcoming`.

## References

- [ADR-0031](0031-exceptions-to-a-repeating-entry.md) — the two routes and
  the properties that spell them.
- [ADR-0023](0023-next-occurrence-field.md) and
  [ADR-0029](0029-next-occurrence-after-the-rendered-day.md) — the two
  next-occurrence fields that step forward past a missing occurrence.
- [RFC 5545](https://datatracker.ietf.org/doc/html/rfc5545), section 3.8.4.4
  — `RECURRENCE-ID` as the instance that stands in for the one it names.
