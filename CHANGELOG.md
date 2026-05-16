# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Table of contents

- [\[Unreleased\]](#unreleased)
- [\[0.1.6\] — 2026-05-11](#016--2026-05-11)
- [\[0.1.5\] — earlier](#015--earlier)

## [Unreleased]

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
- `tests/cli.rs` integration tests using `assert_cmd`.
- CI workflow (`.github/workflows/ci.yml`) running `cargo build`, `test`,
  `clippy`, and `fmt --check` on pull requests and pushes to `master`.
- `Display` implementations for `TaskType` and `Priority` (`Todo` →
  `TODO`, `A` → `A`, …). The Markdown/HTML output uses these instead of
  `{:?}` to insulate the format from enum-variant renaming.

### Changed

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
- `build.rs` validates every date in `holidays_ru.json` (year/month/day
  ranges, leap-year-aware February) with a clear panic message instead of
  silently truncating an invalid date.
- `MAX_TASKS` is enforced as a **global** ceiling across all files.
- `clap` arguments now use `ValueEnum` for `--format` and `--agenda`,
  producing typed help output.
- Markdown rendering switched from `format!() + push_str()` to `write!()`
  to reduce intermediate allocations.
- `agenda.rs` pre-parses each task's timestamp once per agenda invocation
  instead of re-parsing it for every day in week/month ranges.
- Duplicate `normalize_weekdays` implementations consolidated into
  `timestamp::weekdays`.
- Dead code (`find_last_occurrence_before`, `is_occurrence_day`) removed.

## [0.1.6] — 2026-05-11

- Version bump.

## [0.1.5] — earlier

- See git history.
