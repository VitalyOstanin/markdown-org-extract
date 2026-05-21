# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Table of contents

- [\[Unreleased\]](#unreleased)
- [\[0.3.1\] — 2026-05-19](#031--2026-05-19)
- [\[0.3.0\] — 2026-05-19](#030--2026-05-19)
- [\[0.2.2\] — 2026-05-17](#022--2026-05-17)
- [\[0.2.1\] — 2026-05-17](#021--2026-05-17)
- [\[0.2.0\] — 2026-05-17](#020--2026-05-17)
- [\[0.1.6\] — 2026-05-11](#016--2026-05-11)
- [\[0.1.5\] — earlier](#015--earlier)

## [Unreleased]

### Added

- Range-timestamp dash separator follows Emacs' `org-tr-regexp` and
  now accepts one, two, or three dashes (`-`, `--`, `---`). The
  canonical form on output is two dashes, matching Emacs'
  `org-time-stamp`. The end **date** of a range is still not
  surfaced; see ADR-0002 for the documented scope.
- `--completions <SHELL>` prints a shell completion script on stdout
  and exits. Supports `bash`, `zsh`, `fish`, `elvish`, `powershell`.
  See the new "Shell completions" section in the README for the
  expected install paths.
- `CLICOLOR` and `CLICOLOR_FORCE` env vars are now honoured in
  `--color auto`, joining the existing `NO_COLOR` support. Per the
  [bixense convention](https://bixense.com/clicolors/), `CLICOLOR=0`
  disables colour and `CLICOLOR_FORCE` (non-zero, non-empty) forces
  colour even when stderr is piped. CLI flags (`--color always`,
  `--color never`, `--no-color`) and `NO_COLOR` still win over the
  CLICOLOR variants. The decision logic is now a pure function
  exhaustively unit-tested across the precedence matrix.
- `--help` groups options under named sections (`Input`, `Output`,
  `Agenda`, `Limits`, `Diagnostics`, `Actions`) and now opens with
  an `Examples:` block listing the most useful invocation patterns
  (today's agenda, week / range agenda, flat tasks, holidays,
  shell completion install). `-h` keeps the at-a-glance summary
  without the examples block.

### Fixed

- Multi-segment `--glob` patterns (e.g. `notes/*.md`) now match when
  combined with a relative `--dir`. `WalkBuilder` is fed the canonical
  absolute root, so emitted paths stay descendants of it and the
  `strip_prefix(dir_canonical)` used by glob matching and display-path
  computation no longer drops to the `file_name()` fallback that
  could not match path-segmented patterns.
- A walker error on a single subdirectory (typically `PermissionDenied`)
  no longer aborts the whole scan. The error is counted in a new
  `walk_errors` field of the processing summary, the failing entry is
  appended to `failed paths`, and the rest of the tree is scanned as
  usual.

### Changed

- Unified the agenda window across `day`, `week`, `month` modes
  ([ADR-0009](docs/adr/0009-unified-date-window-semantics.md)).
  `--from`/`--to` are now first-class window controls in every
  non-tasks mode (day mode previously ignored them silently).
  A single edge fills the other side from `--current-date`
  (or today): `--from X` -> `[X..current_date]`, `--to Y` ->
  `[current_date..Y]`. `--date` selects the window when no
  `--from`/`--to` is given. In day mode that is a single day; in
  week / month it is the week / month containing the date.
  `tasks` mode now rejects all date arguments (`--date`, `--from`,
  `--to`, `--current-date`) instead of silently ignoring them.
- An unknown `--locale` entry (e.g. `--locale ru,de`) is now a hard
  error at CLI parse time (exit code 2) instead of a `tracing::warn!`
  that `--quiet` could swallow. Empty segments are still tolerated, so
  `--locale ru,` and `,en` keep working. The previous warn-only
  behaviour silently dropped translations for unrecognised locales,
  which was indistinguishable from a successful run.
- Exit codes now reflect the error category instead of a uniform `1`.
  Usage / input-validation errors (invalid `--dir`, `--glob`, `--date`,
  `--tz`, `--output`, `from > to`) exit with code `2`. IO failures
  (unreadable files, walker errors, write failures) exit with `74`
  (`EX_IOERR` from `sysexits.h`). Internal software errors (regex
  compile, serializer) exit with `70` (`EX_SOFTWARE`). Scripts that
  shelled out to the binary and only checked for `!= 0` are
  unaffected; scripts that branched on `== 1` may need to switch to
  `!= 0` or to the new specific codes.

## [0.3.1] — 2026-05-19

Patch release. No user-visible code changes — both fixes are about
release-pipeline correctness and CI matrix coverage.

### Fixed

- `tests/release_check_changelog.rs` is now gated behind `#![cfg(unix)]`.
  These integration tests drive `scripts/check-changelog.sh` through
  `Command::new("bash")`; on `windows-latest` GitHub Actions runners the
  Git for Windows bash plus default CRLF line endings caused 6 of the 8
  tests to fail with empty stderr. The script itself is a POSIX bash
  helper that runs only on the ubuntu-24.04 release runner — its
  behaviour on Windows is not part of any production code path, so
  compile-gating the file keeps the Windows CI matrix green without
  removing any Linux/macOS coverage.

### Changed

- `.github/workflows/release.yml` is hardened against publishing on a
  failing test suite. Previously the workflow contained only a single
  `publish` job that combined fmt/clippy/cargo-test/smoke-test/publish
  on ubuntu-24.04 — `release.yml` and `ci.yml` were independent, so a
  failing CI run on the same commit did not block a tag-triggered
  publish. The workflow now has four jobs:
  - `test` — `cargo test --all-features` on the same matrix as CI
    (`ubuntu-24.04`, `macos-latest`, `windows-latest`).
  - `lint` — `cargo fmt --check` + `cargo clippy -D warnings`.
  - `msrv` — `cargo build --locked` against the declared MSRV (1.85).
  - `publish` — declares `needs: [test, lint, msrv]`. Any failing
    pre-publish job aborts the workflow before `cargo publish` is
    invoked.
  The duplicated `cargo fmt`/`clippy`/`cargo test` steps inside the old
  `publish` job were removed since those gates are now enforced by the
  dedicated jobs. The LTO release smoke test, CHANGELOG gate, version
  cross-check, and `cargo publish --locked` step are unchanged.

### Documentation

- README now uses a CI status badge instead of a docs.rs badge. The
  project is a binary-only crate (no `src/lib.rs`), so docs.rs cannot
  build documentation for it (`cargo doc` reports
  `no library targets found in package`) and the badge stays
  permanently red on every published version. The CI badge points at
  `.github/workflows/ci.yml` on `master` and conveys actually useful
  information.
- The `documentation = "https://docs.rs/markdown-org-extract"` field
  removed from `Cargo.toml` for the same reason — docs.rs has no
  rendered documentation for this crate and the link only leads to a
  failed build page.

### Context

The 0.3.0 release went out while the `test (windows-latest)` job of
`ci.yml` was red. That failure was the very `release_check_changelog.rs`
problem this release fixes — and there was nothing in the release
pipeline to notice it. 0.3.0 is not yanked because the actual crate
content is unaffected (only the release helper's test on Windows was
broken); users running `cargo install markdown-org-extract` or pulling
the crate as a dependency are not exposed to the failure.

## [0.3.0] — 2026-05-19

### Added

- `--color auto|always|never`: standard Rust-ecosystem control over
  diagnostic colour, with precedence Always > Never / `--no-color` >
  `NO_COLOR` > stdout-is-TTY. `--no-color` is now a shortcut for
  `--color never` and conflicts with `--color`.
- `--agenda tasks`: new mode mirroring the legacy `--tasks` bool flag.
  Both produce the flat task list; the bool flag wins when both are
  set so existing pipelines keep working.
- `--output -`: the standard unix sigil for stdout. No file named `-`
  is created; the result is written to stdout instead.
- `--locale` now warns (`tracing::warn!`) when given a value that is
  not in the supported set (`ru`, `en`). Silently dropping
  `--locale es,de` previously left the user with no weekday mappings
  and no signal.
- `tracing` spans (`debug_span!("file", path = ...)`) wrap per-file
  task extraction so every event emitted by the parser, timestamp
  extractor, and clock extractor inherits `path = ...`. Multi-file
  runs at `-vv` now produce per-file event groups instead of one
  undifferentiated stream.
- `holidays_ru.json` carries a `_meta` block (description, source,
  licence, schema) so the calendar's attribution survives even if the
  README is forked away from the data file. `build.rs` ignores
  underscore-prefixed top-level keys, so the block has no effect on
  the compiled-in `HOLIDAYS` / `WORKDAYS` arrays.
- README documents the `holidays_ru.json` provenance in a dedicated
  section under the licence chapter.
- A dedicated `msrv` CI job builds with the toolchain pinned to 1.85.
  `rust-version` in `Cargo.toml` is only a soft check, so a
  stable-only matrix could otherwise mask a regression that prevented
  users on the declared MSRV from compiling.

### Changed

- **Breaking** — MSRV raised from 1.80 to 1.85. Required by the
  `comrak` 0.50+ upgrade, which moved the crate to the Rust 2024
  edition. Users on Rust < 1.85 cannot build this version; install
  the previous release (0.2.2) or upgrade the toolchain.
- **Breaking** — `validate_date` (which covers `--date`, `--from`,
  `--to`, and `--current-date`) now rejects years outside
  1900..=2100, matching the bound long applied by `--holidays`.
  Without this cap an extreme `--current-date 5000-01-01 +1y` could
  spin a repeater for thousands of iterations.
- **Breaking** — every validator message drops the leading
  `Invalid <kind> '<v>':` prefix and is lowercase. clap already
  prefixes the value with `error: invalid value '<v>' for '--<arg>':`,
  and the doubled noun read as stuttering. Scripts that grep the
  exact prefix `Invalid` need to be updated.
- `comrak` dependency bumped from 0.48 to 0.52. Backward-compatible
  for our usage of `NodeValue::{Heading, Paragraph, Code, CodeBlock,
  Text, Emph, Strong, Link, Strikethrough}`; no parser code changes
  were needed.
- Help text rewrites: `--no-color` no longer reads as ambiguous
  ("honors NO_COLOR as well") and instead says "`NO_COLOR` has the
  same effect" with a reference to no-color.org. `--format` help
  mentions the `md` alias for `markdown` explicitly so the alias is
  discoverable from both `-h` and `--help`.
- `validate_max_tasks` distinguishes `IntErrorKind::PosOverflow`
  ("out of range, must be at most 10_000_000") from non-numeric
  garbage ("must be a positive integer up to 10_000_000"). On 32-bit
  targets `usize` overflows above the cap and the old message read
  as "not a number".
- `validate_timezone` propagates the chrono-tz `Display` verbatim and
  keeps the IANA hint; it no longer echoes the input value.
- Agenda mode is threaded through internally as a closed
  `AgendaScope` enum (`Day`/`Week`/`Month`/`Tasks`) instead of a
  stringly-typed `&str`. The fall-through `_ => InvalidDate(...)`
  arm is now impossible by construction.
- Repeating tasks now surface on occurrence days in week and month
  agenda, including past occurrences inside the window. Previously
  the occurrence check rejected anything strictly before "today".
- `--from > --to` is rejected with `AppError::DateRange` instead of
  producing an empty agenda.
- `render_markdown` and `render_html` are collapsed into a single
  implementation shared by `--tasks` and the agenda day view. The
  `Type:` field consistently uses `TODO` / `DONE` (the README was
  out-of-date with `Todo` / `Done` after the 0.2.0 `Display` change),
  and `Priority:` is a bare letter rather than the `[#A]` wrapper.
- README examples section refreshed: the bundled-examples list grew
  from 3 files to all 13 (grouped by intent — general scenarios,
  org-mode label demos, CLOCK-block demos). JSON example for
  `--tasks` updated to reflect actual output
  (`#[serde(skip_serializing_if = "Option::is_none")]` strips
  `null`-valued optional fields, so they no longer appear).

### Fixed

- TOCTOU window in `scan_files`: `fs::metadata().len()` followed by
  `fs::read()` was two separate syscalls, leaving a window where a
  file could grow or be swapped for a symlink between size check and
  content read. `read_capped()` now opens the file once and uses
  `Read::take(cap + 1).read_to_end()`; oversized files are detected
  without re-statting. Defense-in-depth; the local filesystem is
  still trusted as a security boundary.
- `validate_output_path` distinguishes `io::ErrorKind::NotFound`
  (writing to a fresh file, the normal case) from any other
  `symlink_metadata` error. `PermissionDenied` / `EIO` on the target
  used to be swallowed and surface later as a confusing `fs::write`
  failure; they now fail loudly at validation with a precise message.
- `compile_glob` preserves the `globset::Error` `source()` chain via
  a small `format_error_chain` helper, so the user sees the
  underlying brace/range parse failure, not just the top-level
  `Display`.
- `parse_repeater` no longer panics on multibyte UTF-8 trailing
  characters such as `+1й` or `+1🙂`. The unit-character extraction
  switches from byte slicing to `last().len_utf8()`. Every rejection
  branch now emits a `tracing::trace!` with a specific reason
  ("missing prefix", "non-numeric value", "unknown unit", "zero
  step", and so on).
- `extract_clocks` no longer panics on hostile input: the `.expect()`
  is gone, and the regex rejects mismatched bracket pairs (`[…>`,
  `<…]`) at parse time.
- `calculate_total_minutes` returns `Some(0)` when at least one entry
  carries a parseable `duration` (even when the sum is zero) and
  `None` only when nothing contributed. Previously a legitimate
  `0:00` CLOCK was indistinguishable from "no duration recorded".
- Year-repeater walk skips Feb-29 in non-leap years instead of
  truncating to Feb-28. Month-repeater preserves `base_day` across
  month-length truncations.
- `parse_heading` no longer relies on `caps.get(0).unwrap()`. The
  capture was bounded by the regex, but the explicit `?` is safer
  against future regex edits.
- Bare `[#A]` priority is recognised on a heading without a preceding
  `TODO` / `DONE` keyword. This was the 0.2.2 hotfix, now folded into
  the parser rewrite cleanly.
- The 20-entry diagnostic caps for failed/skipped paths and invalid
  timestamps are unified under `types::MAX_DIAGNOSTIC_ITEMS`; their
  independence used to be incidental.

### Removed

- `CONTRIBUTING.md`. The project does not yet have an external
  contributor community, and the document had drifted from the
  actual release workflow. Project conventions now live in
  `CLAUDE.md` and in the `.github/workflows/` files themselves.
- Numeric test counts in the README (`(9 tests)`, `(6 tests)`,
  `(2 tests)`). They were already out of sync with reality, and
  every new task forced an unrelated README update; bullet "what is
  covered" lists carry the same information without the maintenance
  debt.
- `next_occurrence` (a 125-line dead-code helper) and its
  `#[allow(dead_code)]` marker.

### Internal

- `closest_date` decomposed from a 188-line monolith into
  bracket-per-unit helpers (`bracket_year`, `bracket_month`,
  `bracket_uniform_days`, `bracket_workday`) plus a single
  `pick(prefer, ...)` for the Past / Future selection.
- The CLOCK regex body is bounded by a named `CLOCK_BODY_MAX = 128`
  constant declared in `src/regex_limits.rs`; same idea for
  `TS_BODY_MAX = 256` used by `src/timestamp/extract.rs`. Boundary
  tests pin both `len == cap` (must match) and `len == cap + 1`
  (must not).
- `RU_WEEKDAY_MAPPINGS` exported from `src/cli.rs` as `pub(crate)`
  so the parser test that re-runs the production pipeline can import
  the same table instead of drifting from it.
- `MAX_DIAGNOSTIC_ITEMS` exported from `src/types.rs`.

### Release process and CI

- The release workflow now smoke-runs the LTO-enabled release binary
  immediately before publish so optimiser-only regressions (UB,
  dead-code elimination collapsing a side effect, etc.) surface
  here instead of by downstream `cargo install` users.
- The release workflow refuses to publish unless `Cargo.lock`'s
  resolved version for the crate matches the tag. A stale lock used
  to surface as a confusing `--locked` failure later in publish.
- `cargo publish --dry-run` and `cargo publish` are both pinned with
  `--locked` so the resolved dependency graph at the tagged commit
  is what ships.
- Cargo.toml version is now extracted with
  `cargo pkgid | sed -E 's/.*#//'` instead of an ad-hoc `awk`
  pattern that could have matched a `version = ...` line inside a
  `[dependencies.*]` block before `[package]`.
- A single `Resolve tag and version` step produces
  `steps.tag.outputs.{tag, version}` as the source of truth for
  later steps.
- Publication is gated on `scripts/check-changelog.sh "$VERSION"`,
  which fails if `## [<version>]` is missing from CHANGELOG.md or
  if `## [Unreleased]` still carries entries that were not moved
  to the released version's section.
  `tests/release_check_changelog.rs` pins the script's behaviour.
- `actions/checkout` bumped from v4.3.1 to v6.0.2 across all
  workflows (ci, release, outdated).
- `Cargo.toml` `exclude` now lists `scripts/` so the publish-time
  helpers shipped in this release do not end up inside the crate
  tarball.

### Project documentation

- `README.md`, `CLAUDE.md`, and `TODO.md` translated to English so
  the public-facing documentation matches the language used in
  source comments, CLI help, and CHANGELOG. The Russian-weekday
  examples in `README.md` (under "Locale support") are preserved
  intentionally, since they demonstrate the project's
  Russian-weekday recognition.
- Added project-level `CLAUDE.md` capturing TDD-on-every-change,
  no-community-meta-docs-yet, no-registry-duplicate-guards,
  no-test-counts-in-README, and RU-default-intentional rules.

## [0.2.2] — 2026-05-17

### Fixed

- Heading parser now recognises a priority cookie `[#A]` / `[#1]` that is
  not preceded by `TODO` or `DONE`. Previously the cookie ended up as part
  of `task.heading` and `task.priority` was `null`, while in emacs org-mode
  the cookie is parsed independently of the TODO keyword
  (`org-element--headline-parse-title` / `org-priority-regexp`).
- Heading parser follows org-mode's `.*?` semantics: `[#X]` is matched at
  any position after the optional TODO/DONE keyword. The text between the
  keyword (or the start of the heading) and the cookie is dropped, matching
  `goto-char (match-end 0)` in the reference implementation. Example:
  `### TODO Buy [#A] filter` now yields `priority=A`, `heading="filter"`.

### Added

- Numeric priorities `[#0]`..`[#64]` from emacs org-mode
  (`org-priority-value-regexp = "[A-Z]\\|[0-9]\\|[1-5][0-9]\\|6[0-4]"`).
  Values outside this range stay inside the heading verbatim.
- `Priority::parse(&str)` replaces `Priority::from_char(char)` so multi-digit
  numeric values can be parsed in a single call.

### Changed

- **Breaking (JSON)**: `Priority` is now serialised as a plain string in all
  outputs. Previously `Priority::Other('D'..='Z')` was emitted as
  `{"Other":"D"}` due to the default `serde` enum representation, which most
  consumers could not interpret as a priority. After the change, every
  priority — `A`, `B`, `C`, `D`..`Z`, or `0`..`64` — is a string (e.g. `"A"`,
  `"D"`, `"5"`). Deserialisation also accepts an integer for backward
  convenience.
- `Priority::order()` now matches `org-priority-to-value`: numeric priorities
  map to their integer value (`0..=64`), letters map to their ASCII code
  (`A`=65, …, `Z`=90). This means numeric priorities sort _before_ all
  letter priorities, which is the same total order emacs uses.

## [0.2.1] — 2026-05-17

### Fixed

- Parser now recognises `DEADLINE:` / `SCHEDULED:` / `CREATED:` planning
  lines when they are both indented (4-space indent that markdown treats
  as an indented code block) and wrapped in inline-code backticks, e.g.
  `    \`DEADLINE: <2026-05-07 Thu +1y>\``. Previously the wrapping
  backticks prevented the keyword regex from anchoring, and the heading
  was dropped from the agenda entirely. Matches emacs org-mode, which
  surfaces such entries regardless of the visual framing.

## [0.2.0] — 2026-05-17

### Breaking

- MSRV raised from **1.70 to 1.80**. The crate now relies on
  `std::sync::LazyLock` for global regex statics, which stabilized in 1.80.

### Fixed

- `closest_date` for workday repeaters (`+Nwd`) now advances the cursor in
  steps of `N` workdays. Previously `+2wd` behaved like `+1wd`.
- `next_occurrence` for cumulative workday repeaters with `value > 1` is
  corrected analogously.
- Hour repeater (`+Nh`) in `closest_date` now projects onto the daily grid
  instead of always returning `current`. Result: agenda entries with `+1h`
  no longer appear on every day regardless of the base date.
- Range timestamps `<...>--<...>` no longer drop the start time of the
  second bracket. It is now exposed as `timestamp_end_time`.
- Repeater type prefix (`+` / `++` / `.+`) is preserved when an agenda
  entry's timestamp is rewritten for the occurrence day.
- `closest_date` with `current < base_date`:
  - `Past` now returns `None` (no past occurrence exists yet);
  - `Future` returns `base_date` (first occurrence).
  Agenda still shows such tasks on `base_date` and in the upcoming bucket
  if the DEADLINE falls within the warning window.
- Year repeater (`+1y`) on a leap-day base (`02-29`) no longer truncates
  to `02-28` in non-leap years; instead it skips to the next leap year.
- Month repeater preserves the original `base_day` across truncations.
- `days_in_month` no longer returns 30 as a fallback for invalid months;
  invalid values now panic loudly (the function is only reachable with
  validated input).
- `parse_repeater` rejects `+0d` / `+0wd` / etc., preventing a runtime
  division-by-zero panic in occurrence math.
- `next_occurrence` for `CatchUp` weekly/monthly/yearly repeaters with
  `value > 1` now lands on the correct repeater grid.
- `parse_timestamp_fields` no longer misclassifies the timestamp type
  based on the body — it anchors on the leading `SCHEDULED:` / `DEADLINE:` / `CLOSED:`.
- Body text inside `Emph` / `Strong` / `Link` nodes is now included in
  both headings and paragraphs.

### Added

- `LICENSE` (MIT) file, `CONTRIBUTING.md`, and this `CHANGELOG.md`.
- `--absolute-paths` CLI flag. Default output now uses paths **relative to
  `--dir`**, which avoids leaking absolute filesystem paths into JSON /
  Markdown / HTML output.
- `--max-tasks <N>` CLI flag (range `1..=10_000_000`). Replaces the
  hard-coded ceiling and is enforced both per-file and globally.
- Diagnostic-output controls: `--verbose` / `-v` (repeatable: `-v` info,
  `-vv` debug, `-vvv` trace), `--quiet` / `-q`, and `--no-color`. Output is
  routed through `tracing` + `tracing-subscriber`, honouring the `NO_COLOR`
  environment variable as well.
- `tests/cli.rs` integration tests using `assert_cmd` (including coverage
  for `--verbose` / `--quiet` mutual exclusion, `--no-color`, and
  `--max-tasks` bounds).
- CI workflow (`.github/workflows/ci.yml`) running `cargo build`, `test`,
  `clippy`, and `fmt --check` on pull requests and pushes to `master`.
- `.github/workflows/outdated.yml` — weekly `cargo outdated` check
  (also `workflow_dispatch`-runnable, non-blocking).
- `workflow_dispatch` trigger in `release.yml` with `tag` and `dry_run`
  inputs for ad-hoc / dry-run publication.
- `rust-toolchain.toml` and `rustfmt.toml` to pin the toolchain channel
  and formatting baseline for contributors.
- `holidays::workdays_between_exclusive` and `holidays::nth_workday_after`
  enabling `O(log²n)` resolution of `+Nwd` workday repeaters
  (replacing the previous linear day-by-day scan).
- Integration test that compares the compiled-in `HOLIDAYS` / `WORKDAYS`
  arrays against `holidays_ru.json` to guard against build-pipeline drift.
- crates.io / docs.rs / license badges and a `cargo install
  markdown-org-extract` section in `README.md`.
- `Display` implementations for `TaskType` and `Priority` (`Todo` →
  `TODO`, `A` → `A`, …). The Markdown/HTML output uses these instead of
  `{:?}` to insulate the format from enum-variant renaming.

### Changed

- All `once_cell::sync::Lazy` regex statics migrated to
  `std::sync::LazyLock` (`clock.rs`, `parser.rs`, `regex_limits.rs`,
  `timestamp/extract.rs`, `timestamp/parser.rs`). `once_cell` is no longer
  a runtime dependency.
- CLOCK timestamp regex tightened to **homogeneous brackets** — only
  `[...]` or `<...>` pairs are accepted; mixed forms like `[...>` or
  `<...]` are rejected.
- `closest_date` for `+Nwd` workday repeaters now runs in `O(log²n)` via
  a binary search over precomputed prefixes of the workday/holiday lists
  (verified by an oracle sweep across all 365 days of 2026 for step
  ∈ {1, 2, 3, 5} × {Past, Future}).
- `HolidayCalendar` is now a process-wide singleton accessed via
  `HolidayCalendar::global()` (`std::sync::OnceLock`). The previous
  `HolidayCalendar::load() -> Result<…>` API has been removed because it
  could not fail.
- `WalkBuilder` is explicitly configured with `follow_links(false)` and
  `same_file_system(true)` as defense-in-depth.
- Source files are now read **once**: a single `fs::read` feeds both the
  keyword pre-filter (`grep_searcher::Searcher::search_slice`) and the
  parser. Previously each candidate file was opened twice.
- `--output` path is validated before serialization: the parent directory
  must exist and the target must not be an existing symlink.
- `build.rs` validates every date in `holidays_ru.json` using
  `chrono::NaiveDate::parse_from_str` (strict `YYYY-MM-DD`, leap-year
  aware) and panics with a clear message instead of silently truncating
  an invalid date.
- The 10 000-task ceiling is enforced as a **global** cap across all
  files (configurable via `--max-tasks`).
- `clap` arguments now use `ValueEnum` for `--format` and `--agenda`,
  producing typed help output.
- All `eprintln!` diagnostics in production code replaced with
  `tracing::warn!` (`parser.rs`, `types.rs::print_summary`).
- Markdown rendering switched from `format!() + push_str()` to `write!()`
  to reduce intermediate allocations.
- `agenda.rs` pre-parses each task's timestamp once per agenda invocation
  instead of re-parsing it for every day in week/month ranges.
- `is_today` for non-repeating tasks is computed inline inside
  `handle_non_repeating_task`, eliminating an out-of-band parameter.
- Duplicate `normalize_weekdays` implementations consolidated into
  `timestamp::weekdays`.
- GitHub Actions in `release.yml` and `ci.yml` are SHA-pinned
  (`actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`,
  `rustsec/audit-check`).
- Dead code (`find_last_occurrence_before`, `is_occurrence_day`) removed.
- Debug `eprintln!` lines in `#[cfg(test)]` blocks (`agenda.rs`,
  `repeater.rs`) replaced with descriptive `assert!` messages.

### Removed

- `once_cell` runtime dependency (superseded by `std::sync::LazyLock`).

## [0.1.6] — 2026-05-11

- Version bump.

## [0.1.5] — earlier

- See git history.
