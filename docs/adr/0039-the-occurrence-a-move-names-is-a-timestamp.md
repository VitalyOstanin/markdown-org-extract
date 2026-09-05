# ADR-0039: The occurrence a move names is written as an inactive timestamp

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-09-05). Amends the written form of
[ADR-0038](0038-a-move-is-written-inside-the-series.md): what stands before the
arrow is an inactive timestamp rather than a bare date. The bare date is still
read, so files written by 0.22.0 keep working. Everything else ADR-0038 decided
— where the line stands, what the target may carry, one line per occurrence —
is unchanged.

## Context

ADR-0038 wrote the day being moved bare and the day it moves to bracketed:

```markdown
`MOVED: 2026-09-07 -> <2026-09-09 Wed 13:00>`
```

The asymmetry was deliberate. A bare date is not a timestamp, so an editor
walking dates with its arrow keys walks only the target, and the address of the
occurrence cannot be nudged by accident.

Using it showed the cost of that choice. The line reads as two halves written
in two notations, which is what a reader notices first. And the protection has
a price a reader pays every time it is wrong: correcting *which* occurrence
moved means editing text by hand, in a line where every other date is edited
with the keys the editor offers for dates. The extension highlights timestamps
and steps them with Shift+Up / Shift+Down; a bare date is neither highlighted
nor stepped.

The two halves are not the same kind of thing, though, and the format should
say so. The day before the arrow is an address — which occurrence of the series
the line is about. The timestamp after it is when the occurrence is actually
kept. Org already distinguishes exactly this: an active timestamp feeds the
agenda, an inactive one is a date written down.

## Decision

The occurrence being moved is written as an **inactive** timestamp, with the
weekday the series spells:

```markdown
`MOVED: [2026-09-07 Mon] -> <2026-09-09 Wed 13:00>`
```

Both halves are now timestamps, so both are highlighted and both are walked by
whatever keys an editor gives dates. The bracket form says which is which: the
address is inactive, the time kept is active.

What the left half may carry is as narrow as the right half's. It is refused,
not read past, when it is written active (that would say the entry is kept
then), when it names an hour (a series draws at most one occurrence a day, so
a day addresses one), and when it carries a repeater or a warning cookie (both
belong to the series rather than to one occurrence).

The bare date of ADR-0038 is still read and produces the same result. It is not
rewritten on read: what a file says is what it says.

## Consequences

- The line is written in one notation, and every date in it is edited the same
  way. Correcting which occurrence moved no longer means typing over a date.
- The address can now be nudged with the same keys that move the target, which
  is what makes it editable. An accidental step changes which occurrence the
  line is about; the editor's own undo takes it back, and the day is spelt with
  its weekday, which makes a wrong step visible rather than silent.
- Two written forms are read where one was written before. The reader carries
  the bare date for as long as files hold it, which is indefinitely.
- The refusals grow by four, all on the left half, and all reported through the
  same `org-properties` channel the rest of the exceptions use.
- Clients that write the line — the VS Code extension, the Android application
  — write the new form and read both. A file passed between an old client and a
  new one is read by both; only what is written differs.

## References

- [ADR-0038: A moved occurrence is written inside the series](0038-a-move-is-written-inside-the-series.md)
- [ADR-0014: Active and inactive timestamps](0014-active-and-inactive-timestamps.md) —
  the bracket policy this decision leans on: an inactive timestamp never feeds
  the agenda, which is precisely the address's role.
- [ADR-0022: Amend ADRs by reference](0022-amend-adrs-by-reference.md)
