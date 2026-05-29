# ADR-0021: Accept CANCELED spelling; preserve original task_type

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-05-29, 0.9.0). Amends
[ADR-0002](0002-supported-org-mode-subset.md) (supported subset) and
[ADR-0015](0015-json-schema-evolution.md) (schema evolution).

## Context

Release 0.8.0 (2026-05-29) added a cancelled TODO keyword to the
scanner, recognised in the same heading position as `TODO` / `DONE`,
and a matching `CANCELLED` value for the `task_type` field in the JSON
output. It shipped the double-L spelling `CANCELLED`.

Upstream Emacs Org-mode uses the single-L spelling `CANCELED`. Its
manual (`doc/org-manual.org`) and quick guide (`doc/org-guide.org`)
spell the keyword `CANCELED`, for example in the sequence
`(sequence "TODO(t)" "WAIT(w@/!)" "|" "DONE(d!)" "CANCELED(c@)")`.
Neither spelling is built in: upstream's out-of-the-box
`org-todo-keywords` is only `(sequence "TODO" "DONE")`. Both
`CANCELLED` and `CANCELED` are user conventions, and both occur in
real files.

This leaves two open questions. First, which spelling(s) the scanner
should recognise. Second — once both are recognised — what the
`task_type` value should be for a cancelled task: a single
canonicalised form, or whatever spelling the source file used.

The second question matters because the public consumer
[`markdown-org-vscode`](https://github.com/VitalyOstanin/markdown-org-vscode)
performs reverse sync: it writes task state back to the source file.
If the extractor normalised the spelling, a round trip through the
consumer could silently rewrite a user's chosen spelling (turning
`### CANCELED x` into `### CANCELLED x`, or vice versa) merely because
the task passed through the tooling. Round-trip fidelity requires the
extractor to report what it actually saw.

## Decision

1. The scanner recognises **both** `CANCELLED` (double-L) and
   `CANCELED` (single-L) as the cancelled TODO keyword. Both are
   accepted in the same heading position as `TODO` / `DONE`,
   case-sensitive, and must be followed by at least one whitespace
   character.

2. The `task_type` value in the JSON output **preserves the original
   spelling** from the source file. It is not normalised to one form:
   `### CANCELED x` yields `"task_type":"CANCELED"`, and
   `### CANCELLED x` yields `"task_type":"CANCELLED"`.

3. Rationale for preserving the spelling: the public consumer
   `markdown-org-vscode` writes task state back to the source file.
   Reverse-sync consumers must not silently rewrite the user's chosen
   spelling, so the extractor reports the spelling it observed rather
   than a canonical form.

4. Internally the cancelled state carries the observed spelling — for
   reference, a `TaskType::Cancelled(CancelledSpelling)` shape with an
   `enum CancelledSpelling { DoubleL, SingleL }` — so the original form
   can be reproduced on output.

5. This is justified under
   [ADR-0012](0012-verify-org-semantics-against-upstream.md): Org-mode
   TODO keywords are user-defined via `#+TODO:`, so upstream's example
   spelling (single-L `CANCELED` in the manual) is not binding on this
   project's hard-coded keyword set. Supporting both spellings and
   preserving the one the user wrote is the user-respecting choice.

6. This is a **non-breaking** change under
   [ADR-0015](0015-json-schema-evolution.md): it extends the value set
   of an enum-like string field. Consumers must match both spellings;
   the consumer's `normalizeTaskType` already handles this. Introduced
   in 0.9.0.

## Consequences

Easier:

- Files using the upstream-manual single-L spelling `CANCELED` are now
  recognised, not just the 0.8.0 double-L `CANCELLED`.
- Reverse-sync consumers round-trip a cancelled heading without
  rewriting the user's chosen spelling, because the extractor reports
  the original form.

Harder:

- The cancelled `task_type` is no longer a single fixed string;
  consumers must match both `CANCELLED` and `CANCELED`. The current
  consumer already does so via `normalizeTaskType`.
- Internal handling of the cancelled state must carry the observed
  spelling rather than collapsing to one canonical value.

## References

- Amended: [ADR-0002](0002-supported-org-mode-subset.md) (supported
  org-mode keyword subset; the cancelled keyword set).
- Amended: [ADR-0015](0015-json-schema-evolution.md) (non-breaking
  enum-like value-set extension; the cancelled `task_type` value).
- Upstream-semantics policy:
  [ADR-0012](0012-verify-org-semantics-against-upstream.md) (TODO
  keywords are user-defined, so upstream's example spelling is not
  binding).
- Amendment-by-reference policy:
  [ADR-0022](0022-amend-adrs-by-reference.md) (this ADR carries the new
  decision; ADR-0002 / ADR-0015 receive only a `Status` pointer).
- Upstream Emacs Org-mode manual and quick guide
  (`doc/org-manual.org`, `doc/org-guide.org`) spell the keyword
  `CANCELED`; the project source is
  [emacs/org-mode on Savannah](https://git.savannah.gnu.org/cgit/emacs/org-mode.git).
- Consumer that performs reverse sync:
  [`markdown-org-vscode`](https://github.com/VitalyOstanin/markdown-org-vscode)
  (`normalizeTaskType` matches both spellings).
