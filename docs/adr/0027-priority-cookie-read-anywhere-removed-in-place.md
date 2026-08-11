# ADR-0027: A priority cookie is read wherever it is written, and removed only where org puts it

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-11). Amends
[ADR-0002](0002-supported-org-mode-subset.md), whose heading bullet
described the cookie as recognised "immediately after the keyword".

## Context

Emacs answers "where may a priority cookie sit?" in two places, and they
do not agree.

`org-complex-heading-regexp` ([`lisp/org.el`][org-el], the pattern built
around line 4591) allows the cookie only directly after the stars or the
TODO keyword: `\\(?: +\\(\\[#.\\]\\)\\)?`. `org-priority-regexp`
(line 11308) is the other answer — `".*?\\(\\[#\\(%s\\)\\] ?\\)"` — and
its `.*?` prefix lets the cookie appear anywhere on the line.

Which one a reader meets depends on the function. Running the reference
implementation (local checkout of [emacs/org-mode][org-mode-repo],
commit `72db4de02`) on `* TODO Buy [#A] filter` gives:

| Read through                           | Priority | Title                     |
| -------------------------------------- | -------- | ------------------------- |
| `org-element` `:priority` / `:raw-value` | `A`      | `filter`                  |
| `org-heading-components`               | none     | `Buy [#A] filter`         |
| `org-get-priority`                     | `A` (2000) | —                       |
| the line `org-agenda-list` prints      | sorts as `A` | `TODO Buy [#A] filter` |

So what a user sees is: the cookie counts wherever it is written — an
entry with a trailing `[#A]` is sorted above one with a leading `[#C]` —
and the title is shown as typed, cookie included. The prefix-dropping
lives only in `org-element`'s `:raw-value`, which the agenda does not
display.

This project had followed `:raw-value`: `parse_heading` searched for the
cookie anywhere and returned everything after it as the heading. On
`# TODO Title with trailing cookie [#A]` that leaves the heading empty
and the agenda showing a row with no text — the whole title deleted from
view because of where the user put four characters. The VS Code
extension had gone the other way, matching the cookie only in the
canonical place, so the two clients disagreed about both the priority
and the title of the same heading.

## Decision

Split the two questions that were answered by one rule.

1. **Reading the value.** The cookie is recognised at any position in
   the heading text, after an optional keyword, exactly as
   `org-priority-regexp` does. `# TODO Buy [#A] filter` has priority
   `A`, and so does `# Title with trailing cookie [#A]`.
2. **Removing it from the title.** The cookie is taken out of the
   heading **only when it opens the heading text** — directly after the
   keyword, or at the start when there is no keyword. Anywhere else it
   stays in the title, together with everything before it.

The canonical position is where a client writes a cookie it adds, so a
heading this project produced round-trips with the cookie stripped and
rendered as the client's own priority marker, as before. A cookie the
user typed elsewhere is data, and the agenda shows the line as written.

`parse_heading_line`, the editing half, follows the same split:
`priority.range` addresses the cookie where it actually is, so a client
replacing the value edits it in place instead of rebuilding the line and
moving the cookie to the front; `title_start` stops before a
non-canonical cookie, so rewriting the title neither swallows nor
duplicates it.

This is a deliberate divergence from `org-element` `:raw-value`, recorded
here under [ADR-0012](0012-verify-org-semantics-against-upstream.md) §3.
It is not a divergence from what Org-mode shows: the agenda line and the
sort order match upstream in every case above.

## Consequences

Easier:

- A heading never loses text to the parser. The failure that motivated
  this — a trailing cookie emptying the row — cannot happen.
- The two clients can agree, because the rule no longer forces a choice
  between "read the priority" and "keep the text": both are satisfied.
- An editor gains a usable cookie range: replacing `[#B]` with `[#A]` in
  a heading that carries the cookie mid-line is a slice, not a rewrite.

Harder:

- `:raw-value` from `org-element` and `Task::heading` from this parser
  differ for a non-canonical cookie. Anyone comparing the two has to
  know which one they hold; the table above is the reference.
- A heading whose prose happens to contain `[#A]` — quoting a cookie
  rather than setting one — gains a priority it did not intend. Upstream
  behaves the same way (`org-get-priority` finds it), so the surprise is
  shared with Org-mode rather than invented here.
- The rule now has two halves, and a client that implements only the
  first (read anywhere) drops back to the deleted-title behaviour. The
  test named in References below pins both halves.

## References

- Implementation: [`src/parser.rs`](../../src/parser.rs)
  (`HEADING_PRIORITY_RE`, `parse_heading`, `parse_heading_line`).
- Tests: `src/parser.rs`
  (`parse_heading_priority_in_the_middle_org_semantics`,
  `parse_heading_trailing_cookie_leaves_a_title_behind`,
  `parse_heading_todo_then_priority_with_intervening_text`,
  `extract_tasks_priority_in_middle_keeps_the_prefix`),
  `tests/lib_api.rs`
  (`heading_line_keeps_the_text_between_the_keyword_and_the_cookie_addressable`).
- Related ADRs:
  [ADR-0002](0002-supported-org-mode-subset.md) (the subset this amends),
  [ADR-0012](0012-verify-org-semantics-against-upstream.md) (the rule
  requiring the upstream check above and the record of the divergence),
  [ADR-0022](0022-amend-adrs-by-reference.md) (why this is a new record
  rather than an edit to ADR-0002).
- Upstream: [emacs/org-mode][org-mode-repo], `lisp/org.el`
  (`org-priority-regexp`, `org-complex-heading-regexp`),
  `lisp/org-element.el` (`org-element--headline-parse-title`).

[org-mode-repo]: https://git.savannah.gnu.org/cgit/emacs/org-mode.git
[org-el]: https://git.savannah.gnu.org/cgit/emacs/org-mode.git/tree/lisp/org.el
