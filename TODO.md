# TODO

Deferred tasks that need separate sign-off or carry a substantial work
package.

## Table of contents

- [Switch to edition 2024](#switch-to-edition-2024)
- [Parallel walker (rayon)](#parallel-walker-rayon)
- [Property-based and fuzz tests](#property-based-and-fuzz-tests)
- [Localising CLI messages](#localising-cli-messages)
- [Benchmarks (criterion)](#benchmarks-criterion)
- [Open info-level review notes](#open-info-level-review-notes)

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

Risk areas:

- `closest_date` across all combinations of `value`, `unit`, and
  `prefer` — invariant `Past <= current <= Future`.
- `parse_repeater(format(...))` round-trip.
- `add_months` associativity.

Tools: `proptest` or `quickcheck`. `cargo-fuzz` for the regexes under
`timestamp/*`.

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

## Open info-level review notes

Items from the `docs/reviews/` audit rounds
(`2026-05-21-1811-review.md` and `2026-05-25-1450-review.md`) that
were deliberately deferred at their close. Each is non-blocking,
info-severity, and recorded here so the rationale does not get lost.

- **`tasks` mode filters only `TaskType::Todo` (logic i2)** — the
  README explicitly says "tasks whose state is TODO", so the
  filtering is documented behaviour, not a defect. Revisit if a
  request appears for a "show DONE in the flat list" mode (could
  be added as `--tasks-include-done`).
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
