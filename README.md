# markdown-org-extract

[![crates.io](https://img.shields.io/crates/v/markdown-org-extract.svg)](https://crates.io/crates/markdown-org-extract)
[![CI](https://github.com/VitalyOstanin/markdown-org-extract/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/VitalyOstanin/markdown-org-extract/actions/workflows/ci.yml?query=branch%3Amaster)
[![license](https://img.shields.io/crates/l/markdown-org-extract.svg)](https://github.com/VitalyOstanin/markdown-org-extract/blob/master/LICENSE)

CLI utility for extracting tasks from markdown files with support for Emacs Org-mode markers.

## Table of contents

- [Installation and build](#installation-and-build)
- [For downstream packagers](#for-downstream-packagers)
- [Usage](#usage)
- [Example files](#example-files)
- [Agenda modes](#agenda-modes)
- [Supported markers](#supported-markers)
- [Locale support](#locale-support)
- [Output format](#output-format)
- [Repeating tasks](#repeating-tasks)
- [Project layout](#project-layout)
- [Dependencies](#dependencies)
- [License](#license)

## Installation and build

### Requirements

- Rust 1.85 or newer (the `comrak` 0.50+ upgrade requires the 2024 edition)
- Cargo

### Install from crates.io

If you only need the binary and do not want to clone the repository:

```bash
cargo install markdown-org-extract
```

After installation the binary lands in `~/.cargo/bin/markdown-org-extract`
(this path must be on your `PATH`).

### Shell completions

The binary can emit its own completion script for `bash`, `zsh`, `fish`,
`elvish`, and `powershell` via `--completions <SHELL>`. The script is
printed to stdout; redirect it to wherever your shell expects
completions.

```bash
# bash (user-local)
mkdir -p ~/.local/share/bash-completion/completions
markdown-org-extract --completions bash \
    > ~/.local/share/bash-completion/completions/markdown-org-extract

# zsh (add a directory to $fpath, e.g. ~/.zfunc)
markdown-org-extract --completions zsh \
    > ~/.zfunc/_markdown-org-extract

# fish
markdown-org-extract --completions fish \
    > ~/.config/fish/completions/markdown-org-extract.fish
```

Reload the shell or re-source its config after writing the file.

### Building the project

Debug build:
```bash
cargo build
```

Optimised release build:
```bash
cargo build --release
```

The resulting binary appears in:
- Debug: `target/debug/markdown-org-extract`
- Release: `target/release/markdown-org-extract`

### Running

After building, run the utility:

```bash
# Debug build
./target/debug/markdown-org-extract [OPTIONS]

# Release build
./target/release/markdown-org-extract [OPTIONS]
```

Or use cargo to run it without an explicit build step:
```bash
cargo run -- [OPTIONS]
```

### Testing

Run the test suite:
```bash
cargo test
```

Run with verbose output:
```bash
cargo test -- --nocapture
```

Static checks:
```bash
cargo check
cargo clippy
```

#### Workday-handling test coverage

`holidays` module:
- Loading the holiday calendar
- Distinguishing regular weekends and working days
- 2025 New Year holidays (1–8 January) and 2026 (1–9 January)
- 2026 holiday shifts (8 March → 9 March, 9 May → 11 May)
- Skipping weekends and holidays when locating the next working day

`timestamp::repeater` module:
- Parsing repeaters `+1wd`, `+2wd`, `++1wd`, `.+1wd`
- Computing the next occurrence over working days
- Skipping holidays in repeater arithmetic

`timestamp::parser` module:
- Parsing timestamps that carry `+1wd` and `+2wd`

## For downstream packagers

This section documents the contract that the GitHub Release
artefacts keep for downstream packagers (distro maintainers,
Nix derivations, private mirrors, automated bootstrappers).
Within a major version the layout below will not change without
a CHANGELOG entry and a CHANGELOG-referenced ADR.

### Asset naming

Each release publishes one archive per platform target:

| Target                       | Archive extension | Binary name                |
|------------------------------|-------------------|----------------------------|
| `x86_64-unknown-linux-gnu`   | `.tar.gz`         | `markdown-org-extract`     |
| `aarch64-apple-darwin`       | `.tar.gz`         | `markdown-org-extract`     |
| `x86_64-pc-windows-msvc`     | `.zip`            | `markdown-org-extract.exe` |

The archive filename template is:

```
markdown-org-extract-<version>-<target>.<ext>
```

Example asset set for tag `v0.3.1`:

```
markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu.tar.gz
markdown-org-extract-0.3.1-aarch64-apple-darwin.tar.gz
markdown-org-extract-0.3.1-x86_64-pc-windows-msvc.zip
```

`<version>` is the tag stripped of its leading `v`, identical to
the `[package].version` field in `Cargo.toml` for that commit
(the `publish` job in `.github/workflows/release.yml` fails the
release if the two diverge).

### Archive layout

Each archive extracts to a single top-level directory whose name
matches the archive stem:

```
markdown-org-extract-<version>-<target>/
├── markdown-org-extract       # markdown-org-extract.exe on Windows
├── README.md
└── LICENSE
```

No nested target subdirectories, no separate debug symbols, no
manpages. Adding a file to the staged directory is a contract
change (CHANGELOG entry + ADR).

### Checksums

Every archive ships with a sibling `.sha256` file in the standard
`sha256sum` format (`<hex>  <filename>`):

```
markdown-org-extract-<version>-<target>.<ext>
markdown-org-extract-<version>-<target>.<ext>.sha256
```

Verification with the GNU tool:

```bash
sha256sum -c markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu.tar.gz.sha256
```

A `SHA256SUMS` aggregate file is not currently published. If one
is added later, the per-archive `.sha256` companions will remain
in place for at least one major-version cycle.

### Reproducibility

Linux and macOS archives are produced with `tar --sort=name
--owner=0 --group=0 --numeric-owner --mtime='@0'`; the Windows
zip uses `7z -mtc=off` to strip per-file timestamps. Re-running
the release workflow on the same commit produces byte-identical
archives and matching SHA-256 values.

### Compatibility floor

- Crate MSRV: 1.85 (declared in `Cargo.toml` and verified by the
  `msrv` CI job). Building from source requires at least this
  toolchain version.
- Build hosts: GitHub-hosted runners current at release time
  (`ubuntu-24.04`, `macos-latest`, `windows-latest`). The Linux
  binary links against the glibc bundled with Ubuntu 24.04;
  older glibc baselines require building from source.
- No runtime native dependencies: the Russian holiday calendar
  is embedded at compile time via `build.rs`.

### Download patterns

The GitHub Release download URL is stable across releases:

```
https://github.com/VitalyOstanin/markdown-org-extract/releases/download/v<version>/markdown-org-extract-<version>-<target>.<ext>
https://github.com/VitalyOstanin/markdown-org-extract/releases/download/v<version>/markdown-org-extract-<version>-<target>.<ext>.sha256
```

`releases/latest` resolves to the most recent non-pre-release;
suitable for unattended downloads when a specific tag is not
required.

### Out of scope

- The binaries are unsigned. Trust is anchored in TLS to GitHub
  plus the published SHA-256 values.
- Distribution-specific repacks (`.deb`, `.rpm`, AUR, MacPorts,
  Homebrew formula) are not maintained by this project; the
  upstream artefact is the GitHub Release archive.
- Additional targets (`aarch64-unknown-linux-gnu`,
  `x86_64-apple-darwin`, musl variants) may be added in a future
  minor release. Removal of a previously published target
  requires a major-version bump.

## Usage

```bash
markdown-org-extract [OPTIONS]
```

### Options

- `--dir <DIR>` — directory to scan (default: `.`)
- `--glob <GLOB>` — file filter pattern (default: `*.md`)
- `--format <FORMAT>` — output format: `json`, `md`, `html` (default: `json`)
- `--output <OUTPUT>` — file to write the result to; `-` means stdout (default: stdout)
- `--locale <LOCALE>` — weekday locales, comma-separated (default: `ru,en`)
- `--agenda <MODE>` — agenda mode: `day`, `week`, `month`, `tasks` (default: `day`)
- `--tasks` — show all TODO tasks sorted by priority (alias for `--agenda tasks`)
- `--date <DATE>` — window anchor for `day`/`week`/`month` mode in `YYYY-MM-DD`. In `day` mode the window is exactly this date; in `week`/`month` it is the week / month containing this date. Overridden by `--from`/`--to`. Not allowed in `tasks` mode. Default: `--current-date` (or today)
- `--from <DATE>` — window start (`YYYY-MM-DD`) for `day`/`week`/`month` mode. Together with `--to`, an explicit range that overrides `--date`. If `--to` is omitted, the window ends at `--current-date` (or today). Not allowed in `tasks` mode
- `--to <DATE>` — window end (`YYYY-MM-DD`) for `day`/`week`/`month` mode. Together with `--from`, an explicit range that overrides `--date`. If `--from` is omitted, the window starts at `--current-date` (or today). Not allowed in `tasks` mode
- `--tz <TIMEZONE>` — IANA timezone for determining the current date (default: `Europe/Moscow`)
- `--current-date <DATE>` — override of "today" (`YYYY-MM-DD`). Used as the reference for overdue / upcoming markers and as the default for a missing `--from`/`--to` edge. Not allowed in `tasks` mode. Default: today in `--tz`
- `--holidays <YEAR>` — print the holiday list for the given year (1900–2100) as JSON
- `--absolute-paths` — emit absolute file paths instead of paths relative to `--dir`. With `-v`/`-vv`/`-vvv`, diagnostic stderr also logs file paths and timestamp content; under `--absolute-paths` these stderr entries carry absolute paths too. Combine with `--quiet` when sharing logs externally.
- `--max-tasks <N>` — task limit (1..=10_000_000, default 10_000). Acts as a global cap on the number of extracted tasks; the same value is reused as a per-file cap so a single hostile file cannot exhaust the global budget on its own. The scan stops as soon as either cap is hit. A separate hard limit of **10 MiB per file** is built in; oversized files are skipped and counted under `files_skipped_size` in the processing summary
- `-v`, `--verbose` — verbose stderr log (`-v` = info, `-vv` = debug, `-vvv` = trace). Mutually exclusive with `--quiet`. The `RUST_LOG` environment variable takes precedence: when set, it overrides `--verbose`/`--quiet` entirely (e.g. `RUST_LOG=error` mutes `-vv`)
- `-q`, `--quiet` — suppress all diagnostic messages except critical errors
- `--color <MODE>` — control ANSI colour in logs: `auto` (default), `always`, `never`
- `--no-color` — disable ANSI colour in logs; equivalent to `--color never`. The `NO_COLOR` environment variable has the same effect (see [no-color.org](https://no-color.org))

In `--color auto` mode the following env vars are honoured (precedence from highest to lowest, after CLI flags):

| Variable          | Effect                                                                                                                                |
|-------------------|---------------------------------------------------------------------------------------------------------------------------------------|
| `NO_COLOR`        | Any value (incl. empty) disables colour. Wins over `CLICOLOR_FORCE`. See [no-color.org](https://no-color.org).                        |
| `CLICOLOR_FORCE`  | Non-zero, non-empty value enables colour even when stderr is not a TTY. See [bixense CLI colours](https://bixense.com/clicolors/).    |
| `CLICOLOR`        | Exactly `0` disables colour. Other values leave the TTY-based default in place.                                                       |

CLI flags `--color always`, `--color never`, and `--no-color` override any of the above.

### Examples

Extract tasks from the current directory as JSON:
```bash
markdown-org-extract
```

Extract tasks from a specific directory:
```bash
markdown-org-extract --dir ./notes
```

Save the result to an HTML file:
```bash
markdown-org-extract --dir ./notes --format html --output agenda.html
```

Emit markdown:
```bash
markdown-org-extract --dir ./notes --format md
```

Run against the bundled examples:
```bash
markdown-org-extract --dir ./examples
markdown-org-extract --dir ./examples --format md
markdown-org-extract --dir ./examples --format html --output examples-agenda.html
```

Use only Russian weekday names:
```bash
markdown-org-extract --dir ./notes --locale ru
```

Use only English weekday names:
```bash
markdown-org-extract --dir ./notes --locale en
```

#### Agenda examples

Today's tasks (default):
```bash
markdown-org-extract --dir ./notes
```

Tasks for a specific date:
```bash
markdown-org-extract --dir ./notes --agenda day --date 2025-12-10
```

Retrieve the holiday list for a year:
```bash
markdown-org-extract --holidays 2025
markdown-org-extract --holidays 2026
```

Sample holiday output:
```json
[
  "2025-01-01",
  "2025-01-02",
  "2025-01-03",
  "2025-01-04",
  "2025-01-05",
  "2025-01-06",
  "2025-01-07",
  "2025-01-08",
  "2025-02-23",
  "2025-03-08",
  "2025-05-01",
  "2025-05-09",
  "2025-06-12",
  "2025-11-04"
]
```

Tasks for the current week:
```bash
markdown-org-extract --dir ./notes --agenda week
```

Tasks for the current month:
```bash
markdown-org-extract --dir ./notes --agenda month
```

Tasks across a date range:
```bash
markdown-org-extract --dir ./notes --agenda week --from 2025-12-01 --to 2025-12-07
markdown-org-extract --dir ./notes --agenda month --from 2025-12-01 --to 2025-12-31
```

All TODO tasks sorted by priority:
```bash
markdown-org-extract --dir ./notes --tasks
```

Use a different timezone:
```bash
markdown-org-extract --dir ./notes --tz UTC
markdown-org-extract --dir ./notes --tz America/New_York
```

Use an explicit current date (useful for tests and deterministic output):
```bash
markdown-org-extract --dir ./notes --agenda week --current-date 2024-12-05
```

Cap the number of extracted tasks (useful for batch processing of very large trees):
```bash
markdown-org-extract --dir ./notes --max-tasks 1000
```

Enable verbose processing logs on stderr:
```bash
markdown-org-extract --dir ./notes -v
```

### Exit codes

The CLI maps error categories to distinct exit codes (sysexits-style) so
shell pipelines can branch on the cause:

| Code  | Category                                                                 | Examples                                                                                              |
|-------|--------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------|
| `0`   | success                                                                  | normal run, `--holidays`, `--completions`                                                             |
| `2`   | usage / input-validation                                                 | invalid `--dir`, `--glob`, `--date`, `--tz`, `--output` parent, `--locale ru,xx`, `from > to`         |
| `70`  | internal software error (`EX_SOFTWARE`)                                  | a regex we built ourselves did not compile, or our own serializer failed                              |
| `74`  | IO failure (`EX_IOERR`)                                                  | unreadable input file, walker error, write failure on `--output`                                      |

`AppError::Io` embeds the failing path or stream sentinel (`<stdout>`)
in its `Display`, so an IO error reads
`error: io: /tmp/out.json: Permission denied (os error 13)` instead of
just the bare OS message.

## Example files

The `examples/` directory contains markdown files with various markers.
The integration tests in `tests/cli.rs` exercise the same files.

General scenarios:

- `project-tasks.md` — project development tasks
- `personal-notes.md` — personal notes and tasks
- `meeting-notes.md` — meeting notes
- `work-log.md` — mixed log with SCHEDULED, DEADLINE, and CLOCK entries

Org-mode marker demonstrations:

- `priorities.md` — tasks with priorities `[#A]`, `[#B]`, `[#C]`
- `org-mode-timestamps.md` — timestamp forms, ranges, and repeaters
- `created-test.md` — using `CREATED:` for the creation date
- `workdays-test.md` — workday repeaters (`+1wd`, `+2wd`) interacting
  with the holiday calendar

CLOCK-block demonstrations (time tracking):

- `clock-formats.md` — every supported CLOCK line form
- `clock-inline.md` — CLOCK inside inline code (`` `CLOCK: ...` ``)
- `clock-test.md` — closed CLOCK intervals with `=> HH:MM`
- `simple-clock.md` — CLOCK inside fenced code blocks
- `done-clock.md` — CLOCK attached to a DONE task (post-completion accounting)

Try running:
```bash
./target/release/markdown-org-extract --dir ./examples --format md
```

## Agenda modes

The utility supports four task-listing modes, mirroring Emacs Org-mode:

### day — tasks for a single day

Shows tasks whose timestamps (SCHEDULED, DEADLINE) fall on the given date.
The default is today in the configured timezone.

```bash
# Today's tasks
markdown-org-extract --agenda day

# Tasks for a specific date
markdown-org-extract --agenda day --date 2025-12-10
```

### week — tasks for a week

Shows tasks whose timestamps fall within a date range. The default is the
current week (Monday–Sunday).

Each day lists:
- Tasks scheduled for that day (scheduled)
- Upcoming tasks relative to that day (upcoming)
- Overdue tasks (overdue) — only for the current date

```bash
# Current week
markdown-org-extract --agenda week

# Explicit range
markdown-org-extract --agenda week --from 2025-12-01 --to 2025-12-07
```

### month — tasks for a month

Shows tasks whose timestamps fall within a date range. The default is the
current month (first to last day).

Behaves the same way as `week` — each day surfaces scheduled, upcoming,
and overdue tasks.

```bash
# Current month
markdown-org-extract --agenda month

# Explicit range
markdown-org-extract --agenda month --from 2025-12-01 --to 2025-12-31
```

### tasks — all TODO tasks

Lists every task whose state is TODO, sorted by priority
(A → B → C → no priority). Timestamps are ignored.

```bash
# All TODO tasks by priority
markdown-org-extract --tasks
```

### Timezones

The `--tz` option controls which timezone is used to derive the current
date and current week. All standard IANA timezones are accepted.

```bash
# Moscow time (default)
markdown-org-extract --agenda day --tz Europe/Moscow

# UTC
markdown-org-extract --agenda day --tz UTC

# New York
markdown-org-extract --agenda day --tz America/New_York
```

## Supported markers

### Task markers

The utility recognises TODO and DONE markers in headings:

```markdown
### TODO Implement feature
### DONE Complete task
```

### Task priorities

Priorities follow the org-mode convention (letters A–Z inside square brackets):

```markdown
### TODO [#A] Critical task
### TODO [#B] Important task
### TODO [#C] Regular task
### DONE [#A] Completed high-priority task
```

The priority appears after the TODO/DONE marker and before the task text.
The most common priorities are:
- `[#A]` — high priority (critical tasks)
- `[#B]` — medium priority (important tasks)
- `[#C]` — low priority (regular tasks)

Priority is optional.

### Timestamps

Timestamps must be wrapped in backticks:

**Simple timestamp:**
```markdown
`<2024-12-10 Mon 10:00-12:00>`
```

**Planning markers:**
```markdown
`CREATED: <2024-12-01 Mon>`
`DEADLINE: <2024-12-15 Sun>`
`SCHEDULED: <2024-12-05 Wed>`
`CLOSED: <2024-12-01 Mon>`
```

**Date range:**
```markdown
`<2024-12-20 Mon>--<2024-12-22 Wed>`
```

The dash separator follows Emacs' `org-tr-regexp` and accepts one,
two, or three dashes (`-`, `--`, `---`). The canonical form on
output is two dashes.

**Limitation:** the start date and start / end times of a range are
surfaced in the output, but the end **date** is not. A range task
is therefore shown on its start day only, not on every day spanned
by the range. See
[ADR-0002](docs/adr/0002-supported-org-mode-subset.md) for the
documented scope and
[ADR-0009](docs/adr/0009-unified-date-window-semantics.md) for the
agenda window model.

**Inactive timestamps (NOT extracted):**
```markdown
`[2024-12-10 Mon]` — square brackets denote an inactive timestamp
```

**Note:** `CREATED` is extracted separately from the other timestamps and
stored in the `created` field. This lets consumers track the task
creation date independently of SCHEDULED, DEADLINE, and CLOSED.

**Warning-period cookie on DEADLINE:**

A DEADLINE can carry a `-N<unit>` cookie that overrides the global
14-day upcoming-window for that one task. Units `h/d/w/m/y` are
recognised; values are converted to whole days using upstream
`org-get-wdays`'s factors (`d=1`, `w=7`, `m=30.4`, `y=365.25`,
`h=1/24`, floored).

```markdown
`DEADLINE: <2025-12-10 Wed -3d>`   — show only 3 days before
`DEADLINE: <2025-12-20 Sat -30d>`  — start warning 30 days out
`DEADLINE: <2025-12-10 Wed +1y -3d>` — repeater + cookie together
`DEADLINE: <2025-12-10 Wed -3d +1y>` — order does not matter
```

Without a cookie the task uses the default 14-day window.

### Time tracking (CLOCK)

The utility supports CLOCK entries for tracking time spent on tasks,
mirroring Emacs Org-mode.

**CLOCK format inside backticks (same as timestamps):**
```markdown
### TODO Implement feature

`SCHEDULED: <2024-12-10 Tue>`
`CLOCK: <2024-12-09 Mon 10:00>--<2024-12-09 Mon 12:30> => 2:30`
`CLOCK: <2024-12-09 Mon 14:00>--<2024-12-09 Mon 16:15> => 2:15`
```

**Alternative format inside code blocks (as in org-mode):**
```markdown
### TODO Implement feature

`SCHEDULED: <2024-12-10 Tue>`

```
CLOCK: [2024-12-09 Mon 10:00]--[2024-12-09 Mon 12:30] =>  2:30
CLOCK: [2024-12-09 Mon 14:00]--[2024-12-09 Mon 16:15] =>  2:15
```
```

**Open CLOCK entry (active work):**
```markdown
`CLOCK: <2024-12-10 Tue 09:00>`
```

**Features:**
- Automatic extraction of every CLOCK entry under a heading
- Total time (`total_clock_time`) summed across all entries
- Open (active) CLOCK entries without a close time
- Rendering in JSON, Markdown, and HTML
- Both square `[...]` (org-mode style) and angle `<...>` brackets are accepted

**Sample JSON output:**
```json
{
  "heading": "Implement feature",
  "clocks": [
    {
      "start": "2024-12-09 Mon 10:00",
      "end": "2024-12-09 Mon 12:30",
      "duration": "2:30"
    },
    {
      "start": "2024-12-09 Mon 14:00",
      "end": "2024-12-09 Mon 16:15",
      "duration": "2:15"
    }
  ],
  "total_clock_time": "4:45"
}
```

**Sample Markdown output:**
```markdown
## Implement feature
**Total Time:** 4:45

**Clock:**
- 2024-12-09 Mon 10:00 → 2024-12-09 Mon 12:30 (2:30)
- 2024-12-09 Mon 14:00 → 2024-12-09 Mon 16:15 (2:15)
```

## Locale support

The utility recognises weekday names in different languages via the
`--locale` option.

### Supported locales

- `en` — English (Mon, Tue, Wed, Thu, Fri, Sat, Sun, Monday, Tuesday, ...)
- `ru` — Russian (Пн, Вт, Ср, Чт, Пт, Сб, Вс, Понедельник, Вторник, ...)

The default is both locales: `--locale ru,en`.

An unknown entry (e.g. `--locale ru,fr`) is rejected at CLI parse time
with exit code `2` — `--quiet` does not mask it. Empty segments are
tolerated, so `--locale ru,` and `--locale ,en` parse the same as
`--locale ru` and `--locale en` respectively.

### Russian-weekday examples

```markdown
### TODO Встреча
`<2024-12-10 Пн 10:00>`

### Конференция
`<2024-12-20 Понедельник>--<2024-12-22 Среда>`

### TODO Задача
`DEADLINE: <2024-12-15 Вс>`
```

Russian weekday names are normalised to the English form during extraction.

## Output format

The output format depends on the agenda mode.

### `--tasks` mode (task list)

#### JSON

Optional fields (`priority`, `created`, `timestamp_time`,
`timestamp_end_time`, `clocks`, `total_clock_time`, `task_type`) are
omitted when absent rather than serialised as `null`. This matches the
`#[serde(skip_serializing_if = "Option::is_none")]` convention used in
`src/types.rs`.

Example below is the actual output of
`--dir examples --glob 'project-tasks.md' --tasks --max-tasks 1
--current-date 2025-12-05`.

```json
[
  {
    "file": "project-tasks.md",
    "line": 5,
    "heading": "Design database schema",
    "content": "Need to finalize the database structure before implementation.",
    "task_type": "TODO",
    "priority": "A",
    "timestamp": "SCHEDULED: <2024-12-05 Wed>",
    "timestamp_type": "SCHEDULED",
    "timestamp_date": "2024-12-05"
  }
]
```

#### Markdown

```markdown
# Tasks

## Design database schema
**File:** `project-tasks.md:5`
**Type:** TODO
**Priority:** A
**Time:** `SCHEDULED: <2024-12-05 Wed>`

Need to finalize the database structure before implementation.
```

### `--agenda day` and `--agenda week` modes (day-grouped agenda)

In these modes tasks are grouped by day. Each day contains task
categories (in display order):

1. **Overdue** (only for the current date) — overdue tasks, oldest first
2. **Scheduled (with time)** — that day's tasks with a time, earliest first
3. **Scheduled (no time)** — that day's tasks without a time
4. **Upcoming** — upcoming tasks relative to that day, nearest first

**Important:** Each day shows upcoming tasks relative to that day, not
relative to a global reference date.

#### JSON

File paths are emitted relative to `--dir` (or absolute when
`--absolute-paths` is set). Optional fields are omitted when absent, as
in `--tasks` mode.

```json
[
  {
    "date": "2025-12-05",
    "overdue": [
      {
        "file": "project-tasks.md",
        "line": 5,
        "heading": "Design database schema",
        "content": "Need to finalize the database structure before implementation.",
        "task_type": "TODO",
        "priority": "A",
        "timestamp": "SCHEDULED: <2024-12-05 Wed>",
        "timestamp_type": "SCHEDULED",
        "timestamp_date": "2024-12-05",
        "days_offset": -365
      }
    ],
    "scheduled_timed": [],
    "scheduled_no_time": [],
    "upcoming": [
      {
        "file": "project-tasks.md",
        "line": 47,
        "heading": "Review pull request #42",
        "content": "Critical bug fix needs review.",
        "task_type": "TODO",
        "timestamp": "DEADLINE: <2025-12-06 Sat>",
        "timestamp_type": "DEADLINE",
        "timestamp_date": "2025-12-06",
        "days_offset": 1
      }
    ]
  }
]
```

The `days_offset` field encodes:
- Positive number — days until the deadline (upcoming)
- Negative number — days the task is overdue
- Absent for tasks belonging to the day itself (scheduled)

#### Markdown

File paths and timestamps are wrapped in inline code (`` `...` ``) to
preserve formatting. `Type:` uses `TODO` / `DONE` (not `Todo` / `Done`);
`Priority:` is shown as a bare letter without the `[#]` wrapper.

```markdown
# Agenda

## 2025-12-05

### Overdue

#### Design database schema (365 days ago)
**File:** `project-tasks.md:5`
**Type:** TODO
**Priority:** A
**Time:** `SCHEDULED: <2024-12-05 Wed>`

Need to finalize the database structure before implementation.

### Scheduled

#### Daily standup
**File:** `project-tasks.md:33`
**Time:** `<2025-12-05 Friday 09:00-09:15>`

Daily standup meeting.

### Upcoming

#### Review pull request \#42 (in 1 days)
**File:** `project-tasks.md:47`
**Type:** TODO
**Time:** `DEADLINE: <2025-12-06 Sat>`

Critical bug fix needs review.
```

#### Parsed timestamp fields

To let downstream consumers render agendas without re-parsing the
`timestamp` string, the timestamp is split into structured fields:

- `timestamp_type` — `SCHEDULED`, `DEADLINE`, `CLOSED`, or `PLAIN`
- `timestamp_date` — date as `YYYY-MM-DD`
- `timestamp_time` — start time, e.g. `10:00` (when present)
- `timestamp_end_time` — end time, e.g. `12:00` (when a range was given)

## Repeating tasks

The utility honours org-mode repeater syntax for automatically scheduling
follow-up occurrences.

### Repeater kinds

Every standard org-mode unit is supported:

- `+Nh` — every N hours
- `+Nd` — every N days (strict; preserves the original date offset)
- `+Nw` — every N weeks
- `+Nm` — every N months
- `+Ny` — every N years
- `+Nwd` — **every N working days** (project extension; honours RF
  holidays and weekends)

Repeater modifiers:
- `+` — strict (cumulative); preserves the date offset
- `++` — catch-up (smart); preserves the weekday
- `.+` — restart-from-completion (relative to the close date)

### Working days

Repeaters with the `wd` (workday) suffix take into account:
- Regular weekends (Saturday, Sunday)
- Official RF holidays
- Holiday shifts

Holiday data lives in `holidays_ru.json`. At build time (`build.rs`) the
data is compiled into static Rust constants — the JSON is parsed once
during compilation rather than at runtime.

### Examples

```markdown
### TODO Hourly check
`SCHEDULED: <2025-12-05 Thu 10:00 +1h>`

### TODO Daily task
`SCHEDULED: <2025-12-05 Thu +1d>`

### TODO Weekly meeting
`SCHEDULED: <2025-12-05 Thu +1w>`

### TODO Monthly report
`SCHEDULED: <2025-12-05 Thu +1m>`

### TODO Annual review
`SCHEDULED: <2025-12-05 Thu +1y>`

### TODO Workday-only task
`SCHEDULED: <2025-12-05 Thu +1wd>`

### TODO Every two working days
`SCHEDULED: <2025-12-05 Thu +2wd>`
```

## Project layout

```
markdown-org-extract/
├── src/
│   ├── main.rs             # CLI entry point, file walker, file I/O
│   ├── cli.rs              # Argument parsing (clap), tracing init
│   ├── agenda.rs           # Agenda logic (day/week/month), repeaters
│   ├── parser.rs           # Task extraction from the markdown AST
│   ├── render.rs           # Markdown/HTML rendering
│   ├── format.rs           # OutputFormat (clap ValueEnum)
│   ├── error.rs            # AppError
│   ├── types.rs            # Task / Priority / DayAgenda / ProcessingStats
│   ├── clock.rs            # CLOCK parsing and time aggregation
│   ├── holidays.rs         # RF workday calendar (singleton, binary search)
│   ├── regex_limits.rs     # `compile_bounded`: regex with size/DFA caps
│   └── timestamp/          # Org-mode timestamp parsing
│       ├── parser.rs       #   <2024-12-05 Thu 10:00 +1d> → ParsedTimestamp
│       ├── extract.rs      #   pull timestamp/CREATED out of arbitrary text
│       ├── repeater.rs     #   parsing and arithmetic of repeaters (+1d, ++2w, .+1wd…)
│       └── weekdays.rs     #   normalisation of Russian weekday names
├── tests/
│   └── cli.rs              # CLI integration tests (assert_cmd)
├── examples/               # Sample markdown files
├── docs/                   # Supplementary documentation
├── holidays_ru.json        # RF holiday / workday calendar
├── build.rs                # Generates holidays_data.rs at build time
├── rustfmt.toml            # Formatter settings (edition 2021, width 100)
├── rust-toolchain.toml     # Pinned channel = stable, components rustfmt+clippy
├── .github/workflows/
│   ├── ci.yml              # PR/push CI: lint + test matrix (Linux/macOS/Windows) + cargo audit
│   ├── release.yml         # Publish to crates.io on tag v* (+ workflow_dispatch)
│   └── outdated.yml        # Weekly non-blocking `cargo outdated`
├── Cargo.toml
├── CHANGELOG.md
├── TODO.md                 # Deferred technical tasks
├── LICENSE                 # MIT
└── README.md
```

The `Cargo.toml` `exclude` list omits `docs/`, `.github/`, `scripts/`,
`TODO.md`, and `CHANGELOG.md` from the published crate tarball on
crates.io — these files matter for repository contributors but not for
downstream `cargo install` users. The GitHub Release archives keep
the binary, README, and LICENSE only (see "For downstream packagers"
above).

See also:
- [docs/CLOCK_IMPLEMENTATION.md](docs/CLOCK_IMPLEMENTATION.md) — CLOCK marker implementation details
- [docs/org-mode-keywords.md](docs/org-mode-keywords.md) — supported-keyword reference
- [CHANGELOG.md](CHANGELOG.md) — version history
- [TODO.md](TODO.md) — deferred technical tasks

## Dependencies

- `clap` — command-line argument parsing
- `comrak` — markdown parsing (without onig/syntect: `default-features = false`)
- `regex` — regular expressions (with size/DFA caps)
- `serde` / `serde_json` — data serialisation
- `chrono` / `chrono-tz` — dates and timezones
- `grep-regex` / `grep-searcher` — fast pre-filter over keywords
- `ignore` — directory tree walk that honours `.gitignore`
- `globset` — glob compilation for `--glob`
- `tracing` / `tracing-subscriber` — structured diagnostic logging (`--verbose`, `--quiet`, `--color`, `--no-color`)

Lazily initialised `static` regular expressions use `std::sync::LazyLock`
from the standard library (Rust 1.80+; the project itself requires 1.85).

## License

MIT — see the [LICENSE](LICENSE) file.

### Provenance of `holidays_ru.json`

The holiday calendar and weekend-shift table in `holidays_ru.json` was
compiled by the project author from the official RF government decrees
on weekend rescheduling. This is public factual information and is not
subject to copyright. For packaging convenience the file is distributed
under the same MIT licence as the rest of the code.

Attribution and a schema description are duplicated inside the file
itself under the `_meta` key (`build.rs` ignores underscore-prefixed
top-level keys, so the block has no effect on the compiled output).
