# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Table of contents

- [\[Unreleased\]](#unreleased)
- [\[0.2.2\] — 2026-05-17](#022--2026-05-17)
- [\[0.2.1\] — 2026-05-17](#021--2026-05-17)
- [\[0.2.0\] — 2026-05-17](#020--2026-05-17)
- [\[0.1.6\] — 2026-05-11](#016--2026-05-11)
- [\[0.1.5\] — earlier](#015--earlier)

## [Unreleased]

_No user-visible changes yet._

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
