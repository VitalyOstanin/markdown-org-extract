# Project rules for Claude Code

Architectural and policy decisions live in [`docs/adr/`](docs/adr/).
This file lists the ones the agent must apply during work, with
pointers to the full text and rationale.

## Decisions in force

- TDD is mandatory for every code change. See
  [ADR-0004](docs/adr/0004-tdd-mandatory.md). Documentation-only
  edits are exempt; everything else lands together with tests that
  exercise the actual behaviour.
- Community meta-docs (`CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`,
  `SECURITY.md`, issue / PR templates) are not created without an
  explicit request from the maintainer. See
  [ADR-0005](docs/adr/0005-no-community-meta-docs.md).
- Do not add a custom "version already published" probe before
  `cargo publish`. The registry rejects duplicates server-side
  with a clear error. See
  [ADR-0006](docs/adr/0006-no-registry-duplicate-guard.md). Same
  rule applies to npm / PyPI / Docker Hub.
- Do not put test counts in the README or other docs ("(N tests)",
  "12 tests covering X"). Describe coverage by behaviour, not by
  number. See [ADR-0007](docs/adr/0007-no-test-counts-in-readme.md).
- Russian-locale defaults (`--tz=Europe/Moscow`,
  `holidays_ru.json`, `--locale=ru,en`) are deliberate author
  choices. Do not propose changing them without an explicit
  request. See [ADR-0008](docs/adr/0008-rf-defaults.md).
- The agenda's date-window model is unified across day / week /
  month. See [ADR-0009](docs/adr/0009-unified-date-window-semantics.md)
  for the role of `--date`, `--from`, `--to`, `--current-date` and
  for the priorities between them.

## Background

- [ADR-0001](docs/adr/0001-standalone-cli-for-org-in-markdown.md):
  the project is a standalone Rust CLI with a JSON wire contract on
  stdout.
- [ADR-0002](docs/adr/0002-supported-org-mode-subset.md): which
  Org-mode syntax is parsed and which is out of scope (Obsidian
  Tasks emoji markers and Dataview inline fields are **not**
  parsed).
- [ADR-0003](docs/adr/0003-clock-metadata-support.md): CLOCK
  metadata layout, bracket variants, and total-time calculation.

For agent-level rules that apply across all projects (commit
policy, language of communication, link formatting, resource-limit
hygiene), see the user-level `CLAUDE.md`. This file holds only the
project-specific items.
