# ADR-0036: A later phrase refines the entry, it does not replace it

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-31), amended by
[ADR-0037](0037-a-phrase-also-edits-an-entry-that-exists.md). Amends
[ADR-0035](0035-a-phrase-is-parsed-into-an-entry-by-rules.md) in the shape of
the parser, not in what the rules do.

## Context

ADR-0035 has the parser take a phrase and return the fields it
recognised. One phrase is rarely right the first time. A dictated one is
worse: the recogniser hears "в пятнадцать" as "в пятьдесят", and the
repeater is the thing a person remembers a moment after saying the rest.

If every phrase starts from an empty entry, a correction means saying the
whole sentence again. Typed, that is an annoyance. Spoken, it is the
difference between a feature and a demonstration: nobody re-dictates
"call the doctor tomorrow at three every week" to move the day to Friday.

The creation screen already holds the nine fields, so the state a second
phrase would refine exists whichever way this is decided. The question is
only whether the rules can see it.

## Decision

The parser takes the entry parsed so far and returns it refined.

A field named by the new phrase replaces what was there. A field the new
phrase does not name keeps its value. Text the rules do not consume is
appended to the heading, separated by a space; on the first phrase, where
the heading is empty, this is exactly the rule ADR-0035 already states.

The merge lives in the core with the rules, not in the clients. Both
clients then refine the same way, and one table of examples proves it for
both.

## Consequences

The signature carries what is already known, so the parser is a fold over
phrases rather than a function of one. The empty entry is not a special
case: the first phrase is the fold's first step.

The table of examples becomes a table of chains — a sequence of phrases
and the entry they leave behind — which is a stricter test than the same
phrases checked apart, because it pins down what the second phrase leaves
alone as well as what it changes.

Nothing in the grammar removes a field. "Не завтра" sets no date and
clears none; a field is emptied on the screen, where every field can be
emptied already. The rules stay a list of things that are recognised, and
a phrase that removes would need the opposite kind of rule.

Unrecognised text lands in the heading, so a refinement phrased poorly
("в проект работа", when no collection is named that) leaves a visible
trace in a field the person is looking at, rather than being dropped in
silence. Losing a phrase quietly is the worse failure: the screen would
show a correct entry and the person would never learn that half of what
they said was gone.

## References

- [ADR-0035](0035-a-phrase-is-parsed-into-an-entry-by-rules.md) — the
  rules themselves, and why they live in the core.
- [ADR-0004](0004-tdd-mandatory.md) — the chains are written before the
  grammar that satisfies them.
