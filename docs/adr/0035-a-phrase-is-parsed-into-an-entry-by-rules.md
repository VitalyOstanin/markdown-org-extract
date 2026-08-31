# ADR-0035: A phrase is parsed into an entry by rules, in the core

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-31). Amended by
[ADR-0036](0036-a-later-phrase-refines-the-entry.md) in the shape of the
parser: it takes the entry parsed so far and refines it.

## Context

Creating an entry means answering nine questions — the heading, the body,
the keyword, the priority, which kind of date it is, the date, the hour,
the repeater, and which collection receives it. In the Android client
each is a separate control, so a task due tomorrow at three carries four
taps through a calendar and a clock before it is written. One typed
sentence — "call the doctor tomorrow at 15:00, every week" — holds all of
it, and so does one spoken sentence.

Three ways to turn that sentence into the nine fields were weighed.

A language model on the device understands any phrasing and needs no
network. It also needs the model: Gemma 3 1B in int4 is around 529 MB of
file and some 720 MB of memory while it answers, against an APK of
14.4 MB. Nothing about that fits an application whose whole point is that
notes are plain files.

A model behind an interface is the cheapest to write — one request, one
JSON answer — and the most expensive to own. The phrase leaves the
device, a key has to be held somewhere, and without a network there is no
feature at all. F-Droid's "Non-Free Network Services" has an exemption
for opt-in only in the "Tracking" anti-feature, so shipping the interface
at all is what earns the label, not enabling it.

Rules understand a fixed list of phrasings and nothing else. In exchange
they cost nothing to run, work with the radio off, leave the phrase where
it was typed, and answer the same way every time — which is the only one
of the three a test can pin down. Nothing off the shelf helps with
Russian: `interim`, `chrono-english`, `two_timer` and `date_time_parser`
parse English only.

Where the rules live decides who gets them. The core already holds the
grammar of a timestamp (`timestamp/parser.rs`) and the Russian weekday
table (`locale.rs`) that `--locale ru` reads, and it is already what both
clients call — the extension as a subprocess, the Android client through
UniFFI.

## Decision

A phrase is parsed by rules, and the rules live in the core beside the
grammar they extend.

The parser takes the phrase, the locale, and the day the phrase is
relative to; it returns the fields it recognised. The reference day is a
parameter for the same reason `--current-date` is one: "tomorrow" is
meaningless without saying tomorrow from when, and the core is not the
place that decides what today is.

What the rules do not consume becomes the heading. A phrase that names no
date yields no date rather than a guessed one.

The parser only reads. It writes nothing, opens no file, and creates no
entry: the caller receives the fields and does with them what it will.
The Android client fills the creation screen, so what was understood is
visible and correctable before anything reaches a file, and every check
the writing path already makes still runs.

A model behind an interface is not part of this decision. If the rules
turn out to miss phrasings worth having, that is a later decision with
its own record, and it starts from evidence about which phrasings were
missed.

## Consequences

The grammar is ours to carry, in both languages. Russian needs the cases
a weekday takes ("во вторник", "к пятнице") and the ways an hour is said;
English needs its own list. Each addition is a row in a table of examples,
which is also the test.

The extension and the CLI get the parser for free — it is a function of
the library and a subcommand of the binary, not something the Android
client keeps to itself. The cost is the ordinary one: the Android client
sees it after the core is released and the pin is moved.

A phrase outside the list stays unparsed, and the boundary is not visible
to the person typing. Two things soften it and neither removes it: the
unconsumed text still becomes a heading, so nothing is lost, and the
fields are shown for correction rather than written.

Nothing leaves the device, no key is held, no network is touched, and no
anti-feature is earned.

## References

- [ADR-0009](0009-unified-date-window-semantics.md) — what `--current-date`
  is for, and why the notion of today is given rather than taken.
- [ADR-0002](0002-supported-org-mode-subset.md) — the keywords a parsed
  phrase may name.
- [ADR-0004](0004-tdd-mandatory.md) — the table of examples is written
  before the grammar that satisfies it.
