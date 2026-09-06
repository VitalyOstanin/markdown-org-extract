# markdown-org-extract

[![crates.io](https://img.shields.io/crates/v/markdown-org-extract.svg)](https://crates.io/crates/markdown-org-extract)
[![CI](https://github.com/VitalyOstanin/markdown-org-extract/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/VitalyOstanin/markdown-org-extract/actions/workflows/ci.yml?query=branch%3Amaster)
[![license](https://img.shields.io/crates/l/markdown-org-extract.svg)](https://github.com/VitalyOstanin/markdown-org-extract/blob/master/LICENSE)

Extracts tasks from markdown files with support for Emacs Org-mode markers.
Ships as a command-line tool and as a Rust library — both run the same code.

It is the core of a three-part ecosystem, all reading the same files:

| Project                                                                         | What it is                                                           |
|---------------------------------------------------------------------------------|----------------------------------------------------------------------|
| `markdown-org-extract` (this one)                                               | the CLI and the Rust library both clients read their tasks through   |
| [`markdown-org-vscode`](https://github.com/VitalyOstanin/markdown-org-vscode)   | the VS Code extension: agenda panel, editing commands, time tracking |
| [`markdown-org-android`](https://github.com/VitalyOstanin/markdown-org-android) | the Android client, syncing the same notes over git                  |

The extension runs this binary as a subprocess; the Android client links the
same code in-process through UniFFI. That is what keeps the three in agreement
about what a file means.

## What the name says, and what the crate does

The name records what the crate started as: a reader that extracted tasks from
markdown and printed them. Reading is still the entry point, but it is no
longer the whole of it. What is here now:

| № | Part                | What it does                                                                                              |
|---|---------------------|-----------------------------------------------------------------------------------------------------------|
| 1 | Reading             | walks a directory — or several at once — and returns the tasks it finds, with the agenda already bucketed  |
| 2 | Writing             | sets and clears the keyword and the priority cookie, moves a planning date, and completes a repeating task by advancing its repeater |
| 3 | Bulk writing        | applies one action to a whole group in a single pass per file, and takes it back from a snapshot            |
| 4 | Version control     | commits what it wrote, clones, fetches, fast-forwards and pushes over `https://` and `ssh://`               |

So a caller does not read here and write elsewhere: everything that touches a
note is in this crate, and a client is the screen in front of it. The name is
kept because the crate is published under it — renaming on crates.io means a
second crate rather than a new name for this one.

## Table of contents

- [What the name says, and what the crate does](#what-the-name-says-and-what-the-crate-does)
- [Installation and build](#installation-and-build)
- [Use as a library](#use-as-a-library)
- [For downstream packagers](#for-downstream-packagers)
- [Usage](#usage)
- [Example files](#example-files)
- [Agenda modes](#agenda-modes)
- [Supported markers](#supported-markers)
- [Locale support](#locale-support)
- [Output format](#output-format)
- [Repeating tasks](#repeating-tasks)
- [Parsing a phrase into an entry](#parsing-a-phrase-into-an-entry)
- [Project layout](#project-layout)
- [Dependencies](#dependencies)
- [License](#license)

## Installation and build

### Requirements

- Rust 1.85 or newer. The bundled `comrak` (0.52 at the time of writing)
  ships on Rust edition 2024 and therefore requires a 1.85+ toolchain,
  as every release since 0.50 has; this crate itself is
  still on edition 2021 (see [`TODO.md`](TODO.md#switch-to-edition-2024)
  for the planned migration).
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

> The rest of this section — building, running from a checkout, and
> testing — is for contributors and people building from source. If you
> installed the binary with `cargo install` (above), skip to
> [Usage](#usage); you do not need to clone the repository.

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

Run the full CI-equivalent locally before pushing or opening a PR:
```bash
scripts/check.sh
```

`scripts/check.sh` chains `cargo fmt --check`,
`yamllint .github/workflows/`, `cargo clippy --all-targets -D warnings`,
`cargo doc --no-deps -D warnings`, and `cargo test`. It is the single
command that mirrors the CI configuration; running `cargo test` alone
will not catch `rustfmt` or `yamllint` regressions that block CI on a
subsequent push.

#### Properties

`tests/properties.rs` states what has to hold for *every* input of the parsers
that read a value someone else wrote — the `EXDATE` and `RECURRENCE_ID` of a
repeating series (see [One occurrence that
differs](#one-occurrence-that-differs)). They run inside the ordinary `cargo
test`; `proptest` is a dev-dependency and CI needs nothing added.

A failing case is written to `tests/properties.proptest-regressions` and re-run
before any new case on the next run. **That file is committed**: it is the
shortest input that showed the bug, and leaving it out of the repository would
mean the next person has to find it again. A file that appears after a
deliberately broken build — a mutation run to check that a property has teeth —
is not a finding and is deleted with the mutation.

#### Workday handling

Workday-aware scheduling is exercised by tests across three modules:
`holidays` (the RU calendar, weekend/holiday classification, and the
next-working-day walk), `timestamp::repeater` (the `+Nwd` / `++Nwd` /
`.+Nwd` repeater grammar and holiday-skipping occurrence arithmetic),
and `timestamp::parser` (timestamps that carry a workday repeater).
For the authoritative, always-current list of cases, read the
`#[test]` functions in those modules (`cargo test -- --list` prints
the names).

## Use as a library

The crate is also a library, so a Rust consumer can extract tasks
in-process instead of spawning the binary and parsing its JSON. This is
required where processes cannot be spawned at all, such as Android.

```toml
[dependencies]
markdown-org-extract = "0.23"
```

Scanning and agenda building are separate steps, so one scan can feed
several agendas:

```rust
use markdown_org_extract::{filter_agenda, scan_directory, AgendaDates, AgendaScope, ScanOptions};

let outcome = scan_directory("notes".as_ref(), &ScanOptions::default(), None)?;
println!("{} tasks in {} files", outcome.tasks.len(), outcome.stats.files_processed);

let agenda = filter_agenda(
    outcome.tasks,
    AgendaScope::Week,
    AgendaDates::default(),
    "Europe/Moscow",
    false, // include_done
    false, // include_cancelled
    true,  // annotate_next
)?;
```

`ScanOptions` carries what the walk needs (`glob`, `max_tasks`,
`absolute_paths`, `locale`) and has a `Default`. The third argument to
`scan_directory` is an optional `&AtomicBool` the caller can set to stop
a running scan; pass `None` when there is nothing to interrupt it.

Notes kept in more than one place are one agenda rather than one agenda
each. `scan_directories` walks several roots as a single run — one task
list, one `ProcessingStats`, and `max_tasks` as the budget over all of
them — and every task carries `root`, the canonical path its `file` is
relative to:

```rust
use std::path::PathBuf;
use markdown_org_extract::{scan_directories, ScanOptions};

let roots = vec![PathBuf::from("work-notes"), PathBuf::from("personal-notes")];
let outcome = scan_directories(&roots, &ScanOptions::default(), None)?;

for task in &outcome.tasks {
    // `file` is relative to `root`, and the same relative path can occur
    // in two collections, so the two are joined before opening a note.
    let path = PathBuf::from(task.root.as_deref().unwrap_or(".")).join(&task.file);
    println!("{} — {}", path.display(), task.heading);
}
```

A root named twice is walked once; a root that is missing fails the call
rather than reading as an empty collection. See
[ADR-0026](docs/adr/0026-several-roots-in-one-scan.md).

Nothing here reads the wall clock unless it has to:
`AgendaDates::current_date` sets what "today" means, so the same input
renders the same agenda on any day. See
[ADR-0025](docs/adr/0025-library-crate-with-thin-cli.md) for the split
between the library and the CLI, and the
[API documentation](https://docs.rs/markdown-org-extract) for the rest of
the surface.

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
| `x86_64-apple-darwin`        | `.tar.gz`         | `markdown-org-extract`     |
| `aarch64-apple-darwin`       | `.tar.gz`         | `markdown-org-extract`     |
| `x86_64-pc-windows-msvc`     | `.zip`            | `markdown-org-extract.exe` |

The archive filename template is:

```
markdown-org-extract-<version>-<target>.<ext>
```

Example asset set for tag `v0.4.1`:

```
markdown-org-extract-0.4.1-x86_64-unknown-linux-gnu.tar.gz
markdown-org-extract-0.4.1-x86_64-apple-darwin.tar.gz
markdown-org-extract-0.4.1-aarch64-apple-darwin.tar.gz
markdown-org-extract-0.4.1-x86_64-pc-windows-msvc.zip
```

`<version>` is the tag stripped of its leading `v`, identical to
the `[package].version` field in `Cargo.toml` for that commit
(the `verify` job in `.github/workflows/release.yml` fails the
release if the two diverge).

### Archive layout

Each archive extracts to a single top-level directory whose name
matches the archive stem:

```
markdown-org-extract-<version>-<target>/
├── markdown-org-extract       # markdown-org-extract.exe on Windows
├── README.md
├── LICENSE
└── THIRD-PARTY-LICENSES.txt
```

No nested target subdirectories, no separate debug symbols, no
manpages. Adding a file to the staged directory is a contract
change (CHANGELOG entry + ADR).

`LICENSE` covers this project's own code. The binary is statically
linked, so the licence texts and copyright notices of every crate
linked into it travel with it in `THIRD-PARTY-LICENSES.txt` —
generated from the dependency graph, not maintained by hand, and
verified fresh in CI. See
[ADR-0024](docs/adr/0024-third-party-license-notices-in-archives.md).

### Checksums

Every archive ships with a sibling `.sha256` file in the standard
`sha256sum` format (`<hex>  <filename>`):

```
markdown-org-extract-<version>-<target>.<ext>
markdown-org-extract-<version>-<target>.<ext>.sha256
```

Verification with the GNU tool:

```bash
sha256sum -c markdown-org-extract-0.4.1-x86_64-unknown-linux-gnu.tar.gz.sha256
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
- Additional targets (`aarch64-unknown-linux-gnu`, musl variants,
  `aarch64-pc-windows-msvc`) may be added in a future minor
  release. Removal of a previously published target requires a
  major-version bump.

## Usage

```bash
markdown-org-extract [OPTIONS]
```

### Options

- `--dir <DIR>` — directory to scan (default: `.`). Repeat the flag to scan several collections as one run: the tasks are merged in the order the roots are given, `--max-tasks` is the budget for all of them together, and every task then carries a `root` field naming the directory its `file` is relative to. A directory named twice is scanned once; a directory that is missing fails the run rather than reading as empty. A single `--dir` emits the output it always did, without the `root` field
- `--glob <GLOB>` — file filter pattern (default: `*.md`)
- `--format <FORMAT>` — output format: `json`, `md`, `html` (default: `json`)
- `--output <OUTPUT>` — file to write the result to; `-` means stdout (default: stdout)
- `--locale <LOCALE>` — weekday locales, comma-separated (default: `ru,en`)
- `--agenda <MODE>` — agenda mode: `day`, `week`, `month`, `month-grid`, `tasks` (default: `day`). `month` is the calendar month; `month-grid` is the whole weeks that month falls in, so the first and last rows carry the days borrowed from the months beside it — the window a calendar is drawn on. See [ADR-0028](docs/adr/0028-week-start-and-the-month-grid.md)
- `--tasks` — show all TODO tasks sorted by priority, then by date and time (alias for `--agenda tasks`)
- `--tasks-include-done` — also include DONE tasks in the flat `--tasks` / `--agenda tasks` list (default: TODO only). No effect in `day`/`week`/`month` mode
- `--tasks-include-cancelled` — also include cancelled tasks (either spelling, `CANCELLED` or `CANCELED`) in the flat `--tasks` / `--agenda tasks` list (default: TODO only). Independent of `--tasks-include-done`. No effect in `day`/`week`/`month` mode
- `--date <DATE>` — window anchor for `day`/`week`/`month`/`month-grid` mode in `YYYY-MM-DD`. In `day` mode the window is exactly this date; in the others it is the week / month / grid containing this date. Overridden by `--from`/`--to`. Not allowed in `tasks` mode. Default: `--current-date` (or today)
- `--from <DATE>` — window start (`YYYY-MM-DD`) for `day`/`week`/`month`/`month-grid` mode. Together with `--to`, an explicit range that overrides `--date`. If `--to` is omitted, the window ends at `--current-date` (or today). In `month-grid` mode the range is grown outward to whole weeks. Not allowed in `tasks` mode
- `--to <DATE>` — window end (`YYYY-MM-DD`) for `day`/`week`/`month`/`month-grid` mode. Together with `--from`, an explicit range that overrides `--date`. If `--from` is omitted, the window starts at `--current-date` (or today). In `month-grid` mode the range is grown outward to whole weeks. Not allowed in `tasks` mode
- `--tz <TIMEZONE>` — IANA timezone for determining the current date (default: `Europe/Moscow`)
- `--current-date <DATE>` — override of "today" (`YYYY-MM-DD`). Used as the reference for overdue / upcoming markers and as the default for a missing `--from`/`--to` edge. Not allowed in `tasks` mode. Default: today in `--tz`
- `--week-start <DAY>` — which weekday a week begins on: a name (`monday` … `sunday`, or the three-letter form `mon` … `sun`), or `today` for a week beginning on the anchor day. Case is ignored, and an unknown value is refused by the parser before any file is read. Default: `monday` — a fixed default rather than one read from the environment, so the same arguments produce the same window on any machine; a client that wants the user's own first day of the week passes it explicitly. This is upstream's `org-agenda-start-on-weekday`, and like it, reaches the week-shaped windows only: `week` mode and the columns of `month-grid`, which refuses `today` because a calendar column is a fixed weekday. Accepted but inert in `day` and `month` mode, which have no week to align (`-vv` logs when it is ignored). Not allowed in `tasks` mode
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

### Environment variables

The CLI reads no configuration files; behaviour is driven by flags and a
small set of environment variables:

- `RUST_LOG` — sets the diagnostic log filter (`tracing` syntax, e.g.
  `RUST_LOG=debug` or `RUST_LOG=markdown_org_extract=trace`). When set,
  it **overrides** `--verbose` / `--quiet` entirely (see
  [ADR-0016](docs/adr/0016-rust-log-cli-precedence.md)).
- `NO_COLOR`, `CLICOLOR_FORCE`, `CLICOLOR` — control ANSI colour in the
  diagnostic log; see the colour-precedence table under
  [Options](#options).

The timezone is **not** taken from the `TZ` environment variable; it is
controlled only by `--tz` (default `Europe/Moscow`). "Today" can be
pinned with `--current-date` for reproducible output.

### File selection

The scan walks `--dir` with the [`ignore`](https://docs.rs/ignore)
crate's standard filters, so it behaves like other Rust tooling
(`ripgrep`, `fd`):

- `.ignore` files (including nested ones deeper in the tree) are always
  honoured; matching files are not scanned.
- `.gitignore`, the global gitignore, and `.git/info/exclude` are
  honoured only when the scanned tree is inside a git repository — the
  `ignore` crate evaluates them relative to the enclosing `.git`. A
  `.gitignore` in a directory with no `.git` has no effect; use
  `.ignore` for VCS-independent rules.
- Hidden files and directories (dot-prefixed) are skipped.
- Symbolic links are not followed, and the walk stays on the starting
  filesystem (it will not cross a mount point).
- Of the files that survive those filters, only those matching `--glob`
  (default `*.md`) are parsed.

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
  "2025-11-04",
  "2025-12-31"
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

The grid a month is drawn on — whole weeks, borrowed days included, and the
same grid read from a Sunday:
```bash
markdown-org-extract --dir ./notes --agenda month-grid
markdown-org-extract --dir ./notes --agenda month-grid --week-start sunday
```

A grid over an explicit window — two months of calendar in one answer. The
window is grown outward to the weeks it touches, so the result is always a
whole number of weeks beginning on `--week-start`, whatever dates bound it
([ADR-0030](docs/adr/0030-explicit-window-in-the-month-grid.md)):
```bash
markdown-org-extract --dir ./notes --agenda month-grid --from 2026-08-01 --to 2026-09-30
```

Tasks across a date range:
```bash
markdown-org-extract --dir ./notes --agenda week --from 2025-12-01 --to 2025-12-07
markdown-org-extract --dir ./notes --agenda month --from 2025-12-01 --to 2025-12-31
```

All TODO tasks sorted by priority, then by date and time:
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

| Code  | Category                                                                 | Examples                                                                                                  |
|-------|--------------------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------|
| `0`   | success                                                                  | normal run, `--holidays`, `--completions`                                                                 |
| `2`   | usage / input-validation                                                 | invalid `--dir`, `--glob`, `--date`, `--tz`, `--output` parent, `--locale ru,xx`, `from > to`             |
| `70`  | internal software error (`EX_SOFTWARE`)                                  | a regex we built ourselves did not compile, or our own serializer failed                                  |
| `74`  | IO failure (`EX_IOERR`)                                                  | unreadable input file, walker error, write failure on `--output`                                          |
| `130` | scan aborted by signal (`128 + SIGINT`)                                  | Ctrl-C during a long scan; SIGTERM on Unix. A partial `processing summary` is logged on stderr at warn.   |

A broken output pipe is **not** an error: when a downstream consumer
closes the read end early (`markdown-org-extract … | head -n 1`), the
write fails with `EPIPE` / `BrokenPipe`. By then the bytes the consumer
kept have already been produced, so the CLI exits `0` silently with no
diagnostic — matching `cat`, `grep`, and `jq` in the same situation. A
broken pipe on `--output` (a real file) is reported normally as an IO
error (`74`); only the stdout pipe is treated as a clean stop.

`AppError::Io` embeds the failing path or stream sentinel (`<stdout>`)
in its `Display`, so an IO error reads
`error: io: /tmp/out.json: Permission denied (os error 13)` instead of
just the bare OS message.

A SIGINT or SIGTERM during the directory walk flips an internal flag
that the scan loop polls between files. On the next iteration the walk
stops, the partial `processing summary` is emitted on stderr (with
`interrupted = true`), `--output` is not written, and the process exits
with code `130`. Sending the signal a second time after the scan has
already finished has no effect; the process is past the polling point.

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
- `series-exceptions.md` — a weekly series with one occurrence cancelled
  (`EXDATE`) and one taken over by an entry of its own (`SERIES_ID` /
  `RECURRENCE_ID`), the two shapes of [One occurrence that
  differs](#one-occurrence-that-differs)

CLOCK-block demonstrations (time tracking):

- `clock-formats.md` — every supported CLOCK line form
- `clock-inline.md` — CLOCK inside inline code (`` `CLOCK: ...` ``)
- `clock-test.md` — closed CLOCK intervals with `=> HH:MM`
- `simple-clock.md` — CLOCK inside fenced code blocks
- `done-clock.md` — CLOCK attached to a DONE task (post-completion accounting)

Try running (after `cargo install`, or substitute `cargo run --release --`
from a checkout):
```bash
markdown-org-extract --dir ./examples --format md
```

## Agenda modes

The utility supports five task-listing modes. Four mirror Emacs Org-mode;
`month-grid` is the one window upstream has no equivalent for, added because
both clients draw a calendar and were each computing it themselves
([ADR-0028](docs/adr/0028-week-start-and-the-month-grid.md)):

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

### month-grid — the grid a month is drawn on

The whole weeks a calendar month falls in: the month itself plus the days its
first and last weeks borrow from the months on either side. This is the window
a calendar page is laid out on, seven days to a row, and `--agenda month` stays
what it was — the days of the month and nothing else.

The rows begin on `--week-start`, so the grid a reader who starts weeks on
Sunday sees is a different set of days than the Monday one. `--week-start
today` is refused here: a calendar column is a fixed weekday, and a week
beginning wherever the reader stands has no columns. The grid is not padded to
six rows — a month whose edges already fall on the edges of a week borrows
nothing.

An explicit `--from`/`--to` window is grown outward to the weeks it touches, so
the answer is a whole number of weeks whatever dates bound it
([ADR-0030](docs/adr/0030-explicit-window-in-the-month-grid.md)).

The output carries no new field: the grid is a longer array of the same day
objects a `month` answer is made of.

```bash
# The grid the current month is drawn on
markdown-org-extract --agenda month-grid

# The same grid read by someone whose week starts on Sunday
markdown-org-extract --agenda month-grid --week-start sunday

# Two months of calendar in one answer
markdown-org-extract --agenda month-grid --from 2026-08-01 --to 2026-09-30
```

### tasks — all TODO tasks

Lists every task whose state is TODO, sorted by priority
(A → B → C → no priority) and, within one priority, by date and then by
time. What has no time to sort by goes last: a task carrying no date
after every dated one, and a whole-day task after the timed ones of its
day, with the file and the line as the tiebreaker. A timestamp never
decides whether a task is listed, only where it sits. Add
`--tasks-include-done` to additionally surface DONE tasks (off by
default), e.g. for a consumer that needs completed tasks to remove a
linked calendar event. Add `--tasks-include-cancelled` to additionally
surface cancelled tasks — either spelling, `CANCELLED` or `CANCELED` —
(off by default, independent of `--tasks-include-done`), e.g. for a
consumer that needs cancelled tasks to remove a linked calendar event.

```bash
# All TODO tasks by priority, then by date and time
markdown-org-extract --tasks

# TODO tasks plus completed (DONE) ones
markdown-org-extract --tasks --tasks-include-done

# TODO tasks plus cancelled ones
markdown-org-extract --tasks --tasks-include-cancelled

# TODO tasks plus both DONE and CANCELLED
markdown-org-extract --tasks --tasks-include-done --tasks-include-cancelled
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

The utility recognises the following task state markers in headings:

- `TODO` — task to be done.
- `DONE` — task completed.
- `CANCELLED` (or the single-L `CANCELED`, as used in upstream
  Org-mode) — task cancelled (must not be done; distinct from `DONE`).
  The spelling you write is preserved in the `task_type` output.

```markdown
### TODO Implement feature
### DONE Complete task
### CANCELLED Abandoned idea
### CANCELED Dropped variant
```

### Task priorities

Priorities follow the org-mode convention (letters A–Z inside square brackets):

```markdown
### TODO [#A] Critical task
### TODO [#B] Important task
### TODO [#C] Regular task
### DONE [#A] Completed high-priority task
```

The priority appears after the task state marker (`TODO`/`DONE`/`CANCELLED`/`CANCELED`) and before the task text.
The most common priorities are:
- `[#A]` — high priority (critical tasks)
- `[#B]` — medium priority (important tasks)
- `[#C]` — low priority (regular tasks)

Priority is optional. A numeric cookie is accepted as well: `[#0]` through
`[#64]`, where a lower number is the higher priority.

A cookie written anywhere else in the heading still sets the priority —
`### TODO Buy [#A] filter` is an `A` — but there it stays part of the task text
and is shown as typed, so nothing the heading says is lost. Only a cookie in the
position above is taken out of the text. Emacs reads a heading the same way: it
finds the cookie through `org-priority-regexp` wherever it sits, and
`org-agenda` prints the line as written. See
[ADR-0027](docs/adr/0027-priority-cookie-read-anywhere-removed-in-place.md).

### Timestamps

Timestamps must be wrapped in backticks:

**Simple timestamp:**
```markdown
`<2024-12-10 Tue 10:00-12:00>`
```

**Planning markers:**
```markdown
`CREATED: [2024-12-01 Sun]`
`DEADLINE: <2024-12-15 Sun>`
`SCHEDULED: <2024-12-05 Thu>`
`CLOSED: [2024-12-01 Sun]`
```

The bracket form is per-keyword (see
[ADR-0014](docs/adr/0014-active-and-inactive-timestamps.md)):
`SCHEDULED:` and `DEADLINE:` carry active `<...>`; `CLOSED:` and
`CREATED:` carry inactive `[...]`.

**Date range:**
```markdown
`<2024-12-20 Fri>--<2024-12-22 Sun>`
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

**Active and inactive timestamps:**

Emacs Org-mode distinguishes two bracket forms — active `<...>`
drives the agenda; inactive `[...]` is descriptive metadata that
never feeds agenda windows. The accepted form is fixed per
context:

| Context        | Active `<...>` | Inactive `[...]` |
| -------------- | -------------- | ---------------- |
| `SCHEDULED:`   | yes            | no               |
| `DEADLINE:`    | yes            | no               |
| `CLOSED:`      | no             | yes              |
| `CREATED:`     | no             | yes              |
| Inline plain   | yes            | yes              |
| `CLOCK:`       | yes            | yes              |

Mixed pairs `<...]` and `[...>` are rejected. Inactive timestamps
never drive day / week / month agenda windows; they are surfaced
in the JSON output via the `timestamp_active` field
(`true` for `<...>`, `false` for `[...]`). See
[ADR-0014](docs/adr/0014-active-and-inactive-timestamps.md) for
the upstream-Emacs sources and the breaking-change migration.

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
`<2024-12-10 Вт 10:00>`

### Конференция
`<2024-12-20 Пятница>--<2024-12-22 Воскресенье>`

### TODO Задача
`DEADLINE: <2024-12-15 Вс>`
```

Russian weekday names are normalised to the English form during extraction.

## Output format

The output format depends on the agenda mode.

### `--tasks` mode (task list)

#### JSON

Optional fields (`priority`, `created`, `timestamp_active`,
`timestamp_time`, `timestamp_end_time`, `timestamp_repeater`, `clocks`,
`total_clock_time`, `properties`, `task_type`) are omitted when absent
rather than serialised as `null`.
`timestamp_repeater` carries the timestamp's org repeater in its
canonical form (`++7d`, `.+1m`, `+1wd`) and is absent when the
timestamp has no repeater; the repeater also remains present verbatim
inside the raw `timestamp` string.
This matches the `#[serde(skip_serializing_if = "Option::is_none")]`
convention used in `src/types.rs`.

Example below is the actual output of
`--dir examples --glob 'project-tasks.md' --tasks --max-tasks 1`. The flat
list has no window to place a task in, so `tasks` mode takes no date argument
at all — `--current-date` alongside `--tasks` is refused with exit code 2.

```json
[
  {
    "file": "project-tasks.md",
    "line": 5,
    "heading": "Design database schema",
    "content": "Need to finalize the database structure before implementation.",
    "task_type": "TODO",
    "priority": "A",
    "timestamp": "SCHEDULED: <2024-12-05 Thu>",
    "timestamp_type": "SCHEDULED",
    "timestamp_active": true,
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
**Time:** `SCHEDULED: <2024-12-05 Thu>`

Need to finalize the database structure before implementation.
```

### `--agenda day` and `--agenda week` modes (day-grouped agenda)

In these modes tasks are grouped by day. Each day contains task
categories (in display order):

1. **Overdue** (only for the current date) — overdue tasks, oldest first
2. **Scheduled (with time)** — that day's tasks with a time, earliest first
3. **Scheduled (no time)** — that day's tasks without a time
4. **Upcoming** — upcoming tasks relative to that day, nearest first

**Overdue is for planning keywords only.** A date that has passed is carried
into today's agenda when it came from `SCHEDULED:` or `DEADLINE:` — upstream
Org-mode forwards such an entry day after day until it is marked done. A plain
timestamp is an event in a calendar, shown on its date and nowhere else, so a
class held every Monday since last autumn shows up on Mondays and is never
reported as a year of arrears. Give a recurring appointment a plain repeating
timestamp (`<2025-09-01 Mon 19:00 +1w>`) and a recurring obligation
`SCHEDULED:` with `++1w`, which moves the date into the future when the task is
done rather than counting every occurrence missed.

**Important:** Each day shows upcoming tasks relative to that day, not
relative to a global reference date.

#### JSON

File paths are emitted relative to `--dir` (or absolute when
`--absolute-paths` is set). Optional fields are omitted when absent, as
in `--tasks` mode.

Example below is the actual output of
`--dir examples --glob 'project-tasks.md' --agenda day --current-date 2024-12-08`.

```json
[
  {
    "date": "2024-12-08",
    "overdue": [
      {
        "file": "project-tasks.md",
        "line": 5,
        "heading": "Design database schema",
        "content": "Need to finalize the database structure before implementation.",
        "task_type": "TODO",
        "priority": "A",
        "timestamp": "SCHEDULED: <2024-12-05 Thu>",
        "timestamp_type": "SCHEDULED",
        "timestamp_active": true,
        "timestamp_date": "2024-12-05",
        "days_offset": -3
      },
      {
        "file": "project-tasks.md",
        "line": 47,
        "heading": "Review pull request #42",
        "content": "Critical bug fix needs review.",
        "task_type": "TODO",
        "timestamp": "DEADLINE: <2024-12-06 Fri>",
        "timestamp_type": "DEADLINE",
        "timestamp_active": true,
        "timestamp_date": "2024-12-06",
        "days_offset": -2
      }
    ],
    "scheduled_timed": [],
    "scheduled_no_time": [
      {
        "file": "project-tasks.md",
        "line": 19,
        "heading": "Implement user authentication",
        "content": "Add JWT-based authentication system.",
        "task_type": "TODO",
        "priority": "A",
        "timestamp": "SCHEDULED: <2024-12-08 Sun>",
        "timestamp_type": "SCHEDULED",
        "timestamp_active": true,
        "timestamp_date": "2024-12-08"
      }
    ],
    "upcoming": [
      {
        "file": "project-tasks.md",
        "line": 9,
        "heading": "Setup CI/CD pipeline",
        "content": "Configure GitHub Actions for automated testing and deployment.",
        "task_type": "TODO",
        "priority": "B",
        "timestamp": "DEADLINE: <2024-12-10 Tue>",
        "timestamp_type": "DEADLINE",
        "timestamp_active": true,
        "timestamp_date": "2024-12-10",
        "days_offset": 2
      }
    ]
  }
]
```

The `days_offset` field encodes:
- Positive number — days until the deadline (upcoming)
- Negative number — days the task is overdue
- Absent for tasks belonging to the day itself (scheduled)

For a task that carries a repeater, an extra `timestamp_next`
(`YYYY-MM-DD`) gives the resolved next still-upcoming occurrence relative
to "now": a date before today rolls forward to the closest occurrence
today-or-later, and a timed occurrence earlier today rolls to the
following one (the reference is the local wall clock; under
`--current-date` the time is treated as midnight, so only the date-level
rolling is deterministic). It is absent for non-repeating tasks and
**only present in the `day`/`week`/`month` agenda modes** — the date-less
`--tasks` mode never carries it. See
[ADR-0023](docs/adr/0023-next-occurrence-field.md).

A repeating task drawn on a day of its own — the `scheduled_timed` and
`scheduled_no_time` buckets — carries a second date,
`timestamp_next_after` (`YYYY-MM-DD`): the first occurrence *after* that
cell's day, whatever "now" is. A `+1d` task read in the cell of the 18th
gives `2026-08-19` there and `2026-08-20` in the cell of the 19th, while
`timestamp_next` stays the same in both. The `overdue` and `upcoming`
buckets do not carry it — their rows are borrowed into the reference day
rather than drawn on their own date, so "next from now" is still the
question there. See
[ADR-0029](docs/adr/0029-next-occurrence-after-the-rendered-day.md).

#### Markdown

File paths and timestamps are wrapped in inline code (`` `...` ``) to
preserve formatting. `Type:` uses `TODO` / `DONE` (not `Todo` / `Done`);
`Priority:` is shown as a bare letter without the `[#]` wrapper.

```markdown
# Agenda

## 2024-12-08

### Overdue

#### Design database schema (3 days ago)
**File:** `project-tasks.md:5`
**Type:** TODO
**Priority:** A
**Time:** `SCHEDULED: <2024-12-05 Thu>`

Need to finalize the database structure before implementation.

#### Review pull request \#42 (2 days ago)
**File:** `project-tasks.md:47`
**Type:** TODO
**Time:** `DEADLINE: <2024-12-06 Fri>`

Critical bug fix needs review.


### Scheduled

#### Implement user authentication
**File:** `project-tasks.md:19`
**Type:** TODO
**Priority:** A
**Time:** `SCHEDULED: <2024-12-08 Sun>`

Add JWT-based authentication system.


### Upcoming

#### Setup CI/CD pipeline (in 2 days)
**File:** `project-tasks.md:9`
**Type:** TODO
**Priority:** B
**Time:** `DEADLINE: <2024-12-10 Tue>`

Configure GitHub Actions for automated testing and deployment.
```

#### Parsed timestamp fields

To let downstream consumers render agendas without re-parsing the
`timestamp` string, the timestamp is split into structured fields:

- `timestamp_type` — `SCHEDULED`, `DEADLINE`, `CLOSED`, or `PLAIN`
- `timestamp_active` — bracket form: `true` for active `<...>`,
  `false` for inactive `[...]`; omitted when no timestamp is
  present (see
  [ADR-0014](docs/adr/0014-active-and-inactive-timestamps.md))
- `timestamp_date` — date as `YYYY-MM-DD`
- `timestamp_time` — start time, e.g. `10:00` (when present)
- `timestamp_end_time` — end time, e.g. `12:00` (when a range was given)
- `timestamp_repeater` — the repeater cookie, e.g. `++7d` (when present)
- `timestamp_next` — resolved next still-upcoming occurrence as
  `YYYY-MM-DD`, for repeating tasks in the `day`/`week`/`month` agenda
  modes only (see [Repeating tasks](#repeating-tasks) and
  [ADR-0023](docs/adr/0023-next-occurrence-field.md))
- `timestamp_next_after` — the occurrence following this cell's own day
  as `YYYY-MM-DD`, for repeating tasks in the scheduled buckets only
  (see [Occurrence after this
  one](#occurrence-after-this-one-timestamp_next_after) and
  [ADR-0029](docs/adr/0029-next-occurrence-after-the-rendered-day.md))

#### Task properties

- `properties` (object, optional): per-task key/value pairs parsed from an
  `org-properties` fenced code block placed under the heading and its
  planning lines. Bare `UPPER_SNAKE: value` lines; absent when a task has
  no such block. See
  [ADR-0020](docs/adr/0020-task-properties-org-properties-block.md).

On disk the block sits under the heading and planning lines:

````markdown
### TODO Ship release
`SCHEDULED: <2026-06-01 Mon 10:00>`
```org-properties
GCAL_EVENT_ID: abc123/primary
```
````

#### Series exceptions

Three of the `org-properties` keys are read out of the block into fields of
their own, because every consumer answering "does this series occur on this
day" needs them parsed (see [One occurrence that
differs](#one-occurrence-that-differs) and
[ADR-0031](docs/adr/0031-exceptions-to-a-repeating-entry.md)):

- `excluded_dates` (array of strings, optional) — the occurrences this entry
  cancels, each as `YYYY-MM-DD`, deduplicated and in the order the file wrote
  them. Read from `EXDATE`.
- `recurrence_id` (string, optional) — the occurrence this entry stands in
  for, as `YYYY-MM-DD` or `YYYY-MM-DD HH:MM`. Seconds are read and cut to the
  minute occurrences are matched on.
- `series_id` (string, optional) — the `ID` of the series the occurrence
  belongs to. Read from `SERIES_ID`.

Unlike `timestamp_next` and `timestamp_next_after`, which the agenda modes
compute, these three are parsed from the file and appear in every mode,
`--tasks` included. A field is omitted when the key is absent, and also when
what it held could not be used — an `EXDATE` naming no readable date leaves no
`excluded_dates` rather than an empty array, and the reason is reported on
stderr.

The raw keys stay in `properties` exactly as the file wrote them, beside the
parsed fields, so a consumer that read them itself before these fields existed
keeps working.

## Repeating tasks

The utility honours org-mode repeater syntax for automatically scheduling
follow-up occurrences.

### Repeater kinds

Every standard org-mode unit is supported:

- `+Nh` — every N hours. Agendas are a day grid, so an hour repeater is
  projected onto it: every day counts as an occurrence and **N is
  ignored** (`+5h` behaves like `+1h`, not like "every 5 days").
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

The modifier describes how the *stored* stamp advances when the task is
completed, which is the editor's job. This tool only places occurrences on
a calendar, so all three modifiers bracket the same grid: the agenda
placement and `timestamp_next` are identical for `+7d`, `++7d`, and
`.+7d`.

### Next occurrence (`timestamp_next`)

In the `day`/`week`/`month` agenda modes every repeating task carries
`timestamp_next` — the closest occurrence that is still upcoming:

- a date before today rolls forward to the first occurrence
  today-or-later;
- an occurrence landing on today stays today while its clock time is
  still ahead, and rolls to the following occurrence once that time has
  passed (an all-day occurrence stays today until midnight);
- the anchor is the task's own timestamp, not the occurrence the agenda
  renders, so a monthly repeater anchored on the 31st keeps naming
  month-end;
- the value is the same in every cell the task appears in — it answers
  "when does this come round next", not "what does this cell show".

The reference moment is the real local wall clock (`--tz`), independent
of `--date`/`--from`/`--to`; with `--current-date` the time of day is
unknown, so it is taken as midnight and only the date-level rolling
applies. The field is absent for non-repeating tasks, for a repeater the
parser rejects, and in the date-less `--tasks` mode (ADR-0009), which
stays deterministic. See
[ADR-0023](docs/adr/0023-next-occurrence-field.md).

### Occurrence after this one (`timestamp_next_after`)

`timestamp_next` answers "when does this come round next" and reads the
same in every cell. A reader looking at one particular day asks a
different question — "and after this one?" — which
`timestamp_next_after` answers:

- it is the first occurrence strictly after the day the cell is dated to,
  so a `+1d` task gives `2026-08-19` in the cell of the 18th and
  `2026-08-20` in the cell of the 19th;
- it is filled only in the scheduled buckets, where the task is drawn on
  a day of its own. The `overdue` and `upcoming` rows are copies borrowed
  into the reference day, so they keep `timestamp_next` and leave this
  field out;
- the clock time plays no part: the search starts at midnight of the day
  after the cell, so a 14:00 task read at 22:00 still names the next day
  rather than skipping one;
- the anchor is the task's own timestamp, as for `timestamp_next`, so a
  monthly repeater anchored on the 31st keeps naming month-end.

A consumer rendering a repeat tooltip reads `timestamp_next_after` on a
dated row and `timestamp_next` on an overdue or upcoming one. See
[ADR-0029](docs/adr/0029-next-occurrence-after-the-rendered-day.md).

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
`SCHEDULED: <2025-12-05 Fri 10:00 +1h>`

### TODO Daily task
`SCHEDULED: <2025-12-05 Fri +1d>`

### TODO Weekly meeting
`SCHEDULED: <2025-12-05 Fri +1w>`

### TODO Monthly report
`SCHEDULED: <2025-12-05 Fri +1m>`

### TODO Annual review
`SCHEDULED: <2025-12-05 Fri +1y>`

### TODO Workday-only task
`SCHEDULED: <2025-12-05 Fri +1wd>`

### TODO Every two working days
`SCHEDULED: <2025-12-05 Fri +2wd>`
```

### One occurrence that differs

A repeating timestamp describes an endless series, and there is nowhere in
that line to say that one occurrence is cancelled or happens at another time.
A `MOVED` line says where one occurrence went
([ADR-0038](docs/adr/0038-a-move-is-written-inside-the-series.md)), and an
`EXDATE` property says which ones are gone
([ADR-0031](docs/adr/0031-exceptions-to-a-repeating-entry.md)):

````markdown
### TODO English
`SCHEDULED: <2026-08-13 Thu 15:00 +1w>`
`MOVED: [2026-08-20 Thu] -> <2026-08-22 Sat 18:00>`
```org-properties
EXDATE: 2026-08-27
```
````

- `EXDATE` lists dates the series does not occur on, separated by commas
  and/or whitespace. The 27th above simply has no class. A date may carry a
  time (`2026-08-20 15:00`, the way RFC 5545 writes it for a timed series);
  the time is read and left out, since occurrences are matched by day.
- `MOVED` names an occurrence by the day the series draws it on, and where it
  is held instead: the agenda leaves the 20th empty and draws the class on
  Saturday the 22nd at 18:00. No `EXDATE` is needed beside it — a move is not a
  cancellation. Both halves are timestamps, and the brackets say which is
  which: the occurrence is addressed with an inactive one, the day it is kept
  on is active ([ADR-0039](docs/adr/0039-the-occurrence-a-move-names-is-a-timestamp.md)).
  The target may carry a weekday, a time and a time range; a repeater or a
  warning cookie there refuses the line, since one occurrence does not repeat
  and how far ahead a `DEADLINE` warns belongs to the series. The address may
  carry a weekday and nothing else: an active bracket, an hour, a repeater or a
  cookie refuse the line the same way. A bare `2026-08-20`, which is what
  ADR-0038 wrote, is still read.
  An entry may hold as many `MOVED` lines as it has moved occurrences, and two
  naming the same occurrence leave the first standing.
- The day a `MOVED` line addresses has to be an occurrence the entry actually
  has: the series its timestamp describes must fall on that day. A line
  addressing any other day is refused and reported, because reading it would
  give the series a day it never had, and no line of this format adds an
  occurrence ([ADR-0040](docs/adr/0040-what-a-move-conflicts-with.md)). An
  entry whose timestamp does not repeat has one occurrence — the timestamp
  itself, which is edited rather than moved — and an entry with no timestamp
  has none, so a `MOVED` line on either is refused the same way.
- Where a move meets something else that speaks about the same day, both
  stand. Two occurrences moved onto one day are both drawn, each at the hour
  its own line names. An occurrence moved onto a day the series already draws
  on leaves that day holding both. An `EXDATE` naming the day a move holds an
  occurrence on cancels the occurrence the series draws there and not the
  moved one — the move names that day itself, which is the more particular
  statement; to cancel a moved occurrence, remove its `MOVED` line.
- The older shape is still read: a separate entry carrying `SERIES_ID` (the
  `ID` of the series) and `RECURRENCE_ID` (the start the occurrence *would*
  have had) takes the place of that one occurrence. It is what this project
  wrote before ADR-0038 and what a calendar export speaks, so files already
  holding it keep resolving.
- The series has to carry an `ID`, because that is what `SERIES_ID` names. An
  entry without one cannot be replaced by anything: the replacement stays an
  ordinary entry, the series keeps drawing the occurrence, and the day holds
  both. The same happens when `SERIES_ID` is misspelt. Neither is silent — see
  the last point below — but neither is refused either, because the entry
  naming the series may simply be outside this scan.
- The replacing entry is an ordinary entry: its own `TODO` state, body,
  priority and clocks stay with it rather than with the series.
- Matching is by date, because the agenda draws at most one occurrence of a
  series per day; the clock time in `RECURRENCE_ID` is carried for the reader
  and for calendar export.
- The moves reach the JSON as `moved_occurrences`, a list of
  `{from, to, time?, end_time?}` with the days as `YYYY-MM-DD` and the hours as
  `HH:MM`. A line that cannot be read is left out of it and reported.
- All three keys reach the JSON as `excluded_dates`, `recurrence_id` and
  `series_id`, normalised: dates as `YYYY-MM-DD`, `RECURRENCE_ID` as
  `YYYY-MM-DD` or `YYYY-MM-DD HH:MM`, and a date listed twice in `EXDATE` kept
  once. A `RECURRENCE_ID` written with seconds (`2026-08-20 15:00:00`, the
  form a calendar export uses) is read and normalised to the minute. The
  properties themselves stay in `properties` exactly as the file wrote them.
- An exception that cannot work is reported on stderr rather than passed over:
  a date nothing can read, an `EXDATE` with no usable date in it, a
  `RECURRENCE_ID` whose time is dropped, half a pair, and a `SERIES_ID` that
  names no entry of the run. The last of these is the one ADR-0031 calls out —
  it leaves both entries standing on the day — and it can only be reported
  when the file holding the series is part of the same run.

## Parsing a phrase into an entry

One sentence holds what the creation screen asks in nine controls: "позвонить
врачу завтра в 15:00, каждую неделю" names a heading, a date, an hour and a
repeater. `parse-phrase` reads such a sentence and prints the fields; it writes
nothing and opens no file, so the caller shows them for correction and decides
what to do with them.

The rules run here, on the device. Nothing is sent anywhere, no key is held,
and the same phrase always gives the same answer — which is what makes the
behaviour testable. The price is that only a fixed list of phrasings is
understood; anything else stays in the heading rather than being guessed at.

```bash
markdown-org-extract parse-phrase --current-date 2026-08-31 \
    "напомни позвонить врачу завтра в 15:00" "каждую неделю"
```

```json
{"current_date":"2026-08-31","heading":"позвонить врачу","keyword":null,
 "priority":null,"planning":"scheduled","date":"2026-09-01","time":"15:00",
 "repeater":"+1w","cleared":[]}
```

Each phrase refines what the earlier ones left: a field the new phrase names
replaces its value, a field it does not name keeps it, and text no rule
consumes is appended to the heading. Correcting an hour is one short phrase
("в 16:00"), not the whole sentence again.

`--current-date`, `--tz` and `--locale` mean the same to a scan and to a
phrase, so they may be written on either side of the subcommand name and read
the same:

```bash
markdown-org-extract --current-date 2026-08-31 parse-phrase "завтра купить хлеб"
markdown-org-extract parse-phrase --current-date 2026-08-31 "завтра купить хлеб"
```

The flags that only a scan can honour — `--dir`, `--glob`, `--format`,
`--output`, `--absolute-paths`, `--agenda`, `--tasks`,
`--tasks-include-done`, `--tasks-include-cancelled`, `--date`, `--from`,
`--to`, `--week-start`, `--max-tasks`, `--holidays`, `--completions` — are
refused alongside the subcommand with exit code 2, because the subcommand
stands in place of the scan and there is nothing for them to act on. The
answer always goes to stdout as JSON.

### What the rules understand

| № | Field    | Russian                                                     | English                                                  |
|---|----------|--------------------------------------------------------------|-----------------------------------------------------------|
| 1 | date     | `сегодня`, `завтра`, `послезавтра`, `в пятницу`, `через 3 дня`, `15 сентября`, `1 сентября 2026`, `2026-09-15` | `today`, `tomorrow`, `on friday`, `in 3 days`, `september 15`, `1 september 2026`, `2026-09-15` |
| 2 | planning | `к`, `до`, `срок`, `дедлайн` make it a deadline; `в`, `на` schedule it | `by`, `due`, `before`, `until` make it a deadline; `on`, `at` schedule it |
| 3 | time     | `в 15:00`, `в 15`, `в три часа дня`, `в 9 вечера`            | `at 15:00`, `at 3pm`, `at 10 o'clock`, `at 3`             |
| 4 | repeater | `каждый день`, `каждые 2 недели`, `каждый рабочий день`, `еженедельно` | `every day`, `every 2 weeks`, `every workday`, `weekly` |
| 5 | priority | `срочно`, `критично` → `A`; `важно` → `B`; `приоритет C`     | `urgent`, `asap`, `critical` → `A`; `important` → `B`; `priority C` |
| 6 | keyword  | `выполнено`, `сделано`, `завершено` → `DONE`; `в работу` → `TODO`; `отменено` → `CANCELLED` | `done`, `completed` → `DONE`; `todo` → `TODO`; `cancelled` → `CANCELLED` |
| 7 | cleared  | `убрать дату`, `снять срок`, `без времени`, `убрать повтор`, `без приоритета` | `no date`, `remove the time`, `no repeat`, `clear the priority` |
| 8 | heading  | everything the rules did not consume                          | everything the rules did not consume                       |

A weekday resolves to the nearest one **on or after** the reference day, which
is what a bare weekday name does in upstream's `org-read-date`: "во вторник"
said on a Tuesday means that Tuesday. A month day without a year resolves the
same way — the next 15 September, not the one that has passed.

### Editing an entry that exists

The last two fields are what a phrase says about an entry that is already
written: a keyword moves it between `TODO`, `DONE` and cancelled, and a field
can be said to be empty. The grammar is one; which of the two things a phrase
does is the caller's reading of the answer.

```bash
markdown-org-extract parse-phrase --current-date 2026-08-31 \
    "отметь выполненной и убрать повтор"
```

```json
{"current_date":"2026-08-31","heading":"","keyword":"DONE","priority":null,
 "planning":null,"date":null,"time":null,"repeater":null,
 "cleared":["repeater"]}
```

`cleared` names the fields the phrase emptied, which is how a caller tells
"убрать дату" from a phrase that says nothing about the date: `date` is `null`
in both answers, and only the first lists it in `cleared`. Emptying the date
empties the planning line with it. Naming a field and emptying it cancel each
other, so a chain says what its last phrase said.

A conjunction joins two instructions in one phrase — "перенеси на пятницу в
16:00 и сделай срочной" sets the date, the hour and the priority — because a
verb of editing may start again after `и` / `and`. A verb of creating may not:
"позвонить и напомни про отчёт" is one entry, not an entry and an instruction.

An empty heading is what an edit leaves behind: every word was understood. Text
left over means a word was not, and a caller that is editing refuses the phrase
over it rather than writing the leftover into the entry.

Negation removes nothing: "не завтра" sets no date and empties none, and both
words land in the heading like any other text the rules do not know.

### What it costs

| № | Cost                                                                                     | Why it is accepted                                                        |
|---|------------------------------------------------------------------------------------------|----------------------------------------------------------------------------|
| 1 | A lead-in verb is eaten at the start: `напомни`, `поставь`, `добавь`, `создай`, `запиши`, `remind me to`, `add a task`, `create`, and the verbs of editing `перенеси`, `сделай`, `отметь`, `смени`, `move`, `mark`, `make`, `change`, `set` | An entry that really begins with one loses that word, which is visible on the screen before anything is written and is fixed by a second phrase or by hand |
| 2 | `в N` / `at N` with nothing after the number reads as an hour, so "в 5 минутах ходьбы" sets 05:00 | The alternative is refusing the shortest way an hour is said; the leftover words still reach the heading |
| 3 | A phrasing outside the list is not understood at all                                      | It stays in the heading, so nothing said is lost — only unsorted            |

The list of lead-in verbs is closed and checked by the table of examples,
including phrases where the same verb stands elsewhere and is therefore left
alone ("позвонить и напомни про отчёт").

### From Rust

```rust
use chrono::NaiveDate;
use markdown_org_extract::{parse_phrases, refine_entry, PhraseEntry, PhraseKeyword};

let today = NaiveDate::from_ymd_opt(2026, 8, 31).expect("valid day");

// The whole sentence at once.
let entry = parse_phrases(["позвонить врачу завтра в 15:00"], "ru,en", today);
assert_eq!(entry.heading, "позвонить врачу");

// Or one phrase at a time, keeping the entry between calls — what a screen
// that refines as the person speaks does.
let entry = refine_entry(entry, "каждую неделю", "ru,en", today);
assert_eq!(entry.date, NaiveDate::from_ymd_opt(2026, 9, 1));

// An edit of an entry that exists: a keyword, and a field said to be empty.
let edit = parse_phrases(["отметь выполненной и убрать повтор"], "ru,en", today);
assert_eq!(edit.keyword, Some(PhraseKeyword::Done));
assert_eq!(edit.cleared.names(), ["repeater"]);
```

## Project layout

```
markdown-org-extract/
├── src/
│   ├── lib.rs              # Library facade: the public API embedders use
│   ├── main.rs             # CLI entry point: parse args, call the library, write
│   ├── cli.rs              # Argument parsing (clap), tracing init — binary only
│   ├── format.rs           # OutputFormat (clap ValueEnum) — binary only
│   ├── scan.rs             # Directory walk, file I/O, ScanOptions
│   ├── agenda.rs           # Agenda logic (day/week/month), repeaters
│   ├── parser.rs           # Task extraction from the markdown AST
│   ├── phrase.rs           # One sentence read into the fields of an entry
│   ├── render.rs           # Markdown/HTML rendering
│   ├── locale.rs           # Weekday translation tables
│   ├── error.rs            # AppError
│   ├── exceptions.rs       # EXDATE / SERIES_ID / RECURRENCE_ID: the occurrences a series does not have
│   ├── types.rs            # Task / Priority / DayAgenda / ProcessingStats
│   ├── clock.rs            # CLOCK parsing and time aggregation
│   ├── holidays.rs         # RF workday calendar (singleton, binary search)
│   ├── regex_limits.rs     # `compile_bounded`: regex with size/DFA caps
│   ├── timestamp.rs        # Timestamp module root: re-exports the parts below
│   └── timestamp/          # Org-mode timestamp parsing
│       ├── parser.rs       #   <2024-12-05 Thu 10:00 +1d> → ParsedTimestamp
│       ├── extract.rs      #   pull timestamp/CREATED out of arbitrary text
│       ├── repeater.rs     #   parsing and arithmetic of repeaters (+1d, ++2w, .+1wd…)
│       └── weekdays.rs     #   normalisation of Russian weekday names
├── tests/
│   ├── cli.rs              # CLI integration tests (assert_cmd)
│   ├── lib_api.rs          # Tests against the library API, no process spawned
│   ├── phrase.rs           # The phrases the rules understand, table by table
│   ├── properties.rs       # Properties over the parsers (proptest)
│   ├── dev_scripts.rs      # The helper scripts below, run against fixtures
│   ├── release_check_changelog.rs  # check-changelog.sh against crafted CHANGELOGs
│   ├── release_packaging.rs        # package-archive.sh / verify-archive.sh
│   ├── signal_handling.rs  # SIGINT/SIGPIPE behaviour of the binary
│   ├── third_party_licenses.rs     # THIRD-PARTY-LICENSES.txt is current and reproducible
│   └── fixtures/           # Stand-in tools the script tests call
├── examples/               # Sample markdown files
├── docs/                   # Supplementary documentation
├── scripts/                # Developer helper scripts (see table below)
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

### Helper scripts

The `scripts/` directory holds developer-facing helpers. None of them
ship to end users on crates.io (the `Cargo.toml` `exclude` list omits
the whole directory).

Releases are pushed in two steps, **the branch first and the tag second**:

```bash
git push origin master
git push origin vX.Y.Z
```

Pushing a tag hands GitHub the commit along with it, so a tag pushed on its own
starts the release workflow over a commit no branch of `origin` contains — the
crate would be published while `master` still stands where it did. The
`verify` job checks the tagged commit is on `origin/master` and refuses
otherwise, so the wrong order costs a failed run rather than a stranded
release.

The workflow publishes last. `verify` runs every check that can refuse the
release, `package-binaries` then builds and verifies one archive per target,
`publish` pushes the crate to crates.io, and `release` creates the GitHub
Release and attaches the archives to it — so a packaging failure costs a
re-run rather than a version that can only be yanked
([ADR-0033](docs/adr/0033-nothing-irreversible-before-the-archives-are-checked.md)).
Running the workflow by hand with `dry_run` exercises all of that except the
last two jobs.

Before the tag exists, run the workflow by hand with an **empty tag**: that
rehearses the release on the branch the run was started from
([ADR-0034](docs/adr/0034-a-release-is-rehearsed-before-it-is-tagged.md)). The
version rehearsed is the one `Cargo.toml` carries, so prepare the release
first — bump the version, move the `## [Unreleased]` section over, update the
README snippet — and then ask whether a tag cut from this tree would go
through:

```bash
gh workflow run release.yml --ref <branch>
```

Everything runs except what only a tag can answer: its format and its
annotated body. `publish` and `release` do not run at all — there is nothing
to publish.

| Script                          | Purpose                                                                |
| ------------------------------- | ---------------------------------------------------------------------- |
| `scripts/check.sh`              | Full CI parity locally: `fmt --check` + `yamllint` + `clippy -D warnings` + `doc -D warnings` + `cargo test`. Run before every commit; CI runs the same steps |
| `scripts/install-hooks.sh`      | Install a git `pre-commit` hook that delegates to `scripts/check.sh`. Pass `--force` to overwrite an existing hook |
| `scripts/audit.sh`              | RustSec advisory scan (`cargo audit`) for a pre-push / pre-release run. Kept out of `check.sh` so the commit loop stays offline and fast; skips with an install hint if `cargo-audit` is absent. CI runs the same check via `rustsec/audit-check` |
| `scripts/check-changelog.sh`    | Validate `CHANGELOG.md` shape before tagging: `## [Unreleased]` empty, latest version section present, version numbers monotonic |
| `scripts/package-archive.sh`    | Build a release archive (`.tar.gz` on Linux / macOS, `.zip` on Windows) with a deterministic layout. Used by `.github/workflows/release.yml` |
| `scripts/verify-archive.sh`     | Verify a release archive's filename, layout, and SHA-256. Mirrors what downstream packagers run |
| `scripts/release-validate-tag.sh` | Validate that a release tag follows `vX.Y.Z[-pre+build]`. Called from `.github/workflows/release.yml` on both push-tag and workflow_dispatch paths |
| `scripts/release-prep.sh`       | Print the canonical annotated-tag message for a version: the `v<X.Y.Z>` subject plus the `CHANGELOG.md` section body (`### ` subheadings included). Argument is the **bare version**, no `v` prefix (e.g. `0.7.0`, not `v0.7.0`). Tag with `git tag -a vX.Y.Z --cleanup=verbatim -F <(scripts/release-prep.sh X.Y.Z)` — `--cleanup=verbatim` is required, otherwise the default cleanup deletes the `### ` headings as comment lines |
| `scripts/generate-third-party-licenses.sh` | Render `THIRD-PARTY-LICENSES.txt` from the crates linked into the published binary, with the full text of every licence they ship (ADR-0024). `tests/third_party_licenses.rs` checks the committed file against a fresh run |
| `scripts/release-check-ancestry.sh` | Refuse to publish a commit the remote's release branch does not contain (ADR-0017: the branch is unprotected, and a pushed tag alone can start the release over a commit `master` never saw). Arguments are the remote and the branch, defaulting to `origin master`. Run from `.github/workflows/release.yml` and covered by `tests/dev_scripts.rs` |
| `scripts/release-verify-tag-body.sh` | Check that the release tag is annotated and its body mirrors the `CHANGELOG.md` section (ADR-0011). Argument is the **bare version**, no `v` prefix (the script prepends `v` internally to look up the tag); e.g. `scripts/release-verify-tag-body.sh 0.7.0` checks tag `v0.7.0`. Run from `.github/workflows/release.yml` before publishing; also runnable locally right after tagging |

The `Cargo.toml` `exclude` list omits `docs/`, `.github/`, `scripts/`,
`TODO.md`, and `CHANGELOG.md` from the published crate tarball on
crates.io — these files matter for repository contributors but not for
downstream `cargo install` users. The GitHub Release archives keep
the binary, README, and LICENSE only (see "For downstream packagers"
above).

See also:
- [docs/adr/](docs/adr/) — architectural and policy decisions index
- [docs/adr/0002-supported-org-mode-subset.md](docs/adr/0002-supported-org-mode-subset.md) — supported Org-mode subset
- [docs/adr/0003-clock-metadata-support.md](docs/adr/0003-clock-metadata-support.md) — CLOCK marker implementation details
- [docs/adr/0014-active-and-inactive-timestamps.md](docs/adr/0014-active-and-inactive-timestamps.md) — active vs inactive timestamp bracket policy
- [CHANGELOG.md](CHANGELOG.md) — version history
- [TODO.md](TODO.md) — deferred technical tasks

## Dependencies

- `clap` — command-line argument parsing
- `clap_complete` — the completion scripts `--completions <SHELL>` prints
- `comrak` — markdown parsing (without onig/syntect: `default-features = false`)
- `regex` — regular expressions (with size/DFA caps)
- `serde` / `serde_json` — data serialisation
- `chrono` / `chrono-tz` — dates and timezones
- `grep-regex` / `grep-searcher` — fast pre-filter over keywords
- `ignore` — directory tree walk that honours `.gitignore`
- `globset` — glob compilation for `--glob`
- `aho-corasick` — one pass over a timestamp for every localized weekday name at once
- `signal-hook` — the SIGINT / SIGTERM handler the scan loop polls
- `tracing` / `tracing-subscriber` — structured diagnostic logging (`--verbose`, `--quiet`, `--color`, `--no-color`)

Lazily initialised `static` regular expressions use `std::sync::LazyLock`
from the standard library (Rust 1.80+; the project itself requires 1.85).

## License

MIT — see the [LICENSE](LICENSE) file.

The released binary is statically linked and therefore contains code
from third-party crates. Their licence texts and copyright notices are
reproduced in [`THIRD-PARTY-LICENSES.txt`](THIRD-PARTY-LICENSES.txt),
which ships inside every release archive. The file is generated by
`scripts/generate-third-party-licenses.sh` — run it after any change to
the dependency graph and commit the result; CI fails on a stale copy.
Which licences are acceptable at all is fixed by the allow-list in
[`deny.toml`](deny.toml).

### Provenance of `holidays_ru.json`

The holiday calendar and weekend-shift table in `holidays_ru.json` was
compiled by the project author from the official RF government decrees
on weekend rescheduling. This is public factual information and is not
subject to copyright. For packaging convenience the file is distributed
under the same MIT licence as the rest of the code.

Attribution and a schema description are duplicated inside the file
itself under the `_meta` key (`build.rs` ignores underscore-prefixed
top-level keys, so the block has no effect on the compiled output).
