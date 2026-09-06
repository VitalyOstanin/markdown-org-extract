# ADR-0040: What a move conflicts with, and what it may not invent

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-09-07). Settles four cases
[ADR-0038](0038-a-move-is-written-inside-the-series.md) left open, and changes
none of what it decided: where the line stands, how it is addressed, and what
either half may carry are unchanged.

## Context

ADR-0038 introduced a line that holds one occurrence of a series on another
day, and ADR-0039 settled how it addresses that occurrence. Neither says what
happens when a move meets something else that speaks about the same day. Four
such cases exist, and the code answered all four by accident rather than by
decision — three of them defensibly, one of them not:

1. **Two moves land on one day.** Two occurrences of a series can be
   rescheduled onto a single day at different hours; a course whose Thursday
   and the Thursday after both move to one Saturday is an ordinary thing to
   write.
2. **A move lands on a day the series already draws.** A Thursday occurrence
   moved onto the next Thursday meets the occurrence that Thursday already has.
3. **An `EXDATE` names the day a move holds an occurrence on.** ADR-0031's
   cancellation and ADR-0038's move both speak about that day.
4. **A move addresses a day the series does not have.** `MOVED: [2026-09-08
   Tue] -> <2026-09-20 Sun>` on a series of Mondays addresses nothing: the 8th
   is a Tuesday, and no occurrence falls there.

The fourth case was read like any other. The day before the arrow was never
compared against the series, and the day after it was drawn regardless — so a
line that addressed nothing added an occurrence the entry never had. That is
an operation this format does not have and has deliberately not decided on
(TODO.md, "A series gains occurrences as well as losing them"), and it arrived
through a keyword that says nothing about adding. A mistyped address produced
it silently.

## Decision

The first three cases are conflicts between two true statements, and both
statements stand:

1. **Two moves onto one day both draw.** The day holds two occurrences of the
   entry, each at the hour its own line names. Drawing only the first left the
   second nowhere: off the day the series would have drawn it, which its move
   took it from, and off the day it moved to.
2. **A move onto an occupied day leaves both.** The day holds the occurrence
   the series draws there and the one held there by the move. They are two
   occurrences of one series, and neither line says the other is gone.
3. **An `EXDATE` on the target day does not cancel the move.** The `EXDATE`
   speaks about the occurrence the series draws on that day; the move names
   that day itself, and naming it is the more particular statement. To cancel
   a moved occurrence, remove the move.

The fourth is not a conflict but a line that cannot mean what it says, and it
is **refused and reported**, through the same `org-properties` warning channel
every unusable exception uses. A day is an occurrence when the series the
entry's own timestamp describes falls on it — the same grid the agenda walks,
so a day accepted here is a day the agenda would have drawn. Two shapes of the
same refusal come with it: an entry whose timestamp does not repeat has one
occurrence, which is the timestamp itself and is edited rather than moved; an
entry with no timestamp draws no series for a line to address.

## Consequences

- A move can no longer give a series a day it never had. The only way to add
  an occurrence remains a separate entry, and whether the format should have
  an operation of its own for it stays open where it was.
- A mistyped address is now visible. It was the failure this format cares
  most about — a file that looks like it moves an occurrence and moves none —
  and it was the one case of it that passed silently.
- Files already holding such a line change behaviour: the day it drew goes
  empty and a warning names the line. That is the point of the change, and it
  is the reason it is written down rather than fixed quietly.
- Reading a move now depends on the entry's timestamp and repeater, which the
  parser already has where the line is read. The check is one call per line
  and keeps the reading of an entry's moves linear in their number.
- Clients that write the line — the VS Code extension, the Android application
  — repeat the rule: each has its own reader, and a rule the core enforces
  alone would let a client write a line the core then refuses. The extension
  reports it where it reports the rest of the line's diagnostics.

## References

- [ADR-0038: A moved occurrence is written inside the series](0038-a-move-is-written-inside-the-series.md)
- [ADR-0039: The occurrence a move names is written as an inactive timestamp](0039-the-occurrence-a-move-names-is-a-timestamp.md)
- [ADR-0031: Exceptions to a repeating entry](0031-exceptions-to-a-repeating-entry.md) —
  the `EXDATE` the third case weighs a move against.
- [ADR-0032: What a missing occurrence owes](0032-what-a-missing-occurrence-owes.md)
- TODO.md, "A series gains occurrences as well as losing them" — the operation
  the fourth case must not become.
