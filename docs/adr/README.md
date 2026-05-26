# Architecture Decision Records

This directory holds the project's Architecture Decision Records (ADRs),
following the format proposed by Michael Nygard. Each ADR captures a
single architectural decision: the context that forced the choice, what
was decided, and the trade-offs that came with it.

## Table of Contents

- [Conventions](#conventions)
- [Index](#index)
- [Adding a new ADR](#adding-a-new-adr)

## Conventions

- Files are named `NNNN-kebab-case-title.md` with a four-digit
  zero-padded sequence number.
- ADRs are **immutable** once they leave `Status: Proposed`. To change
  a decision, write a new ADR that supersedes the old one and update
  both files' `Status` fields with cross-references.
- Each ADR has the sections `Status`, `Context`, `Decision`,
  `Consequences`, and (optional) `References`. Keep the body short --
  one to two screens is the target.
- The index below mirrors the directory; keep it in sync when a new
  ADR is added or an existing ADR changes status.
- ADRs are written in English regardless of the rest of the project's
  documentation language, so they remain readable for external
  contributors and tooling.

## Index

| #    | Title                                                                                  | Status   |
| ---- | -------------------------------------------------------------------------------------- | -------- |
| 0001 | [Standalone CLI for org-mode in markdown](0001-standalone-cli-for-org-in-markdown.md)  | Accepted |
| 0002 | [Supported subset of org-mode keywords](0002-supported-org-mode-subset.md)             | Accepted, amended by 0014, 0020 |
| 0003 | [CLOCK metadata support](0003-clock-metadata-support.md)                               | Accepted |
| 0004 | [TDD is mandatory for code changes](0004-tdd-mandatory.md)                             | Accepted |
| 0005 | [No community meta-docs until a community exists](0005-no-community-meta-docs.md)      | Accepted |
| 0006 | [Do not duplicate registry duplicate-publish guards](0006-no-registry-duplicate-guard.md) | Accepted |
| 0007 | [No test counts in README](0007-no-test-counts-in-readme.md)                           | Accepted |
| 0008 | [Russian-locale defaults: tz, holidays, locale list](0008-rf-defaults.md)              | Accepted |
| 0009 | [Unified date-window semantics for agenda](0009-unified-date-window-semantics.md)      | Accepted |
| 0010 | [Rollback policy for published releases](0010-rollback-policy.md)                      | Accepted |
| 0011 | [Release commit and tag format](0011-release-commit-and-tag-format.md)                 | Accepted |
| 0012 | [Verify Org-mode semantics against upstream Elisp](0012-verify-org-semantics-against-upstream.md) | Accepted |
| 0013 | [Documentation language](0013-documentation-language.md)                               | Accepted |
| 0014 | [Active and inactive timestamps](0014-active-and-inactive-timestamps.md)               | Accepted |
| 0015 | [JSON schema evolution and consumer coordination](0015-json-schema-evolution.md)       | Accepted |
| 0016 | [RUST_LOG overrides --verbose / --quiet](0016-rust-log-cli-precedence.md)              | Accepted |
| 0017 | [No branch protection on master; pre-commit hook policy](0017-no-branch-protection-on-master.md) | Accepted |
| 0018 | [Warning-cookie boundary divergence from upstream](0018-warning-cookie-boundary.md) | Accepted |
| 0019 | [Input encoding expectations](0019-input-encoding-expectations.md)                     | Accepted |
| 0020 | [Task properties via an org-properties fenced code block](0020-task-properties-org-properties-block.md) | Accepted |

## Adding a new ADR

1. Copy an existing file as a starting point, increment the sequence
   number, and pick a short imperative title.
2. Fill in `Context`, `Decision`, `Consequences`. Link to the code,
   commits, or PRs that drove the decision under `References`.
3. Add a row to the [Index](#index) above.
4. Commit the ADR alongside the change it documents -- the ADR is
   part of the change, not a separate follow-up.
