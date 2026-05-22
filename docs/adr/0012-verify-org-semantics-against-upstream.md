# ADR-0012: Verify Org-mode semantics against upstream Elisp

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

The project parses an Org-mode subset (see
[ADR-0002](0002-supported-org-mode-subset.md)) embedded inside
Markdown files. Org-mode is not a tidy spec document — its
authoritative definition is the reference implementation in Emacs
Lisp shipped with [Emacs Org-mode][org-mode-repo]. Edge cases of
the semantics that the project must mirror exactly (timestamp
syntax, repeater normalisation, agenda windowing, TODO-state
keywords, scheduled / deadline / closed distinctions, repeater
"next due" arithmetic, etc.) are defined by what that Elisp
actually does, not by what blog posts, the manual prose, or any
language model's training data say.

General knowledge of Org-mode is unreliable in the small. The
manual sometimes describes a feature in one place and the parser
implements a variant in another. A previous instance of this
already happened in this project: the DEADLINE `warning-period`
(`-Nd`) modifier appeared in the parser's regex and in agenda
examples for some time without a working implementation, because
the agent that wrote it had reconstructed the syntax from memory
rather than reading
[`lisp/org-element.el`](https://git.savannah.gnu.org/cgit/emacs/org-mode.git/tree/lisp/org-element.el)
and
[`lisp/org-agenda.el`](https://git.savannah.gnu.org/cgit/emacs/org-mode.git/tree/lisp/org-agenda.el).
That is the failure mode this ADR is meant to prevent.

## Decision

For any change that touches Org-mode semantics — parsing,
normalisation, agenda assembly, repeater evaluation, TODO-state
handling, timestamp arithmetic — the agent must verify behaviour
against the upstream Emacs Org-mode Elisp source **before** writing
the test or the code, not by analogy or recall.

Specifically:

1. Read the relevant Elisp from the upstream repository (canonical
   mirror: <https://git.savannah.gnu.org/cgit/emacs/org-mode.git>).
   Key entry points: `lisp/org.el` (core),
   `lisp/org-element.el` (parser AST),
   `lisp/org-element-ast.el` (AST utilities),
   `lisp/org-agenda.el` (agenda construction and windowing),
   `testing/lisp/test-org-element.el` and friends (behavioural
   tests that pin observed semantics).
2. Cross-check the specific behaviour to be implemented against
   the upstream tests (`testing/lisp/`). If the upstream has a
   test for the case at hand, the project's test should match the
   same input / output relationship.
3. If the project deliberately diverges from upstream — for example
   because Markdown context changes what is meaningful — record the
   divergence in [ADR-0002](0002-supported-org-mode-subset.md) (or
   in a new ADR that supersedes it) before shipping the divergent
   behaviour. Silent drift from upstream is a bug.
4. Reviewers (the `logic-reviewer`, `code-reviewer`, `tests-reviewer`
   sub-agents and any human reviewer) should be told where the
   upstream source lives so they can perform the same cross-check
   on the patch. The exact local path lives in the agent's
   reference memory; it is not committed to this repository.
5. Documentation-only changes that do not alter parser or agenda
   behaviour are exempt.

## Consequences

Easier:

- Bugs of the "the parser accepts something the manual mentions
  but the upstream parser never produces" kind are caught at
  test-design time rather than at user-report time.
- The project has a single, durable source of truth for "what does
  Org-mode mean here?", independent of any model's training cutoff
  or any contributor's memory.
- Intentional divergences become visible: writing them down as
  ADRs forces a deliberate decision instead of an accidental one.

Harder:

- Every Org-semantics change costs a read of Elisp. For a small
  fix this can dwarf the code change itself. The trade-off is
  accepted because the alternative is the
  warning-period-regex-without-implementation class of bug.
- The discipline is not enforced by tooling. It is enforced by
  the agent and by review, the same way [TDD](0004-tdd-mandatory.md)
  is.

## References

- Upstream repository:
  [emacs/org-mode][org-mode-repo].
- Supported subset and explicit non-goals:
  [ADR-0002](0002-supported-org-mode-subset.md).
- TDD discipline that this ADR mirrors in structure:
  [ADR-0004](0004-tdd-mandatory.md).

[org-mode-repo]: https://git.savannah.gnu.org/cgit/emacs/org-mode.git
