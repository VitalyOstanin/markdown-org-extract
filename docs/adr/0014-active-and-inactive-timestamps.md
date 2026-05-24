# ADR-0014: Active and inactive timestamps

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted. Amends [ADR-0002](0002-supported-org-mode-subset.md): partly
removes the out-of-scope bullet "Inactive timestamps in square
brackets `[...]` outside of CLOCK and CLOSED contexts" and replaces
the implicit "all keywords use `<...>`" stance with the per-keyword
table below.

## Context

Emacs Org-mode distinguishes two bracket forms for timestamps:

- **Active** `<2026-05-21 Thu>` -- drives the agenda and scheduling
  machinery.
- **Inactive** `[2026-05-21 Thu]` -- purely descriptive metadata,
  never appears in the agenda.

Until 0.4.x the scanner in this project accepted only `<...>` for
every planning keyword (`SCHEDULED:`, `DEADLINE:`, `CLOSED:`,
`CREATED:`) and for bare inline timestamps. The CLOCK parser already
accepted both forms ([ADR-0003](0003-clock-metadata-support.md)), but
elsewhere the project was stricter than Emacs in one direction
(rejecting inactive forms) and at the same time looser than Emacs in
the opposite direction for CLOSED (Emacs writes `CLOSED:` as inactive
only; the project accepted only the active form, the inverse of
upstream).

The downstream editor `markdown-org-vscode` rendering and editing
flows want the active / inactive distinction so that:

- Files written by Emacs `org-todo` (which closes a task with
  `CLOSED: [...]`) round-trip through the project's parser.
- A user marking a CREATED metadata stamp via `C-c !` in Emacs (which
  writes `[...]`) is read back identically.
- An agenda view never silently picks up a descriptive
  `[2026-05-21 Thu]` from prose as a scheduled item.

The semantics involve a producer / consumer contract change, so this
ADR is required by ADR-0002's own consequence clause:

> Adding a new Org-mode form requires an explicit decision (new ADR
> or amendment) rather than silently accumulating syntax.

The decision was taken after verifying every claim about Emacs Org-mode
behaviour against the upstream Elisp, per
[ADR-0012](0012-verify-org-semantics-against-upstream.md). The exact
upstream sources are quoted in [References](#references) below.

`CREATED:` deserves a separate note. It is **not** a standard Org-mode
keyword: a grep through `lisp/org.el` in the upstream tree returns no
match. The convention comes from the `org-expiry` package in
`org-contrib`, where `org-expiry-insert-created` writes a
`:CREATED: [YYYY-MM-DD Dayname HH:MM]` line **inside a `:PROPERTIES:`
drawer**. This project today recognises `CREATED:` as a top-level
line, which is two steps away from Emacs convention (wrong location +
wrong bracket form). This ADR fixes only the bracket form. Moving
`CREATED:` into a `:PROPERTIES:` drawer is a larger change that
requires its own ADR; it is explicitly out of scope here.

## Decision

### Per-keyword bracket policy

| Context             | Active `<...>` | Inactive `[...]` | Upstream / project source                                               |
| ------------------- | -------------- | ---------------- | ----------------------------------------------------------------------- |
| `SCHEDULED:`        | yes            | no               | `org-scheduled-time-regexp` (`lisp/org.el:563`): `" *<\\([^>]+\\)>"`    |
| `DEADLINE:`         | yes            | no               | `org-deadline-time-regexp` (`lisp/org.el:547`): `" *<\\([^>]+\\)>"`     |
| `CLOSED:`           | no             | yes              | `org-closed-time-regexp` (`lisp/org.el:572`): `" *\\[\\([^]]+\\)\\]"`   |
| `CREATED:`          | no             | yes              | convention from `org-expiry-insert-created` (org-expiry, org-contrib)   |
| Inline plain        | yes            | yes              | `org-ts-regexp` (`lisp/org.el:425`) + `org-ts-regexp-inactive` (`:428`) |
| `CLOCK:`            | yes            | yes              | already accepted per [ADR-0003](0003-clock-metadata-support.md)         |

CLOSED and CREATED change form relative to the pre-ADR-0014 behaviour:
they were accepted only as `<...>`, and from this ADR onward they are
accepted only as `[...]`. This is a **breaking change** for vault
files that store `CLOSED: <...>` or `CREATED: <...>`. The migration
recipe lives in `CHANGELOG.md` for the release that ships this ADR.

### No mixed bracket pairs

A timestamp written as `<2026-05-21 Thu]` or `[2026-05-21 Thu>` is
**rejected**. Justification: Emacs `org-toggle-timestamp-type`
(`lisp/org.el:15510`) uses a strict 1:1 map (`<` ↔ `[`, `>` ↔ `]`) and
never produces a mixed pair. The "fast" matcher
`org-keyword-time-regexp` (`lisp/org.el:576`) technically accepts
`[[<]...[]>]` but only as a compromise for keyword detection; the
canonical timestamp regexes (`org-ts-regexp`,
`org-ts-regexp-inactive`) are pair-strict.

Implementation note: this requires paired alternation in regex (two
alternatives or a backreference), not the lax `[<\[]...[>\]]` form.

### Agenda invariant

Inactive timestamps **never** drive agenda windows. Concretely:

- `build_day_agenda`, `build_week_agenda`, `build_month_agenda`, and
  the "tasks" mode that pulls SCHEDULED/DEADLINE only consider active
  forms.
- `CLOSED:` is always inactive by this ADR but it never fed agenda
  windows anyway; the inactive form just makes the convention
  explicit.
- `CREATED:` is always inactive by this ADR; consumers may display it
  but it does not drive agenda.

This mirrors Emacs: `org-ts-regexp` (active-only) is the regex the
agenda uses; `org-ts-regexp-inactive` is only used by descriptive
searches.

### Round-trip preservation

The bracket form a producer wrote is the form a consumer reads back.
The JSON output exposes a per-timestamp marker so that downstream
editors (`markdown-org-vscode`) can preserve the form on re-write.
The field's exact name (e.g. `active: bool` vs `bracket: "active" |
"inactive"`) is an implementation detail decided when the data model
is extended; this ADR commits only to the marker being present and to
the round-trip guarantee. See
[ADR-0015](0015-json-schema-evolution.md) for the schema-evolution
policy that this addition follows.

### Out of scope

- Moving `CREATED:` into a `:PROPERTIES:` drawer (a separate, larger
  change with its own ADR when undertaken).
- Custom TODO state sequences via `#+TODO:` (still out of scope per
  ADR-0002, no change).
