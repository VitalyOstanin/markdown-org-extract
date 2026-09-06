# TODO

Deferred tasks that need separate sign-off or carry a substantial work
package.

## Table of contents

- [CI on the latest Ubuntu LTS](#ci-on-the-latest-ubuntu-lts)
- [Switch to edition 2024](#switch-to-edition-2024)
- [Parallel walker (rayon)](#parallel-walker-rayon)
- [Property-based and fuzz tests](#property-based-and-fuzz-tests)
- [Coverage reporting and threshold](#coverage-reporting-and-threshold)
- [Localising CLI messages](#localising-cli-messages)
- [Benchmarks (criterion)](#benchmarks-criterion)
- [Deferred performance optimisations](#deferred-performance-optimisations)
- [One grammar for every client, over WebAssembly](#one-grammar-for-every-client-over-webassembly)
- [A move is keyed by a day, and an intra-day repeater would break that](#a-move-is-keyed-by-a-day-and-an-intra-day-repeater-would-break-that)
- [A cancelled occurrence is written where a moved one is](#a-cancelled-occurrence-is-written-where-a-moved-one-is)
- [A series gains occurrences as well as losing them](#a-series-gains-occurrences-as-well-as-losing-them)
- [One prefix for everything said about an occurrence](#one-prefix-for-everything-said-about-an-occurrence)
- [Open info-level review notes](#open-info-level-review-notes)

## CI on the latest Ubuntu LTS

CI must build and test on the current Ubuntu LTS runner, not `ubuntu-latest`.
GitHub keeps `ubuntu-latest` on the previous LTS for months after a new LTS ships
(until its runner image leaves preview), so a workflow that relies on `ubuntu-latest`
is not actually exercising the latest LTS.

Action:

1. Pin the Linux runners to the current LTS image (`ubuntu-26.04` as of 2026-06,
   preview runner image) across `ci.yml` and `release.yml` (`test` matrix, `lint`,
   `msrv`, `audit`, `verify`, `publish`, `package-binaries`).
2. Keep the previous LTS (`ubuntu-24.04`) in the `test` matrix for one cycle so a
   preview-image regression does not block CI.
3. Bump the pin and drop the older LTS when a newer LTS ships.

## Switch to edition 2024

The project sets `edition = "2021"` while `rust-version = "1.85"` is
already in place (raised in 0.3.0 to take in the `comrak` 0.50+
upgrade). Edition 2024 stabilised in Rust 1.85, so the MSRV
requirement is already satisfied; only the edition flip itself is
pending.

Plan:

1. Run `cargo fix --edition` and verify the test suite stays green.
2. Bump `edition = "2024"` in `Cargo.toml`.
3. Audit any new lints introduced by the 2024 edition and address
   them.

Not done yet: separate task, deserves its own review cycle because
the 2024 edition changes capture rules in closures and a few other
lifetime/borrow defaults.

## Parallel walker (rayon)

The `ignore` crate supports a parallel walker through
`WalkBuilder::build_parallel()`. On large vaults it gives a 2–4x
speedup.

Requires:

- Passing `mappings`, `matcher`, and `stats` through `Arc` / channels.
- Collecting `tasks` through `Mutex<Vec<Task>>` or `mpsc`.

Per project rules, parallelism is not raised without explicit sign-off
from the user.

## Property-based and fuzz tests

The suite is 116 CLI tests and some 391 unit tests, nearly all of them by
example. The parts that carry the most arithmetic -- the calendar and the
repeaters -- are exactly the parts an example set covers thinnest, because a
wrong answer there needs a particular date to show itself.

Two separate pieces of work, and the first is much the cheaper.

### Properties (`proptest`)

One dev-dependency, tests run inside the ordinary `cargo test`, nothing new on
CI. The dependency is in place and `tests/properties.rs` holds the first five
properties, over the parsers of the exception keys — the newest reading surface
and the one whose input another person writes. Row 9 below is what was done;
the rest of the table is what is left, and the calendar rows are the ones the
plan calls for next. A failing case lands in
`tests/properties.proptest-regressions`, which is committed (see README,
"Properties").

Invariants worth stating:

| # | Where | Property |
|---|-------|----------|
| 1 | `get_month_grid_for_date` (`src/agenda.rs`) | the grid is whole weeks, begins on the requested `week_start`, and holds every day of the anchor month exactly once |
| 2 | `next_after_day` (`src/agenda.rs`) | the occurrence returned is strictly after the day asked about, belongs to the rule, and does not change when the same question is asked twice |
| 3 | the agenda ordering (`src/agenda.rs`, the `cmp` chain) | the order is total and antisymmetric: no three rows form a cycle |
| 4 | `extract_tasks` / `parse_heading_line` (`src/parser.rs`) | arbitrary markdown neither panics nor loses a line, and every offset reported lands inside the input |
| 5 | `compile_bounded` (`src/regex_limits.rs`) | input of any length finishes within the bound the module exists to enforce |
| 6 | `closest_date` | across all combinations of `value`, `unit`, `prefer`: `Past <= current <= Future` |
| 7 | `parse_repeater(format(...))` | round-trip |
| 8 | `add_months` | associativity |
| 9 | `parse_excluded_dates` / `parse_recurrence_id` (`src/exceptions.rs`) | **done**: every date returned reads back as one, no field is dropped in silence, one entry per date, a parsed `RECURRENCE_ID` always names a day, and a date with a time survives the round trip cut to the minute |
| 10 | `refine_entry` / `parse_phrases` (`src/phrase.rs`) | **done**: no word said is lost -- the heading is what was said, in order; the later phrase wins over the earlier one for every field it names; every value the JSON prints reads back (`%Y-%m-%d`, `%H:%M`, `parse_repeater`) |

### Fuzzing (`cargo-fuzz`)

The target is real: the tool reads markdown it did not write, from files whose
names are arbitrary non-NUL bytes on Linux (`src/scan.rs`, `src/types.rs`
already say so). Worth pointing at `extract_tasks`, the timestamp parsing and
`refine_entry` -- the last reads a sentence a person dictated, which arrives
from the CLI and from the Android client alike -- not at the CLI as a whole.

What it costs, and what it will not find: a nightly toolchain, a `fuzz/`
directory with a manifest of its own (outside the workspace), a corpus to keep
somewhere, and a run long enough to be worth starting -- so a scheduled job or
a manual one, not every push. Safe Rust with bounds checks rules out the whole
class of findings fuzzing is run for in C; what is left to catch here is
panics, overflow in the date arithmetic, and inputs that take pathologically
long.

Order: properties first (they cover the calendar and the repeaters, where the
mistakes have actually been), fuzzing second and only for the parser.

## Coverage reporting and threshold

No coverage tooling is wired up yet. Adding a report and a minimum threshold
needs a coverage tool (`cargo-llvm-cov` or `cargo-tarpaulin`), which is not
installed. Set it up in CI (installed on the runner, not on the dev host) and
enforce a floor at the measured level so regressions fail the build.

## Localising CLI messages

The CLI targets an RU locale (RF holidays, `--locale ru,en`), but
every message and `--help` string is in English. Options:

1. Translate all messages into Russian (breaks pipelines that grep
   for English text).
2. Bilingual messages switched via `LANG` / `LC_ALL`.
3. Leave as is.

## Benchmarks (criterion)

Areas:

- `extract_tasks` on large markdown inputs.
- `build_week_agenda` / `build_day_agenda` with many repeating tasks.
- `closest_date` across different `unit` values.

Directory `benches/`, with `criterion` as a dev-dependency.

## Deferred performance optimisations

Remaining micro-optimisations from the performance review in
`docs/reviews/2026-05-25-1450-review.md` (INFO-4). The two highest-value
items it listed — caching the Aho-Corasick weekday engine and removing
the double weekday normalisation in `finalize_task` — already shipped
(see the `### Performance` block in the `[Unreleased]` CHANGELOG). The
review's own guidance applies to everything below: **benchmark a typical
notes tree first** (`--agenda month` over ~1000 timestamped files,
wall-time + `perf record`); without numbers the ordering is a guess, and
none of these is worth a behavioural or API risk taken blind.

- **Single-regex dispatch in `parse_org_timestamp` (priority 3)** —
  `parse_org_timestamp` runs both the `<…>` and `[…]` single-timestamp
  regexes. A `memchr`/`find` probe for the first `<` vs `[` could pick
  one. Medium expected benefit, but it sits in the hot parse path and
  changing it touches org-bracket semantics (ADR-0012), so it needs a
  benchmark and full timestamp-matrix tests before it is worth the risk.
- **`Arc<Task>` (or index refactor) in `TaskWithOffset` (priority 4)** —
  week / month agendas clone `Task` into per-day buckets. Sharing via
  `Arc<Task>` would cut clones on large trees but changes the internal
  `TaskWithOffset` API and the agenda builders. Defer until a benchmark
  shows the clone cost matters.
- **`String::with_capacity` for render outputs (priority 5)** —
  `render_days` / `render_tasks` (`src/render.rs`) grow the output
  string from empty. Pre-sizing from a task-count estimate would save
  reallocations on a busy month agenda. Behaviour-neutral but small;
  fold into the first render-path benchmark.
- **`get_holidays_for_year` via `partition_point` (priority 6)** —
  `src/holidays.rs` filters the sorted holiday list linearly. A
  `partition_point` on the year would make it O(log N + range). Called
  once per `--holidays`, so the practical win is negligible; listed for
  completeness.
- **`extract_clocks` owned-string allocations (perf 3.6)** — each
  `ClockEntry` field is an owned `String`. Lowering to `Cow<str>` or
  source-text indices would cut allocations on clock-heavy files, but
  the whole `Task -> JSON` pipeline is built on owned strings, so this
  is a redesign, not a tweak.
- **Compact JSON for machine consumers (perf 4.5)** — output is always
  `to_string_pretty`. A compact `to_string` would be smaller/faster for
  a pure machine parser, but pretty output is read by humans and is what
  the JSON wire-contract snapshot tests pin, so switching the default is
  a UX trade-off that needs sign-off, not a free win. A `--compact`
  opt-in flag would be the non-breaking route if a consumer asks.

## One grammar for every client, over WebAssembly

**Investigate first, then agree the implementation.** Nothing is decided
here beyond the problem being worth solving.

The format is read by three grammars today. This crate parses a heading in
two steps (`HEADING_TODO_RE`, then `HEADING_PRIORITY_RE` anywhere in the
remainder) and a timestamp as a date plus a free-form bracket body scanned
for a repeater and a warning cookie. The Android application calls that code
directly. The VS Code extension has its own positional regexes
(`HEADING_REGEX` in `src/orgPatterns.ts`, `TIMESTAMP_REGEX` in
`src/utils/timestampParts.ts`), and they disagree with this crate on lines
users actually write:

| № | Line | Here | VS Code |
|---|------|------|---------|
| 1 | `## TODO Написать [#A] отчёт`      | priority `A`         | no priority; the cookie stays in the title and a second one gets appended |
| 2 | `## TODO [#A]Написать отчёт`        | priority `A`         | no priority |
| 3 | `` `SCHEDULED: <2026-01-12 Пн +1w -2d>` `` | date moves, tokens kept | not recognised at all; the timestamp commands do nothing |

Publishing the regexes alone would not fix this: what diverges is the order
the patterns are applied in and the way the bracket body is scanned, neither
of which a pattern string carries. A shared conformance corpus would catch a
divergence but not prevent one.

Compiling this crate to `wasm32` and having the extension call it removes
the second grammar instead of synchronising it. Two things make it closer
than it sounds: `parse_heading_line` and `parse_timestamp_parts` already
return byte ranges for every token, which is exactly what the cursor-aware
commands in the extension compute by hand today; and the same code is
already reached from another language through UniFFI on Android.

What the investigation has to establish:

| № | Question                                                                 |
|---|--------------------------------------------------------------------------|
| 1 | What the facade is: `ignore`, `grep-searcher`, `signal-hook` and `clap` do not build for `wasm32`, so string parsing has to be separable from directory walking behind a feature |
| 2 | Size of the resulting module, with and without `comrak` (needed only by `display_text`) |
| 3 | How the extension loads it — bundled in the vsix, or fetched like the binary is today — and what that means for the web build of VS Code, where a native binary cannot run at all |
| 4 | Which extension code moves: the `HEADING_REGEX` consumers and the two callers of `getTimestampPartAt` |
| 5 | How the module is versioned against the crate, replacing the per-platform SHA256 table the extension pins today |

## A move is keyed by a day, and an intra-day repeater would break that

A `MOVED` line names the occurrence it moves by the day the series draws it on
(ADR-0038), and one entry refuses a second line naming the same day. That is
sound only while a series has at most one occurrence per day, which is what the
agenda offers today: an `+Nh` repeater is projected onto the day grid and `N` is
ignored, so every day counts as one occurrence (`timestamp/repeater.rs`,
`RepeaterUnit::Hour`; README "Repeaters").

Should hour repeaters ever be expanded within a day — an agenda that draws
"every 2 hours" as several cells of one day — a day would no longer say which
occurrence a line means, and the key would have to become a day and an hour.
That is a breaking change to the written format, not an additive one: files
already carrying day-keyed lines would have to keep being read.

Action: decide it in a new ADR, superseding the key half of ADR-0038, before any
work on intra-day expansion starts. The tests to look at first are
`parser::tests::an_occurrence_moved_twice_by_one_entry_keeps_the_first_move` and
the agenda's `push_moved_occurrence`.

## A cancelled occurrence is written where a moved one is

The two answers about one occurrence of a series are written in two places and
in two notations. A move is a planning line of the entry, with the occurrence
as an inactive timestamp; a cancellation is a date in the `EXDATE` property,
written bare:

````text
## English on Mondays at 15:00
`SCHEDULED: <2026-08-06 Thu 15:00 +1w>`
`MOVED: [2026-08-20 Thu] -> <2026-08-27 Thu 18:00>`
```org-properties
EXDATE: 2026-08-13
```
````

Both lines say something about one occurrence, and a reader has to know two
forms to write either. The asymmetry is not visible from the file: nothing
explains why one of them is a planning line and the other a property.

The symmetry could be restored the other way round — a cancellation written as
a planning line of the series, with the occurrence in the same inactive form a
move addresses it with:

````text
`CANCELLED: [2026-08-13 Thu]`
````

What that would buy: one place to look, one notation for the occurrence, and a
date the clients already step with the keys and controls they offer for
timestamps — which is the argument that put the move on a planning line
(ADR-0038). What it costs, and what has to be decided before any of it:

1. `CANCELLED` is already a task keyword of these notes (a heading reads
   `## CANCELLED Abandoned task`). The same word in two roles — the state of an
   entry and an exception of a series — is a collision to resolve rather than
   an oversight to fix: either a different word, or a rule that reads the two
   apart by position.
2. `EXDATE` is fixed by ADR-0020 as a property and by ADR-0031 as the way a
   cancellation is written. Files already hold it, this project writes it, and
   the extension and the Android client read it. Any new form is read
   alongside the old one indefinitely, exactly as the bare `MOVED` address is.
3. A cancellation carries no target, so a planning line for it is a keyword and
   an address and nothing else — which is a thinner line than any other
   planning line the format has. Whether that reads as a line or as noise is
   the question the form has to answer.
4. `EXDATE` is what iCalendar calls it, and the property maps onto RFC 5545
   without translation. A planning line does not, and the Google Calendar
   export of the extension reads the property today.

Nothing here is urgent: both forms work and both are read. This is a note that
the format has an asymmetry with a known cost, so the question is not
rediscovered from the files a third time.

## A series gains occurrences as well as losing them

A series answers two questions about one of its occurrences today: it moved
(`MOVED`) and it is gone (`EXDATE`). Real series ask a third. A course of
English lessons every Thursday is rescheduled, cancelled for a week — and an
extra lesson is arranged, on a day the series does not fall on, before an exam
or to make up for one that was missed. There is nothing to write that with.

What the reader is left to do instead is write a separate entry. That is the
shape ADR-0031 used for a move and ADR-0038 moved away from, and it costs the
same here: a second heading with the same title, standing wherever the file
puts it, which the reader has to keep in mind when editing either. The extra
lesson is not a different thing from the course — it is the course, on one more
day.

iCalendar has the pair: `RDATE` adds a date to a series as `EXDATE` takes one
away. A form that follows the notes rather than the RFC would be a planning
line beside the others, naming a whole timestamp because an added occurrence
has an hour of its own:

````text
## TODO English on Thursdays at 15:00
`SCHEDULED: <2026-08-06 Thu 15:00 +1w>`
`ADDED: <2026-08-25 Tue 18:00>`
````

Questions to settle before any of it:

1. **Which form.** A property (`RDATE`, list of dates, the `EXDATE` shape and
   the iCalendar name) or a planning line (an editable timestamp, an hour of
   its own, the `MOVED` shape). The same asymmetry the note above is about,
   and the two should be decided together rather than one at a time.
2. **What an added occurrence inherits.** The keyword, the priority, the body,
   the clocks — all of the series, as a moved occurrence does. What it cannot
   inherit is the hour, since a lesson added before an exam is usually held at
   another one.
3. **How it interacts with the other two.** An added occurrence can later be
   moved or cancelled, and whether the three operations compose or have to be
   special-cased against each other turns on whether `MOVED` and `EXDATE` may
   name a day the series does not fall on. Half of that is now answered:
   ADR-0040 refuses a `MOVED` line addressing such a day, precisely so that
   adding an occurrence stays an operation this format has not decided on.
   Whatever form adding takes, it has to say how a move addresses what it
   added — the days a series falls on will no longer be the whole answer.
4. **What it owes.** ADR-0032 says what a missing occurrence owes; an added one
   that has gone by unfinished is arrears the same way, and the day it is
   counted from is its own rather than the series'.
5. **What the clients offer.** The sheet of a repeating row today offers "move"
   and "cancel" about the day the row is drawn on. Adding is not about a day
   the series falls on, so it is asked for from somewhere else — the entry, not
   the occurrence — and needs its own place in both clients.
6. **The Google Calendar export.** `RDATE` maps onto RFC 5545; a planning line
   does not, and the extension's export would translate it.

Not urgent, and larger than the note above: this is a third operation on the
model rather than a change of notation for an existing one.

## One prefix for everything said about an occurrence

The two notes above are about the form of single operations. A third question
is about the set of them. Everything a series says about one of its occurrences
could carry a common prefix, so that the family is visible as a family:

````text
## TODO English on Thursdays at 15:00
`SCHEDULED: <2026-08-06 Thu 15:00 +1w>`
`OCCURRENCE_MOVED: [2026-08-20 Thu] -> <2026-08-27 Thu 18:00>`
`OCCURRENCE_CANCELLED: [2026-08-13 Thu]`
`OCCURRENCE_ADDED: <2026-08-25 Tue 18:00>`
````

Written with an underscore rather than a hyphen: ADR-0020 fixes keys as
`UPPER_SNAKE_CASE`, and a hyphen in one key while every other key of these
notes uses an underscore is a second convention for no gain.

What the prefix buys:

1. A reader who meets one of the three can guess the other two, and a reader
   who meets none of them can tell at a glance that the line is about one
   occurrence rather than about the series.
2. It is a namespace, which settles the collision the `CANCELLED` note runs
   into: `CANCELLED` alone is already a task keyword of these notes, while
   `OCCURRENCE_CANCELLED` cannot be mistaken for the state of an entry.
3. A fourth operation, whatever it turns out to be, has a place to go and a
   name that reads like the ones before it.
4. Every line of the family is found by one search, in a file or across the
   notes, and a client can route them by prefix rather than by a list of
   keywords it has to keep in step with the extractor.

What it costs:

1. `MOVED` is written today, by this project and by both clients, and files
   hold it. A rename is read alongside the old form indefinitely — the bare
   address of ADR-0038 is already carried that way — and every client rewrites
   what it touches. The gain has to be worth two spellings of one thing.
2. The lines get long. `OCCURRENCE_MOVED: [2026-08-20 Thu] -> <2026-08-27 Thu
   18:00>` is 60 characters before the entry is even named, and these are
   inline-code spans in a Markdown file a person reads.
3. The prefix repeats what the line already says: an address in inactive
   brackets followed by an arrow is not a statement about the series under any
   reading. Whether the repetition is redundancy or signposting is exactly what
   has to be decided.

Decide this together with the two notes above: whether a cancellation moves to
a planning line, and what an addition is written as, are the same question
asked about three operations, and answering them one at a time is how the
format ends up with three conventions.

## Open info-level review notes

Items from the `docs/reviews/` audit rounds
(`2026-05-21-1811-review.md` and `2026-05-25-1450-review.md`) that
were deliberately deferred at their close. Each is non-blocking,
info-severity, and recorded here so the rationale does not get lost.

- **`tasks` mode filters only `TaskType::Todo` (logic i2)** — the
  README explicitly says "tasks whose state is TODO", so the default
  filtering is documented behaviour, not a defect. The "show DONE in
  the flat list" request (Google Calendar sync needs completed tasks
  to delete their events) shipped as the opt-in `--tasks-include-done`
  flag; the TODO-only default is unchanged. Resolved.
- **`print_summary` direction (logic i1)** — the per-run summary
  uses `tracing::warn!`. This is gated behind
  `stats.has_warnings()` so the warn level is honest. If the CLI
  ever grows an always-on summary (like `rg`/`fd` print on `-v`),
  flip the summary to `info!` and keep `warn!` for the per-file
  failure lines.
- **Switch to `thiserror` (error-handling I01)** — `AppError`'s
  hand-rolled `Display` / `From` impls are fine for the current 5
  variants. Reconsider when a sixth variant or a structured
  context field (e.g. failing path on more variants) appears: the
  derive saves real code at that point.
- **`O_NOFOLLOW` on `--output` open (error-handling I02; SEC-2)** —
  the TOCTOU window between `validate_output_path` and `fs::write`
  is documented in the function comment. Closing it needs an
  `OpenOptions` path with `O_NOFOLLOW` (Unix-only) and a fallback
  on Windows. Defer until the CLI runs in a context where the
  attacker does not already own the target directory. The
  2026-05-25 security review (SEC-2, info) re-confirmed this is the
  same window and that the non-setuid user-level CLI threat model
  (cf. `cp` / `mv` / `tee`, none of which fight TOCTOU without
  `O_NOFOLLOW`) makes the deferral correct; a reviewer re-raising
  it closes with a pointer here.
- **`read_capped` file-type re-check (error-handling I03)** — the
  walker filters by `is_file()` and `read_capped_into` caps the
  read at `MAX_FILE_SIZE + 1`, so a FIFO/named pipe replacement
  between walk and open would still terminate; but `read_to_end`
  may stall up to that cap. A `metadata().file_type().is_file()`
  check after `File::open` would close the stall window cheaply.
- **`cargo build --release` in CI (infra-ci-tests info-2)** — only
  the release-tag workflow exercises the LTO + codegen-units=1
  profile. Adding a non-blocking release build to `ci.yml` (Linux
  only) would catch optimizer-only regressions earlier. Worth
  doing when the next "optimised-only" bug surfaces, not before.
- **`file` span pre-filtering coverage (observability INFO-6)** —
  `tracing::debug_span!("file", ...)` wraps `extract_tasks` only.
  If per-file debug events ever land in the pre-filter phase
  (e.g. "skipped by glob"), pull span creation out to the walker
  iteration instead of inside the processing call.
- **Crate name pinned in `release.yml` awk (config Info-3)** —
  `release.yml`'s `Cargo.lock` parser hard-codes
  `name = "markdown-org-extract"`. A rename would make the awk
  silently produce an empty version and fail later with a
  confusing message. Not a real risk for an already-published
  crates.io name, but worth a follow-up grep if a rename ever
  happens.
- **Split `tests/cli.rs` by theme (tests i5)** — the integration
  suite is one ~1800-line file. Splitting it into
  `tests/cli_help.rs`, `tests/cli_output.rs`,
  `tests/cli_agenda_window.rs`, `tests/cli_exit_codes.rs` would ease
  navigation, but each `tests/*.rs` is a separate crate, so the
  shared `bin()` helper and fixtures must be lifted into a
  `tests/common/mod.rs` first. Long-term tech debt, not blocking;
  do it when the file next grows enough to slow a search.
- **Pre-tracing error format differs from tracing (observability O2)**
  — hard errors before `Cli::parse()` (`install_signal_handlers`
  failure) and the final `run()` error print via `eprintln!`
  (`error: <msg>`), while everything after `init_tracing` uses the
  `tracing-subscriber` fmt layout. This is deliberate — a hard error
  must reach the user even under `--quiet`, before any subscriber
  exists — and is an accepted CLI-architecture trait, not a defect.
  Revisit only if stderr ever needs a single machine-parseable shape
  end to end.
- **Structured `kind`/`category` event field (observability O7)** —
  events are classified only by message text today
  (`cannot parse timestamp`, `walker entry failed`, the summary).
  A stable tag (`kind = "parse.invalid_timestamp"`,
  `"scan.walker_error"`, `"scan.summary"`) would let a consumer
  classify stderr without matching prose. Not needed while stderr is
  read by humans / CI; add it when `markdown-org-vscode` (or another
  consumer) starts parsing the CLI's stderr for diagnostics.
- **A ceiling on the window width (agenda review, 2026-08-17)** —
  `--from`/`--to` accept any range inside the validated year bounds,
  so `--from 1900-01-01 --to 2100-12-31` materialises ~73 000
  `DayAgenda` values, each of which walks every task. Nothing
  malicious about it — a client that computes its window arithmetically
  can produce one by accident — but the run is minutes long with no
  diagnostic. A cap (or a warning past N days) needs a number that does
  not cut off a legitimate multi-year calendar, which is why it is
  deferred rather than picked here.
- **One constant for the `%Y-%m-%d` format string (agenda review,
  2026-08-17)** — the literal appears in a dozen places across
  `agenda.rs`, `parser.rs` and `timestamp/`. A shared `const` would
  make the wire format greppable, but the literal is also the format
  chrono's own docs use, and a constant hides which calls parse input
  versus format output. Do it alongside the next change that touches
  the date format itself.
- **`n2` from one bracket instead of a second `closest_date`
  (agenda review, 2026-08-17)** — a scheduled cell computes its
  occurrence with `closest_date` and then the occurrence after it with
  `next_occurrence`, so the repeater grid is bracketed twice per drawn
  cell (twice more against the holiday calendar for `+Nwd`). Returning
  the pair `(n1, n2)` from one bracket would halve that. Deferred: the
  saving is ~0.25 µs per cell, while `closest_date` has two documented
  short-circuits before the bracket is built (`current == base_date`,
  `current < base_date`) that a pair-returning API would have to
  reproduce exactly — a behavioural risk out of proportion to the win.
  Revisit if a benchmark ever shows the bracket in the profile.
- **Signing release tags (release review, 2026-08-17)** — tags are
  annotated but not signed, so the tag body is verified by CI
  (`release-verify-tag-body.sh`) while its provenance is not.
  Signing needs a key policy for the release workflow first; until
  then the published artefact is trusted through the crates.io token,
  as it already is.
