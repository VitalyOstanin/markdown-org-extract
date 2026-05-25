# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Table of contents

- [\[Unreleased\]](#unreleased)
- [\[0.5.0\] — 2026-05-25](#050--2026-05-25)
- [\[0.4.2\] — 2026-05-22](#042--2026-05-22)
- [\[0.4.1\] — 2026-05-22](#041--2026-05-22)
- [\[0.4.0\] — 2026-05-22](#040--2026-05-22)
- [\[0.3.1\] — 2026-05-19](#031--2026-05-19)
- [\[0.3.0\] — 2026-05-19](#030--2026-05-19)
- [\[0.2.2\] — 2026-05-17](#022--2026-05-17)
- [\[0.2.1\] — 2026-05-17](#021--2026-05-17)
- [\[0.2.0\] — 2026-05-17](#020--2026-05-17)
- [\[0.1.6\] — 2026-05-11](#016--2026-05-11)
- [\[0.1.5\] — 2025-12-06..2025-12-09](#015--2025-12-062025-12-09)

## [Unreleased]

### Fixed

- Monthly repeaters whose base day is later in the month than the
  query day no longer return a *future* date for a `Past`
  preference. `bracket_month` computed the bracket from the bare
  month-number difference, so for a `+1m` repeater based on
  2024-01-31 and a query on 2024-04-15 it returned the truncated
  April occurrence 2024-04-30 (after the query) instead of the
  last real occurrence 2024-03-31. The `pick` / `closest_date`
  contract `n1 <= current < n2` is now honoured for the month
  grid: when the occurrence in the query's own month falls after
  the query, the bracket steps back one full period. This makes
  `closest_date(.., Past, ..)` consistent for monthly repeaters
  and removes an accidental gap where such a task could be skipped
  by the agenda overdue filter (F1 in the 2026-05-25 logic
  review). Day truncation itself is unchanged (a deliberate
  divergence from upstream's day-overflow semantics).
- A timestamp whose body is longer than 80 characters but within
  the extractor's `TS_BODY_MAX` (256) ceiling no longer parses to
  `None` after passing extraction. `parse_org_timestamp` bounded
  its single-bracket regexes with a literal `80` while the
  extractor in `extract.rs` used `TS_BODY_MAX`, so such a task was
  extracted into `--tasks` yet silently dropped from every agenda
  bucket. Both sides now share `TS_BODY_MAX` (F2 in the
  2026-05-25 logic review).

### Security

- `release.yml` no longer interpolates `inputs.tag` into the shell
  body of the "Resolve tag and version" step. The value now flows
  through `env:` so a malicious workflow_dispatch input cannot
  smuggle additional commands at YAML expansion time (SEC-1 in the
  2026-05-25 review). The resolved tag is then validated by the
  new `scripts/release-validate-tag.sh` against the
  `vX.Y.Z[-pre+build]` SemVer-style format on both code paths
  (push-tag and workflow_dispatch).

### Added (developer)

- `scripts/audit.sh` runs a local RustSec advisory scan
  (`cargo audit`) for a deliberate pre-push / pre-release check. It
  is intentionally kept out of `scripts/check.sh` (and therefore the
  pre-commit hook) so the commit loop stays offline and fast per
  ADR-0017; CI continues to run the same scan on every push via
  `rustsec/audit-check`. If `cargo-audit` is not installed the
  script prints the install command and exits 0 (MIN-9 in the
  2026-05-25 review).
- `scripts/release-prep.sh <X.Y.Z>` prints the canonical
  annotated-tag message for a version — the `v<X.Y.Z>` subject plus
  the `CHANGELOG.md` section body, using the same awk extraction as
  the release workflow — so the maintainer tags with
  `git tag -a vX.Y.Z --cleanup=verbatim -F <(scripts/release-prep.sh X.Y.Z)`
  instead of hand-copying (I2 in the 2026-05-25 release review).
- `scripts/release-verify-tag-body.sh <X.Y.Z>` checks that the
  release tag is annotated and its body mirrors the CHANGELOG
  section, and the release workflow now runs it before publishing.
  This closes the gap that let the v0.5.0 tag ship without its
  `### ` headings: the default tag-message cleanup (`strip`) deletes
  every line beginning with the comment character, so the
  `### Added` / `### Changed` headings were silently dropped. The
  verifier fails such a tag and points at `--cleanup=verbatim`
  (L1 / I1 in the 2026-05-25 release review).

### Documentation (developer)

- ADR-0017 records the decision not to configure branch
  protection on `master`. For a single-maintainer repository,
  the pre-commit hook installed by `scripts/install-hooks.sh`
  (delegating to `scripts/check.sh`) is the canonical safety net
  against fmt / clippy / test regressions; GitHub branch
  protection with `enforce_admins=false` is mostly cosmetic and
  with `enforce_admins=true` imposes a multi-minute PR ceremony
  on every change. The 0.5.0 fmt-fail precedent is addressed by
  the local hook, not by remote enforcement.
- README now documents `scripts/check.sh` as the single command
  that mirrors CI locally (fmt + yamllint + clippy + doc + test).
  Running `cargo test` alone does not catch `rustfmt` or
  `yamllint` regressions, which was the immediate cause of the
  0.5.0 CI red. A new "Helper scripts" subsection under "Project
  layout" tabulates every script under `scripts/` with its
  purpose.

### Changed (diagnostics)

- The three per-file failure branches in `scan_files`
  (read / content-search / UTF-8) now log the underlying
  `io::Error` / search error / `Utf8Error` at `debug` level with
  the path, instead of discarding it. The cause is visible at
  `-vv`; the default warn stream stays aggregated into the single
  summary record (m3 in the 2026-05-25 review). Previously only
  the path reached the summary and the reason was lost entirely.
- `ProcessingStats::print_summary` now emits one structured warn
  record per run instead of up to 22 separate lines. The list of
  failed paths is carried in a `failed_paths` field on the
  summary record itself (capped to `MAX_DIAGNOSTIC_ITEMS=20` as
  before); jq / grep can read it without stitching together
  multiple consecutive warn lines, and a noisy summary no longer
  drowns out genuine per-file warnings.

### Tests

- The byte-exact JSON wire-contract snapshots now cover CLOCK
  output (`clocks[]` element shape plus `total_clock_time`), an
  inactive `[...]` timestamp (`timestamp_active: false`), a
  repeater + warning cookie preserved verbatim in the `timestamp`
  string, and the week agenda envelope; the month envelope is
  pinned structurally (31 buckets spanning the calendar month, the
  four per-day keys, the task on the 21st) rather than as a
  ~190-line literal. Previously only the tasks-mode and day-agenda
  shapes were snapshotted, leaving the ADR-0015 external contract
  for week / month / CLOCK / inactive / repeater under-pinned
  (MIN-12 in the 2026-05-25 review).
- New regression test `utf8_bom_prefix_does_not_swallow_first_heading`
  pins the current behaviour for files saved as UTF-8 with a leading
  byte-order mark (e.g. Windows Notepad, VS Code with the "UTF-8 with
  BOM" option). The 2026-05-25 encoding review (point 1) suggested
  this would silently drop the first task; comrak 0.52 already strips
  the BOM cleanly. The test ensures any future comrak upgrade that
  regresses this path fails CI rather than silently losing tasks.

### Performance

- The default `--locale ru,en` path no longer rebuilds the
  Aho-Corasick weekday-substitution engine on every
  `normalize_weekdays` call. The engine is now constructed once
  per process for the canonical `RU_WEEKDAY_MAPPINGS` table and
  reused across files and tasks. Non-default mappings (tests,
  ad-hoc tables) keep the previous per-call build.
- `parser::finalize_task` calls the new
  `parse_timestamp_fields_normalized` directly. The previous code
  ran `normalize_weekdays` a second time on a substring that was
  already weekday-normalised at extraction time; the redundant
  scan is removed.

### Changed

- `parser::extract_tasks` no longer owns a process-global
  `AtomicUsize` for the invalid-timestamp warning budget. The
  counter is now passed through the new
  `parser::extract_tasks_with_counter` API and lives on
  `ProcessingStats::ts_warnings_emitted`. A long-running library
  consumer or a parallel scan no longer pollutes another caller's
  budget. The legacy `extract_tasks(path, content, mappings,
  max_tasks)` wrapper is kept and now creates a per-call counter
  internally; existing callers see no behaviour change.
- The `scheduled_no_time` agenda bucket is now sorted
  deterministically: by priority (high first, mirroring upstream
  org-agenda's `urgency-down`), then by file path and line. It was
  the only day-agenda bucket left unsorted, so its order followed
  the filesystem walker's unspecified traversal and could differ
  between runs on identical input (m1 in the 2026-05-25 logic
  review). No-priority tasks sort last, consistent with the
  `--tasks` flat list. The week and month agendas inherit the same
  ordering, since they build each day through the same code path.

### Changed (build)

- `build.rs` resolves `holidays_ru.json` through
  `CARGO_MANIFEST_DIR` instead of the current working directory.
  Cargo runs the build script with cwd at the package root today,
  but anchoring on the documented `CARGO_MANIFEST_DIR` contract
  keeps the build correct if it is ever invoked from a packaged
  tarball or a different cwd, and the read error now names the
  absolute path it tried (build.rs minor in the 2026-05-25
  security/DX review).

### Dependencies

- `serde_json` lockfile bumped `1.0.149` → `1.0.150` (a single
  patch release; no semantic change, SemVer-compatible with the
  existing `1.0.149` constraint). The `tempfile` dev-dependency
  constraint is raised `3.10` → `3.27` to match the version the
  lockfile already pinned; this is a cosmetic alignment of the
  manifest with the resolved tree (MIN-16 in the 2026-05-25
  tech-versions review).

### Removed

- The dead `AppError::Walk` variant and its
  `From<ignore::Error>` conversion are gone. `scan_files` handles
  walker errors locally (records the failing path in the summary
  and continues the traversal) and never propagates an
  `ignore::Error` through `?`, so the variant was reachable only
  from its own unit tests. Removing it follows the same rationale
  as the deliberately-absent blanket `From<io::Error>`: a future
  caller that wants to make a walk error fatal must add the
  conversion back explicitly, with a rationale, rather than
  inheriting a silent path (MIN-3 in the 2026-05-25 review).

### Documentation

- README aligned with 0.5.0 behaviour: `CLOSED:` / `CREATED:`
  planning markers shown with inactive `[...]` brackets, a new
  active-vs-inactive table documents the per-keyword policy from
  ADR-0014, `timestamp_active` field added to the JSON examples
  and to the Parsed timestamp fields list, and the broken
  `docs/CLOCK_IMPLEMENTATION.md` / `docs/org-mode-keywords.md`
  links in "See also" are replaced with pointers to ADR-0002,
  ADR-0003, ADR-0014 and the `docs/adr/` index.
- ADR-0016 pins the `RUST_LOG` overrides `--verbose` / `--quiet`
  precedence that 0.5.0 already ships. The contract is enforced
  by a regression test in `tests/cli.rs`.
- The README's "Workday-handling test coverage" section, which
  enumerated what each test module checks, is reformulated as a
  short behaviour-level "Workday handling" note that points at the
  test modules (`cargo test -- --list`) for the authoritative list.
  The per-module checklist duplicated module structure and went
  stale; describing coverage by behaviour follows ADR-0007 (MIN-10
  in the 2026-05-25 review).
- The `--verbose` help text now states that `-vvv` is the maximum
  level and that extra `-v` are ignored (with a one-off saturation
  warning), matching the runtime behaviour pinned by
  `verbose_saturation_warns_on_vvvv_and_beyond` (MIN-8 in the
  2026-05-25 review).
- ADR-0018 records the warning-period cookie scanner's deliberate
  divergence from upstream `org-get-wdays`: a leading whitespace
  separator is required and `]` is accepted as a terminator (for
  inactive `[...]` timestamps). The fail-closed reading — `-3day`
  and a glued `-3d-2d` match no cookie — is now pinned by
  `parser.rs::warning_cookie_requires_separator_and_terminator`
  and the regex comment cites the exact upstream regex. Satisfies
  the ADR-0012 verify-and-record rule for F5 in the 2026-05-25
  logic review.

## [0.5.0] — 2026-05-25

### Breaking

- `CLOSED:` and `CREATED:` keywords no longer accept active angle
  brackets. `CLOSED: <…>` and `CREATED: <…>` were accepted in 0.4.x
  but diverged from upstream Emacs Org-mode (`org-closed-time-regexp`)
  and from the `org-expiry` convention for `CREATED`. ADR-0014 pins
  the policy: `CLOSED:` and `CREATED:` require inactive square
  brackets (`CLOSED: [2024-12-08 Sun]`, `CREATED: [2024-09-01 Mon]`);
  `SCHEDULED:` and `DEADLINE:` continue to require active angle
  brackets and now reject `[…]` by construction. Mixed pairs
  (`<…]`, `[…>`) are also rejected. Migration: rewrite affected
  timestamps with the matching bracket form; bodies are unchanged.

### Added

- New JSON field `timestamp_active: bool` on each task surfaces the
  bracket form (`true` for `<…>`, `false` for `[…]`). The field is
  omitted when no timestamp is present, so the addition is
  non-breaking for consumers that ignore unknown keys. ADR-0015
  documents the schema-evolution policy under which this field was
  added (no `schema_version` field; downstream `markdown-org-vscode`
  pins a minimum extractor version via
  `x-markdown-org.extractorVersion` in its `package.json`).
- Inline plain timestamps now accept both `<2024-12-05 Thu>` and
  `[2024-12-05 Thu]` forms. Previously only the angle form was
  recognised. Inactive plain timestamps appear in the JSON output
  with `timestamp_active: false` but never feed agenda (see ADR-0014).
- `parse_org_timestamp` recognises repeater (`+1d`, `.+2w`, …) and
  warning cookies (`-Nd`, `-Nw`, …) inside both bracket forms.

### Changed

- Agenda (day / week / month) drops any task whose timestamp is
  inactive `[…]`. `CLOSED:`-typed entries were already excluded from
  overdue / upcoming buckets; the new filter extends that to inline
  plain `[…]`. `SCHEDULED:` / `DEADLINE:` are unaffected because the
  regex layer guarantees they are always active.
- CLOCK behaviour is unchanged. ADR-0014 explicitly preserves
  ADR-0003: `CLOCK:` accepts both `<…>` and `[…]` start/end
  endpoints, and a closed CLOCK range may mix the two between start
  and end.

### Documentation

- New ADR-0014 (per-keyword bracket policy for timestamps) amends
  ADR-0002 and pins the regex layer's per-keyword rules against
  upstream Emacs Org-mode citations.
- New ADR-0015 (JSON schema evolution policy) documents the
  non-breaking-field rule and the
  `x-markdown-org.extractorVersion`-based coordination with
  `markdown-org-vscode`.

## [0.4.2] — 2026-05-22

### Fixed

- `tests/release_packaging.rs` no longer fails the `Test (macOS)`
  job. The 0.4.1 release fixed `scripts/package-archive.sh` to
  prefer `gtar` when available, but the test helper
  `make_tar_gz_top_level` invoked `Command::new("tar")` directly
  with GNU-tar reproducibility flags (`--sort=name`, `--owner=0`,
  `--group=0`, `--numeric-owner`, `--mtime=@0`), bypassing the
  script's fallback chain. macOS' BSD tar rejected those flags
  and four tests panicked at the assertion. The helper now goes
  through a new `gnu_tar()` function that mirrors the script's
  `if command -v gtar` chain: `gtar` when present (installed in
  CI via Homebrew), plain `tar` otherwise. The four remaining
  `Command::new("tar")` calls in the file use only BSD-compatible
  flags (`-czf`) and are kept unchanged.

## [0.4.1] — 2026-05-22

### Added

- Release archives are now published for `x86_64-apple-darwin`
  (Intel Macs) in addition to `aarch64-apple-darwin` (Apple
  Silicon). The `package-binaries` matrix in
  `.github/workflows/release.yml` adds a second `macos-latest`
  row that cross-compiles arm → x86_64 via
  `rustup target add x86_64-apple-darwin` + `cargo build --target`.
  Every matrix entry now builds with explicit `--target <triple>`
  for consistency between native and cross-compiled rows; the
  `BIN_PATH` env passed to `scripts/package-archive.sh` was
  updated accordingly. Closes the gap where the
  `markdown-org-vscode` extension's Intel-Mac users could not
  receive a working extractor binary inside the VSIX.

### Fixed

- Release scripts are now portable to macOS' default toolchain
  (`bash` 3.2 + BSD `tar`). The previous 0.4.0 release was blocked
  by `Test (macOS)` because `scripts/package-archive.sh` used
  GNU-tar-only flags (`--sort=name`, `--owner=0`, `--group=0`,
  `--numeric-owner`, `--mtime='@0'`) and `scripts/verify-archive.sh`
  used `mapfile` (bash 4+). Fixes:
  - `package-archive.sh` now prefers `gtar` (installed in CI via
    `brew install gnu-tar`) and falls back to plain `tar` on
    Linux/Windows where GNU tar is the system `tar`. Archives
    on all platforms keep the same reproducibility flags.
  - `verify-archive.sh` replaces both `mapfile -t arr < <(cmd)`
    invocations with a portable `while IFS= read -r line; do
    arr+=("$line"); done < <(cmd)` loop that works on bash 3.2.
  - CI (`ci.yml`) and release (`release.yml`) workflows install
    `gnu-tar` via Homebrew on `macos-latest` runners before
    `cargo test` and before `package-archive.sh`. The conditional
    step is `if: runner.os == 'macOS'`, so Linux/Windows runners
    are unaffected.

## [0.4.0] — 2026-05-22

### Added

- SIGINT and SIGTERM (the latter Unix-only) are now caught mid-scan.
  The walker polls a shared atomic flag between files; on signal the
  walk stops, the partial `processing summary` (with
  `interrupted = true`) is logged on stderr, `--output` is not
  written, and the process exits with `130` (`128 + SIGINT`). Sending
  the signal after the walker has completed is a no-op because the
  poll-point has already been passed. New exit-code `130` row in the
  README "Exit codes" table; new `interrupted: bool` field on
  `ProcessingStats`.
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
- `scripts/check.sh` runs the same gates CI runs (`cargo fmt --check`,
  `cargo clippy -D warnings`, `cargo test --all-features`) in order,
  fail-fast, with a uniform `==>` banner so failures are easy to spot.
  Intended as a one-liner before opening a PR.
- `scripts/install-hooks.sh` installs a `pre-commit` git hook that
  delegates to `scripts/check.sh`. Refuses to overwrite an existing
  hook unless `--force` is passed. The installed hook re-execs from
  the repo root, so `git commit` from any subdirectory behaves the
  same.
- `clippy.toml` pins `msrv = "1.85"` so clippy only suggests APIs
  available on the declared MSRV. Suggestions like
  `manual_strip` -> `str::strip_prefix` will no longer rewrite code
  in a way that breaks the `msrv` CI job. Bump both `clippy.toml`
  and `Cargo.toml` `rust-version` together.
- CI `lint` job now runs `cargo doc --no-deps --all-features` with
  `RUSTDOCFLAGS="-D warnings"`. The crate has no library target
  (docs.rs is not used — see 0.3.1 notes), but `cargo doc` still
  builds rustdoc for the binary's modules, so this step guards
  intra-doc links and other rustdoc warnings before release.
  `scripts/check.sh` mirrors the new step locally between `clippy`
  and `cargo test`.
- Crate root documented with `#![warn(missing_docs)]` and a module
  docstring. The three remaining undocumented top-level pub items
  (`agenda::AgendaOutput`, `agenda::filter_agenda`,
  `cli::get_weekday_mappings`) gained rustdoc explaining inputs,
  variants, and error conditions, including intra-doc links to
  related types and the ADR that defines the date-window model.
- `.github/workflows/release.yml` permissions follow least-privilege:
  the workflow default is now `contents: read`, the `test` / `lint` /
  `msrv` jobs spell out `contents: read` explicitly, and only the
  `publish` job declares `contents: write` (required for
  `gh release create`). Shrinks the blast radius if a compromised
  action runs in the pre-publish jobs.
- `scripts/check-changelog.sh` is stricter on the released version's
  header: it now requires the exact form
  `## [<version>] — YYYY-MM-DD` (em-dash, ISO date) and also verifies
  that this section is the FIRST one immediately after
  `## [Unreleased]`. A stale older heading (e.g. an aborted release)
  between them is refused, and so are missing-date / ASCII-hyphen
  variants of the header. Older entries in the file are unchanged;
  the strict regex applies only to the version being released.
- `CHANGELOG.md` `[0.1.5]` entry replaced the `— earlier` placeholder
  with the concrete `2025-12-06..2025-12-09` range covering all
  pre-0.1.6 work. Documented inline that the range format is a
  grandfathered exception and not a new pattern other releases may
  adopt.
- JSON / Markdown / HTML output now always ends with exactly one
  trailing `\n`, regardless of format and destination (stdout or
  `--output <file>`). The Markdown renderer already produced a
  trailing newline; the JSON serializer and the HTML formatter did
  not, leaving the terminal prompt on the same line as the closing
  `]` / `</html>` and breaking POSIX "text file" tools. A helper
  (`ensure_trailing_newline`) is applied to the rendered string
  before every write so existing renderers can stay newline-agnostic.
  `--holidays` output goes through the same helper.
- Piping the binary into a consumer that closes the pipe early
  (`markdown-org-extract … | head -n 1`) no longer surfaces
  `error: io: <stdout>: Broken pipe (os error 32)` and no longer
  exits 74. The `BrokenPipe` `io::ErrorKind` is now intercepted at
  the top-level error handler and the process exits 0 silently,
  matching the behaviour of `cat`, `grep`, `jq` and other Unix
  pipeline-friendly tools. Every other IO failure is still
  reported and still maps to exit 74.
- GitHub Releases now ship prebuilt binaries for three tier-1
  targets: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`,
  and `x86_64-pc-windows-msvc`. Each release attaches a `.tar.gz`
  / `.zip` archive containing the binary plus `README.md` and
  `LICENSE`, with a matching `.sha256` companion. Archives are
  reproducible (sorted, fixed mtime / owner) so a re-run of the
  workflow on the same commit produces byte-identical assets.
  Adding `aarch64-unknown-linux-gnu` or `x86_64-apple-darwin` is
  one matrix entry's worth of YAML.
- Release-archive packaging is now driven by `scripts/package-archive.sh`
  and verified by `scripts/verify-archive.sh`; the `package-binaries`
  matrix in `release.yml` calls both. Verification enforces the
  downstream-packager contract documented in the README (filename
  template, sibling `.sha256` that passes `sha256sum -c`, single
  top-level directory matching the archive stem, exact file set inside
  binary + README + LICENSE). Both scripts are exercised by Rust
  integration tests in `tests/release_packaging.rs` so contract
  regressions surface in `cargo test` rather than waiting for a tag
  push -- this is how the original Windows-zip flat-layout bug was
  caught.
- Warning-period cookie on DEADLINE timestamps is now honoured.
  `DEADLINE: <2025-12-10 Wed -3d>` shrinks the upcoming window to
  three days; `DEADLINE: <2025-12-20 Sat -30d>` expands it to thirty.
  Units `h/d/w/m/y` are accepted and converted to whole days using
  upstream `org-get-wdays`'s factors (`d=1`, `w=7`, `m=30.4`,
  `y=365.25`, `h=1/24`, floored). The cookie may appear in either
  order relative to the repeater (`<... +1y -3d>` and
  `<... -3d +1y>` both work) -- the parser scans for repeater and
  warning independently, matching upstream's position-agnostic
  handling. Without a cookie the global 14-day window stays in
  effect.
- `.yamllint` config and a new `yamllint .github/workflows/` step in
  `scripts/check.sh` and the CI `lint` job catch structural workflow
  issues before they reach a release. The config is tuned for
  GitHub-Actions idioms (long action-pin lines, `on:` keyword,
  `# vX.Y.Z` single-space comment, no `---` document marker) and
  loosens or disables the corresponding default rules so they do
  not produce noise.

### Documentation

- [ADR-0010](docs/adr/0010-rollback-policy.md) documents the
  rollback policy for published releases: when to `cargo yank`
  (regression, security, accidental breaking change, wrong
  artefact), when not to (release-pipeline-only failures, the
  0.3.0 → 0.3.1 precedent), how to mark the CHANGELOG (Keep a
  Changelog `### Yanked`), and the GitHub-Release pre-release
  marker. CLAUDE.md and the ADR index were updated with the
  decision pointer.
- [ADR-0011](docs/adr/0011-release-commit-and-tag-format.md) fixes
  the release commit subject (`release: <X.Y.Z>`) and the tag
  format (annotated, body mirrors the matching CHANGELOG section)
  so `git show v<X.Y.Z>` is a self-contained change description.
  Applies from the next release forward; historical commits and
  tags are not rewritten.
- Invariant test on `parse_repeater` prefix-stripping order pins
  `++1d → CatchUp`, `+1d → Cumulative`, `.+1d → Restart`. A refactor
  that re-orders the `strip_prefix` arms (matching `+` before `++`)
  would silently classify `++1d` as `Cumulative`; the explicit
  assertion now catches that. Closes logic L9 from the 2026-05-21
  review.
- `holidays::get_holidays_for_year` and
  `holidays::workdays_between_exclusive` carry rustdoc explaining the
  out-of-coverage behaviour: years outside the bundled calendar
  return an empty `Vec` (not an error), and `workdays_between_exclusive`
  computes plain weekday counts arithmetically while excluding
  holidays/transfer-workdays the dataset does not know about. The CLI
  date validators keep that case out of normal use. Closes logic L11
  and L12 from the 2026-05-21 review.
- `--verbose` now emits a one-off `tracing::warn!` when the count
  exceeds `-vvv` (the documented TRACE level). Previously `-vvvv` and
  beyond mapped silently to TRACE, leaving a user expecting "more than
  trace" with no signal that the level is already maxed out. The
  warning is suppressed by `RUST_LOG=error` like any other warn-level
  diagnostic. Closes cli-ux M4 from the 2026-05-21 review.
- Regression guard for the `Utc::now().with_timezone(&tz)` contract in
  `filter_agenda`. A helper `compute_today_in_tz(now_utc, tz)` was
  extracted so it can be tested with an injected "now"; eastward and
  westward midnight-crossing assertions pin the result to the local
  date rather than UTC. A future refactor that drops the timezone
  conversion (returning UTC-relative dates) now fails the unit tests
  instead of producing silently wrong agendas near midnight. Closes
  logic L6 from the 2026-05-21 review.
- README "Requirements" rewords the Rust 1.85 entry so it no longer
  reads as if the crate itself is on edition 2024. The MSRV bump is
  driven by the bundled `comrak` 0.50+ being on edition 2024;
  `markdown-org-extract` remains on edition 2021, with the migration
  tracked in `TODO.md`. Closes docs #1 from the 2026-05-21 review.
- [ADR-0013](docs/adr/0013-documentation-language.md) fixes the
  per-surface language rule that has been emerging implicitly:
  user-facing docs (`README.md`, `CHANGELOG.md`, `CLAUDE.md`,
  `TODO.md`, `docs/adr/`) are English; `docs/reviews/` may stay in
  Russian (research artefacts); Russian-locale examples in
  `examples/` are kept under ADR-0008. Closes the 2026-05-21 review
  finding "language of docs/ is mixed" -- the two Russian files it
  cited had already been deleted in the ADR migration; this ADR
  prevents the question from being re-raised.
- [ADR-0012](docs/adr/0012-verify-org-semantics-against-upstream.md)
  captures the rule the project has been following informally:
  before changing parser, agenda, repeater, or TODO-state behaviour,
  read the upstream Emacs Org-mode Elisp source rather than rely
  on general knowledge. Records the failure mode (the
  warning-period regex that captured `-Nd` but had no
  implementation behind it) and the key entry points.
  Intentional divergences from upstream must be recorded in
  ADR-0002 or a superseding ADR before shipping.
- ADR-0002 ("Supported subset of org-mode keywords") moved the
  warning-period cookie out of the "out of scope" list and into
  the supported-timestamp section now that the implementation
  exists, and replaced the absolute local path to the upstream
  Elisp checkout with the canonical Savannah URL.
- README and `--help` now disclose four implicit limits / overrides
  that previously lived only in code:
  - `--max-tasks` docstring and the README Options table note the
    built-in 10 MiB per-file size limit and the `files_skipped_size`
    counter that surfaces oversized files.
  - `--verbose` docstring and the README note that the `RUST_LOG`
    env var overrides `-v` / `-q` entirely (e.g. `RUST_LOG=error`
    mutes `-vv`).
  - `--absolute-paths` docstring and the README warn that with
    `-v`/`-vv`/`-vvv` the diagnostic stderr also logs file paths and
    timestamp content, so under `--absolute-paths` those entries
    carry absolute paths too.
  - The README "Project layout" section now describes the
    `Cargo.toml` `exclude` list (`docs/`, `.github/`, `scripts/`,
    `TODO.md`, `CHANGELOG.md`) so downstream consumers know what
    the published crate tarball does and does not ship.
- `validate_output_path` rustdoc now spells out the documented
  TOCTOU between the symlink check and `fs::write`: acceptable
  for a non-setuid CLI under an ordinary user, would need
  `O_NOFOLLOW` to close completely.
- `TODO.md` gained a new "Open info-level review notes" section
  recording info-severity items from the 2026-05-21 audit that
  were deliberately deferred (rationale included for each: tasks
  filter, `print_summary` level, `thiserror`, `O_NOFOLLOW`,
  `read_capped` file-type re-check, optional `cargo build
  --release` in CI, `TS_WARNINGS_EMITTED` reset, `file` span
  pre-filter coverage, hardcoded crate name in `release.yml`).
- README gained a "For downstream packagers" section that pins
  the contract the GitHub Release artefacts keep within a major
  version: asset naming (`markdown-org-extract-<version>-<target>.<ext>`),
  archive layout (single top-level directory; binary + README +
  LICENSE; no debug symbols, no manpages), per-archive `.sha256`
  format (`sha256sum`-compatible, verifiable with
  `sha256sum -c`), reproducibility flags
  (`tar --sort=name --owner=0 --group=0 --numeric-owner --mtime='@0'`;
  `7z -mtc=off`), compatibility floor (MSRV 1.85, Ubuntu 24.04
  glibc baseline, no runtime native deps), the stable
  GitHub-download URL pattern, and the explicit out-of-scope
  list (no signing, no distro-specific repacks). Any future
  layout change requires both a CHANGELOG entry and an ADR.

### Fixed

- Windows-zip release asset previously stored its files flat at the
  archive root because the packaging step used `7z a "$asset"
  "${stage}/*"` — 7z stripped the absolute-path prefix and the
  documented single-top-level-directory layout was lost. Replaced
  with `(cd "$RUNNER_TEMP" && 7z a "$asset" "$stem")` so the zip now
  contains `markdown-org-extract-<version>-<target>/` as the single
  root entry, matching the Linux/macOS `tar.gz` archives and the
  README "For downstream packagers" contract. Caught by the new
  `verify_rejects_flat_zip` integration test before the next
  release.
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
- `src/clock.rs` `CLOCK_RE` doc comment wrapped the literal
  `CLOCK: [timestamp]--[timestamp] => duration` in backticks so
  rustdoc no longer tries to resolve `[timestamp]` as an intra-doc
  link. Surfaced by the new `cargo doc -D warnings` CI step.
- DONE tasks with a repeater no longer surface as overdue or
  upcoming. Mirrors upstream `org-agenda.el` (lines 6424-6428):
  past-due warnings and the deadline prewarning are unconditionally
  suppressed for DONE entries; only the occurrence-day scheduled
  bucket is preserved (matches upstream's default
  `org-agenda-skip-deadline-if-done` = nil). CLOSED-typed
  timestamps are also guarded against accidentally driving overdue
  / upcoming placement -- upstream routes them through
  `org-agenda-get-progress`, not the deadline pipeline.
- `examples/org-mode-timestamps.md` line 9 used to claim the `-3d`
  cookie made the task appear three days before its deadline, but
  the parser silently dropped the cookie and the agenda ignored
  it. The example's comment now matches the actual (now-correct)
  behaviour.

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
- `AppError::Io` now carries the failing path or stream sentinel and
  exposes the underlying `io::Error` through `std::error::Error::source()`.
  Previously the variant only wrapped a bare `io::Error`, so a stderr
  message like `error: io: No such file or directory (os error 2)` did
  not tell the user *which* file. The Display now reads
  `error: io: /tmp/out.json: No such file or directory (os error 2)`.
  Internal: the blanket `From<io::Error>` was removed so every IO
  failure must go through `AppError::io(context, err)` and supply a
  caller-side label.
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

## [0.1.5] — 2025-12-06..2025-12-09

- See git history. Aggregates the work from project bootstrap through
  the v0.1.5 / v0.1.6 tag dates (the v0.1.5 tag landed on
  2025-12-07; the entry closes at the v0.1.6 cutoff on 2025-12-09).
  The range date format on this single historical entry is
  intentionally non-conformant to the strict
  `## [<version>] — YYYY-MM-DD` pattern required by
  `scripts/check-changelog.sh` — that script only validates the
  version currently being released, so the grandfathered range
  date is allowed to remain.
