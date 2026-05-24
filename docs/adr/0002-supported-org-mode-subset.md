# ADR-0002: Supported subset of org-mode keywords

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted. Amended by [ADR-0014](0014-active-and-inactive-timestamps.md)
on 2026-05-24: the per-keyword bracket policy below is superseded by
the table in ADR-0014, and the out-of-scope bullet on inactive
timestamps is partly removed.

## Context

Emacs Org-mode has an extensive vocabulary: many timestamp forms,
arbitrary user-configurable TODO state sequences, per-file `#+TODO:`
directives, properties drawers, tags inheritance, agenda categories,
clock tables, repeaters, warning cookies, habit graphs, and more. A
faithful re-implementation is out of scope for a markdown-first tool
that runs as a one-shot CLI.

At the same time, parallel ecosystems (Obsidian Tasks with emoji
markers `📅 ⏫ 🔁`, Dataview inline fields `key:: value`) describe
similar concepts in incompatible syntax. Supporting all of them at
once would multiply parser complexity and force users into one of
them.

The project's primary audience writes notes in markdown but wants
their files to round-trip through Emacs Org-mode. That fixes the
direction: pick the Org-mode wire format, support the subset of it
that actually drives a useful agenda, and document the exclusions
explicitly.

## Decision

The scanner recognises the following subset of Emacs Org-mode syntax:

**Timestamps** (active, in angle brackets `<...>`):

- Plain: `<2026-05-21 Thu>` and `<2026-05-21 Thu 09:39>`.
- Time range on a single day: `<2026-05-21 Thu 10:00-12:00>`.
- Planning keywords: `SCHEDULED:`, `DEADLINE:`, `CLOSED:`, `CREATED:`.
- Repeaters: `+1d`, `+2w`, `+1m`, `+1y`, `+1h`,
  `++Nu` (catch-up), `.+Nu` (restart-on-DONE),
  `+1wd` (next weekday).
- Warning-period cookies on DEADLINE timestamps:
  `-N<unit>` with unit one of `h/d/w/m/y`. The cookie overrides the
  global 14-day default warning window for that one task and is
  converted to whole days using upstream `org-get-wdays`'s factors
  (`d=1`, `w=7`, `m=30.4`, `y=365.25`, `h=1/24`, floored). The cookie
  may appear in either order relative to the repeater
  (`<... +1y -3d>` and `<... -3d +1y>` are both recognised).
- Weekday names accept English (Mon..Sun) and Russian short and full
  forms; they are normalised to English before storage.

**Headings**:

- TODO/DONE prefix at the start of a heading (`### TODO ...`,
  `### DONE ...`).
- Optional priority cookie `[#A]`, `[#B]`, `[#C]` etc. immediately
  after the keyword.
- Headings without TODO/DONE are still extracted when they carry a
  `CREATED:` or other timestamp line.

**CLOCK** entries: documented separately in
[ADR-0003](0003-clock-metadata-support.md).

**Wire wrapping**: timestamp and CLOCK lines are written by
producers wrapped in markdown inline code (backticks), so the same
file renders cleanly in markdown viewers and still parses in Emacs.
The scanner accepts both wrapped and unwrapped forms for backward
compatibility. See the consumer-side
[ADR-0003 in markdown-org-vscode](https://github.com/VitalyOstanin/markdown-org-vscode/blob/master/docs/adr/0003-org-mode-wire-format.md)
for the producer view.

The following Org-mode features are deliberately **out of scope**:

- Custom TODO state sequences via `#+TODO:` directives. Only
  hard-coded `TODO` and `DONE` are recognised.
- ~~Inactive timestamps in square brackets `[...]` outside of CLOCK
  and CLOSED contexts.~~ Superseded by
  [ADR-0014](0014-active-and-inactive-timestamps.md) (2026-05-24):
  inactive `[...]` is now accepted for `CLOSED:`, `CREATED:`, and
  inline plain timestamps; rejected for `SCHEDULED:` and `DEADLINE:`;
  CLOCK behaviour is unchanged.
- Multi-day agenda display of date-range timestamps
  `<...>--?-?<...>`. The dash separator is accepted in all three
  variants Emacs allows (one, two, or three dashes, matching
  `org-tr-regexp`), and the start date and start / end times are
  surfaced; the end **date** is not exposed and the task is shown
  only on the start day. Multi-day spanning is a separate concern.
- Properties drawers, tag inheritance, agenda categories, habit
  graphs.

The following non-Org formats are **not** parsed:

- Obsidian Tasks plugin emoji markers (`📅`, `⏫`, `🔁`, etc.).
- Obsidian Dataview inline fields (`key:: value`).
- Markdown task checkboxes (`- [ ]`, `- [x]`) -- the scanner looks
  at headings, not list items.

## Consequences

Easier:

- The parser stays small and the regex set is bounded by an explicit
  list; users can read [`src/timestamp/extract.rs`](../../src/timestamp/extract.rs)
  and understand exactly what is recognised.
- Files written by this project's documented producers (e.g. the VS
  Code extension) round-trip through Emacs without translation.
- Adding a new Org-mode form requires an explicit decision (new ADR
  or amendment) rather than silently accumulating syntax.

Harder:

- Users coming from Obsidian Tasks or Dataview must either change
  their workflow or write a converter. The README points at the
  Org-mode-style syntax and at this ADR for the rationale.
- Bug reports of the form "feature X of Org-mode doesn't work" need
  triage against this ADR's list before being treated as bugs.

## References

- Timestamp extraction: [`src/timestamp/extract.rs`](../../src/timestamp/extract.rs)
- Repeater parsing and occurrence math: [`src/timestamp/repeater.rs`](../../src/timestamp/repeater.rs)
- Weekday normalisation: [`src/timestamp/weekdays.rs`](../../src/timestamp/weekdays.rs)
- Heading parsing: [`src/parser.rs`](../../src/parser.rs)
- Upstream Elisp source: [emacs/org-mode on Savannah](https://git.savannah.gnu.org/cgit/emacs/org-mode.git)
  (see also [ADR-0012](0012-verify-org-semantics-against-upstream.md)
  for the policy on consulting it before changing semantics).
- Public Org-mode syntax reference: [orgmode.org/worg/dev/org-syntax.html](https://orgmode.org/worg/dev/org-syntax.html)
