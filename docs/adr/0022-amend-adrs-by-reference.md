# ADR-0022: Amend ADRs by reference, not by rewriting

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-05-29). Formalises the immutability convention already
stated in [`docs/adr/README.md`](README.md) and extends it explicitly to
cover *amendments*, not only full supersession.

## Context

[`docs/adr/README.md`](README.md) already states that ADRs are immutable
once they leave `Status: Proposed`, and that changing a decision is done
by writing a new ADR that supersedes the old one with cross-referenced
`Status` fields. That convention covers full supersession but is silent
on the narrower, more common case: an existing decision is left in force
while a new decision amends or extends it.

History on this point is mixed. [ADR-0014](0014-active-and-inactive-timestamps.md)
and [ADR-0020](0020-task-properties-org-properties-block.md) amended their
predecessor ([ADR-0002](0002-supported-org-mode-subset.md)) the intended
way: the new decision lives in its own ADR, and the predecessor carries
only a `Status` pointer to it. By contrast, the 0.8.0 release
(2026-05-29) recorded a new `CANCELLED` decision by rewriting body prose
inside [ADR-0002](0002-supported-org-mode-subset.md) and
[ADR-0015](0015-json-schema-evolution.md) in place.

In-place content edits erode the property that makes an ADR useful: a
reliable record of what was decided and when. Once the body of an
accepted ADR is rewritten, the document no longer attests to its own
original decision, and the date of the new decision is lost unless it is
encoded separately.

## Decision

- A new architectural decision is recorded in a **new** ADR.
- An **existing** ADR is touched only by a short `Status` note of the
  form `Amended by ADR-NNNN (YYYY-MM-DD): <one line>`. Its `Context`,
  `Decision`, and `Consequences` prose is **never** rewritten to carry
  the new decision.
- The current state of a living catalog -- for example the
  supported-keyword enumeration in
  [ADR-0002](0002-supported-org-mode-subset.md) -- is read as the ADR
  plus its amendment chain, by following the `Status` pointers.

## Consequences

Easier:

- The record stays trustworthy over time: each ADR continues to attest to
  the decision it originally captured and the date it was made, and every
  later change is timestamped on its own amending ADR.

Harder:

- To read the current state of a catalog ADR you must follow its
  amendment chain rather than read a single self-contained body. This is
  an accepted trade-off.

Enforcement:

- Reviewers apply this policy. A reviewer finding that an ADR's body was
  rewritten to add a new decision closes the finding by pointing here.

## References

- Conventions: [`docs/adr/README.md`](README.md) (immutability and the
  `Status`-pointer convention this ADR formalises).
- Amendment-by-reference precedents:
  [ADR-0014](0014-active-and-inactive-timestamps.md),
  [ADR-0020](0020-task-properties-org-properties-block.md).
- Catalog ADR read via its amendment chain:
  [ADR-0002](0002-supported-org-mode-subset.md).
