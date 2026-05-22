# ADR-0013: Documentation language

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

The project was originally written with Russian-language documentation
in `README.md`, `CLAUDE.md`, `TODO.md`, and a few notes under `docs/`.
The 0.3.0 cycle translated the user-facing files to English (commit
`e98877c docs(i18n): translate README, CLAUDE, TODO to English`). The
ADR migration in commit `c710119 docs(adr): introduce ADRs and slim
CLAUDE.md` removed the last two Russian-language explanatory docs
(`docs/CLOCK_IMPLEMENTATION.md`, `docs/org-mode-keywords.md`) -- their
content folded into ADR-0002 and ADR-0003, both English. So today the
user-facing surface is fully English.

The remaining Russian-language artefacts are:

- Examples under `examples/` that exercise the Russian-weekday
  recognition path (`Пн`, `Понедельник`, ...). The Russian text is
  the test fixture: replacing it would erase what the example is
  there to demonstrate.
- Reports under `docs/reviews/` produced by the project-check
  reviewer pipeline. These are internal research artefacts, not part
  of what an external consumer reads. The author works in Russian and
  the reviewer prompts run in Russian.

The 2026-05-21 review surfaced "language of `docs/` is mixed" as a
documentation finding. The finding is true but its examples
(`docs/CLOCK_IMPLEMENTATION.md`, `docs/org-mode-keywords.md`) no
longer exist after the ADR migration. The actual question to settle
is which language `docs/reviews/` and any future `docs/*` content
should use, and how that decision is documented so a future reviewer
does not re-raise it.

`docs/adr/README.md:26-28` already fixes the language for ADRs in
particular ("ADRs are written in English regardless of the rest of
the project's documentation language"). This ADR generalises that
rule to the rest of `docs/`.

## Decision

Documentation language by surface:

| Surface                                            | Language    | Rationale                                                                 |
| -------------------------------------------------- | ----------- | ------------------------------------------------------------------------- |
| `README.md`                                        | English     | crates.io / GitHub front page; first-contact surface for any consumer.    |
| `CHANGELOG.md`                                     | English     | machine-parsed by release tooling; consumed alongside `README.md`.        |
| `TODO.md`                                          | English     | tracked alongside the user-facing files; deferred features are public.    |
| `CLAUDE.md`                                        | English     | tracked in the repo; used by the agent against the English code surface.  |
| `docs/adr/`                                        | English     | already fixed in `docs/adr/README.md:26-28`; ADRs are part of the contract. |
| `docs/reviews/`                                    | Russian     | local research artefacts; not part of the user-facing surface.            |
| `examples/` Russian content                        | Russian     | required by the demonstration of Russian-weekday recognition.             |
| Source-code identifiers, comments, doc comments    | English     | consistent with the Rust ecosystem; required by clippy `missing_docs`.    |
| Commit messages, PR descriptions, tag annotations  | English     | shared with the user-facing files in tooling and history.                 |

Any new file under `docs/` outside `docs/reviews/` is English by
default. Adding a non-English research surface (e.g. a new
`docs/proposals-ru/`) requires updating this ADR with the new
location and the reason it is exempt.

The Russian-language examples are covered by ADR-0008 (RF defaults)
and are not the subject of this ADR; they stay as is.

Reviewer findings of the form "language of `docs/` is mixed" or
"translate `docs/reviews/` to English" close with a pointer to this
ADR. Reopening requires a superseding ADR.

## Consequences

Easier:

- A reviewer or external contributor reading the repo sees a clear
  rule for each file. No case-by-case judgement needed.
- The agent does not need to re-derive the language from neighbouring
  files when adding new documentation; the rule is mechanical.
- `docs/reviews/` can stay in the author's working language without
  the contradiction of being half-translated.

Harder:

- A future change that exposes `docs/reviews/` to an external
  audience (publishing reviews as part of release notes, for
  instance) needs a deliberate translation step or a superseding
  ADR. The cost is one-time per such change.
- Adding a new English `docs/*` subdirectory must include a TOC,
  per the user-level CLAUDE.md rule on `.md` files. Not new but
  worth restating because this ADR generalises the surface.

## References

- 2026-05-21 review finding "language of `docs/`":
  [`docs/reviews/2026-05-21-1811-review.md`](../reviews/2026-05-21-1811-review.md)
- 0.3.0 translation: `e98877c docs(i18n): translate README, CLAUDE, TODO to English`.
- ADR migration removing the last Russian docs:
  `c710119 docs(adr): introduce ADRs and slim CLAUDE.md`.
- ADR-language rule already in force:
  [`docs/adr/README.md:26-28`](README.md).
- RF-defaults policy that keeps Russian examples in `examples/`:
  [ADR-0008](0008-rf-defaults.md).
