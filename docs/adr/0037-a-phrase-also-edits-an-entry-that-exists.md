# ADR-0037: A phrase also edits an entry that exists

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-09-02). Amends
[ADR-0036](0036-a-later-phrase-refines-the-entry.md) in one of its
consequences: the grammar now removes a field when it is told to.

## Context

ADR-0035 and ADR-0036 give the rules that turn a phrase into the fields of
a new entry. The entries a person has already written are edited from the
other end: the clients offer a command or a button for each field — set
the keyword, set the priority, insert a planning line, shift a date.

One change is one button, which is short enough. Three changes are three
buttons and two dialogs of choice, and the sentence that names all three
("перенеси на пятницу в 16:00 и сделай срочной") already parses into two
of the fields it names. What it cannot express is the third:

1. the keyword is not among the parsed fields at all, so "отметь
   выполненной" is unsayable;
2. emptying a field has no phrasing, and negation is not one — ADR-0036
   states outright that "не завтра" clears nothing;
3. the verbs of editing — `перенеси`, `сделай`, `отметь`, `move`, `mark` —
   are not in the closed list of lead-ins, so they land in the heading.

The third is what makes the first two visible: a phrase said about an
entry that exists starts with a verb, and the parser reads that verb as
part of a heading it is not being asked for.

`PhraseEntry::date` being `None` means "the phrase said nothing about the
date". An edit needs a third answer — "the phrase said the date is gone" —
which `Option` has no room for.

## Decision

The grammar of the core covers editing as well as creating, in one set of
rules and one parsed entry.

Three things are added. A keyword (`PhraseKeyword`: `TODO`, `DONE`,
`CANCELLED`) — not `TaskType`, whose cancelled variant carries the
spelling found in the file, which a phrase does not say. A set of emptied
fields (`ClearedFields`) beside the values, which is the third answer
`Option` cannot give. And the verbs of editing, in a list of their own
next to the verbs of creating.

Both additions to `PhraseEntry` are additive: a new field on a
`#[non_exhaustive]` struct and a new key in the JSON, which per ADR-0015
a consumer that does not know it ignores. The types of the fields that
exist do not change — the Android bridge and the extension read them.

A conjunction may restart a verb of editing in the middle of a phrase, so
one sentence carries two instructions; a verb of creating is not restarted
that way, because "позвонить и напомни про отчёт" is one entry.

Which of the two things a phrase did is not decided here. The core answers
with fields; whether they are written into a new entry or applied to one
that exists is the caller's reading of the answer.

## Consequences

Emptying is said outright and is told apart from silence: "убрать дату"
answers with `date: null` and `cleared: ["date"]`, a phrase that says
nothing about the date answers with `date: null` and `cleared: []`.
Negation is unchanged and still removes nothing — the rules recognise
removal, they do not infer it.

Naming a field and emptying it are the same rule read twice, so each
undoes the other and a chain says what its last phrase said. This keeps
ADR-0036's fold intact: the step is still "refine what is known".

The list of lead-ins grows, and with it the cost ADR-0035 accepted: an
entry that really begins with "сделай" or "mark" loses that word. The cost
is unchanged in kind, and is corrected on the screen the fields are shown
on.

A leftover in the heading means something in the phrase was not
understood. For a new entry that is harmless — the words become part of
the heading. For an edit there is no heading to put them in, so a client
that is editing refuses the phrase over the leftover rather than applying
half of what was said. That reading is the client's; the core only reports
what it did not consume.

The table of examples grows a second half, for the phrases that edit. It
states two more columns and it checks the invariant that keeps the two
uses apart: a phrase that creates an entry names no keyword and empties
nothing.

## References

- [ADR-0035](0035-a-phrase-is-parsed-into-an-entry-by-rules.md) — the
  rules themselves, and why they live in the core.
- [ADR-0036](0036-a-later-phrase-refines-the-entry.md) — the fold this
  extends, and the consequence this amends.
- [ADR-0021](0021-accept-canceled-spelling.md) — why the cancelled
  spelling is a property of the file rather than of the phrase.
- [ADR-0015](0015-json-schema-evolution.md) — why a new key
  in the JSON is an additive change.