- Date-range timestamps across the active / inactive boundary
  (`<a>--[b]` and similar). Emacs does not produce these.

## Consequences

Easier:

- The parser now matches Emacs behaviour for every keyword that the
  project claims to support. `org-todo`-closed entries and
  `C-c !`-stamped CREATED lines from Emacs round-trip without
  translation.
- The agenda invariant is mechanical: an inactive timestamp is by
  definition not eligible. No risk of descriptive `[YYYY-MM-DD ...]`
  in prose drifting into the agenda.
- Mixed-pair rejection means the regex set has a single
  canonical-form check; ambiguous input fails fast at parse time
  rather than at agenda time.

Harder:

- Vault files that the project produced or accepted previously with
  `CLOSED: <...>` or `CREATED: <...>` need a one-time rewrite. The
  recipe in CHANGELOG covers both keywords with `sed`. Users who
  edited their files by hand and used the old form must run the
  migration before upgrading.
- Downstream consumers that hard-coded `<...>` regex for CLOSED /
  CREATED need to read the new JSON marker (or the timestamp string
  itself) and accept `[...]`. This is the
  `x-markdown-org.extractorVersion` coordination point described in
  [ADR-0015](0015-json-schema-evolution.md).
- The regex set grows: every previously single-form regex now has
  either a paired alternation or a backreference. Maintenance cost is
  modest and bounded.

## References

- Upstream Emacs Org-mode regex constants (verified against the local
  checkout per ADR-0012):
  - `org-ts-regexp` -- `lisp/org.el:425`
  - `org-ts-regexp-inactive` -- `lisp/org.el:428`
  - `org-ts-regexp-both` -- `lisp/org.el:432`
  - `org-deadline-time-regexp` -- `lisp/org.el:547`
  - `org-scheduled-time-regexp` -- `lisp/org.el:563`
  - `org-closed-time-regexp` -- `lisp/org.el:572`
  - `org-keyword-time-regexp` -- `lisp/org.el:576`
  - `org-toggle-timestamp-type` -- `lisp/org.el:15510`
- `org-expiry` package and `org-expiry-insert-created`:
  [Worg: org-expiry](https://orgmode.org/worg/org-contrib/org-expiry.html)
- Inactive timestamp documentation:
  [Timestamps (The Org Manual)](https://orgmode.org/manual/Timestamps.html),
  [Creating Timestamps (The Org Manual)](https://orgmode.org/manual/Creating-Timestamps.html)
- Project code affected:
  [`src/timestamp/extract.rs`](../../src/timestamp/extract.rs),
  [`src/timestamp/parser.rs`](../../src/timestamp/parser.rs),
  [`src/timestamp.rs`](../../src/timestamp.rs),
  [`src/clock.rs`](../../src/clock.rs),
  agenda builders in [`src/agenda.rs`](../../src/agenda.rs).
- Related ADRs: [ADR-0002](0002-supported-org-mode-subset.md) (amended
  by this ADR), [ADR-0003](0003-clock-metadata-support.md) (CLOCK
  bracket policy was the precedent for accepting both forms),
  [ADR-0012](0012-verify-org-semantics-against-upstream.md)
  (procedure used to verify the upstream regex citations above),
  [ADR-0015](0015-json-schema-evolution.md) (how the new JSON marker
  is communicated to consumers).
- Consumer-side coordination:
  [`markdown-org-vscode/TODO.md`](https://github.com/VitalyOstanin/markdown-org-vscode/blob/master/TODO.md)
  (square-bracket section, to be added in the same release cycle).
