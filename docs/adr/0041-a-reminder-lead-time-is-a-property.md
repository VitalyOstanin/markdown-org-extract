# ADR-0041: A reminder's lead time is written as a property, and read once

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-09-08).

## Context

A reminder is planned by the client that shows it, and how far ahead of an
occurrence it arrives is a setting of that client: one lead time for every
entry it reminds about. An entry that needs its own -- an hour before a call,
a day before a flight -- cannot say so, and neither can a phrase that edits
it ("remind me an hour before").

Three places could hold such a statement, and one of them is already taken.
The warning cookie of a timestamp, `<2026-09-08 Tue -3d>`, reads as a lead
time and is not one: it names the window in which the agenda starts showing a
deadline, converted to whole days by the multipliers upstream `org-get-wdays`
uses (`d=1`, `w=7`, `m=30.4`, `y=365.25`, `h=1/24`). Giving it a second
meaning would make one cookie mean the window here and the reminder there,
and the conversion it carries cannot express "fifteen minutes before" at all.

The remaining two are a property of the entry and a line of its own. A
property costs no new syntax: the ```` ```org-properties ```` block already
reads arbitrary keys into `Task.properties`, and both clients had already
written the key down as the shape they expected -- markdown-org-android's
ADR-0034 names `REMINDER: 30m` as a lead time "that would be read by the
core, would have to mean the same in the editor extension", and deferred it
for exactly that reason.

What the core owes is therefore not the reading of the key but the reading of
its value, and the shape in which the value arrives. Two constraints bound
that shape. A unit letter must not mean one thing in a repeater and another
here: `+1m` already adds a month, so `m` is a month. And the core cannot hand
back a ready moment: an entry with a day but no hour is reminded about at the
client's digest hour, which is a setting of a device the core knows nothing
about.

## Decision

**The lead time is the `REMINDER` key of the entry's ```` ```org-properties ````
block**, written as a number and a unit: `min` minutes, `h` hours, `d` days,
`w` weeks, `m` months, `y` years. `REMINDER: 30min` is half an hour before,
`REMINDER: 1m` is a month before. Minutes are spelled `min` because `m` is
already a month, in a repeater and now here.

**The core reads the value, once.** It parses the key into a pair -- a number
and a unit -- and hands that pair back on `Task` and in the JSON of
`parse-phrase`. It is not reduced to minutes: a month and a year have no
fixed length, and reducing them would either lose the statement or invent a
length for it. A client that needs a moment subtracts the pair from the
occurrence it is reminding about, by the calendar for months and years, the
way a repeater adds them.

**A value that cannot be read is refused and reported** through the same
capped `org-properties` warning channel a malformed property line and an
unusable exception use, and the entry keeps no lead time of its own: it falls
back to the client's setting, as an entry without the key does.

**A phrase can set the lead time and remove it.** The parsed entry carries
the pair, and clearing it is one of the fields a phrase can clear, alongside
the date, the time, the repeater and the priority.

## Consequences

- The syntax is read in one place. A client that wrote its own reader would
  be the second answer to one question -- the defect the `MOVED` line already
  produced once, when the extension read it separately from the core.
- Clients keep their setting, and the key overrides it per entry. That is
  what ADR-0034 planned for on the Android side, and it stays true for the
  extension.
- The warning cookie keeps its one meaning. An entry may carry both: a
  deadline shown three days ahead and a reminder an hour before are different
  statements about it, and now they are written differently.
- Subtracting a month is calendar arithmetic, not a duration, and it happens
  in the client rather than the core. A client that only handles minutes and
  hours can say so by refusing the value it cannot use; it may not silently
  turn a month into thirty days.
- An unreadable `REMINDER` behaves like an entry that never had one. The
  warning is the only trace, which is the same bargain every other property
  mistake in this format makes.

## References

- [ADR-0020: Task properties via an `org-properties` fenced code block](0020-task-properties-org-properties-block.md) --
  the block this key lives in, and the warning channel its mistakes use.
- [ADR-0031: Exceptions to a repeating entry, in the iCalendar shape](0031-exceptions-to-a-repeating-entry.md) --
  the other set of keys read out of that block.
- [ADR-0035: A phrase is parsed into an entry by rules, in the core](0035-a-phrase-is-parsed-into-an-entry-by-rules.md)
- [ADR-0037: A phrase also edits an entry that exists](0037-a-phrase-also-edits-an-entry-that-exists.md) --
  the editing a cleared lead time belongs to.
- markdown-org-android, ADR-0034 (reminders are planned on the device) --
  where `REMINDER` was named and deferred.
