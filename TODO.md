# TODO

Deferred tasks that need separate sign-off or carry a substantial work
package.

## Table of contents

- [Switch to edition 2024](#switch-to-edition-2024)
- [Parallel walker (rayon)](#parallel-walker-rayon)
- [Property-based and fuzz tests](#property-based-and-fuzz-tests)
- [Localising CLI messages](#localising-cli-messages)
- [Benchmarks (criterion)](#benchmarks-criterion)
- [Active and inactive timestamps: square brackets across all keywords](#active-and-inactive-timestamps-square-brackets-across-all-keywords)
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

## Active and inactive timestamps: square brackets across all keywords

Emacs Org-mode distinguishes **active** timestamps `<2026-05-25 Mon>`
(land in agenda, drive scheduling) from **inactive** ones
`[2026-05-25 Mon]` (purely descriptive, never appear in agenda).
Today this project accepts only the angle-bracket form for every
keyword: `SCHEDULED:`, `DEADLINE:`, `CLOSED:`, `CREATED:` and bare
inline timestamps. ADR-0002 explicitly lists "Inactive timestamps in
square brackets `[...]` outside of CLOCK and CLOSED contexts" as out
of scope, and in practice even `CLOSED: [...]` is rejected by the
current regex set (`CLOSED:\s*<...>` only). The downstream editor
(`markdown-org-vscode`) wants to align with upstream Emacs Org and
support both forms across all keywords plus inline.

This is an ADR-level decision because it changes the
producer/consumer contract and breaks the simplification recorded in
ADR-0002.

### Required changes in this crate

1. Regex updates in `src/timestamp/extract.rs`:
   - `TIMESTAMP_RE`: accept either `<...>` or `[...]` for
     SCHEDULED/DEADLINE/CLOSED and capture which form was used.
   - `CREATED_RE`: same for CREATED.
   - `SIMPLE_TIMESTAMP_RE` and `RANGE_TIMESTAMP_RE`: same for bare
     timestamps (inline use case).
   - Closing-bracket must match the opening one (no mixed
     `<2026-05-25]`); this likely needs paired alternation rather
     than `[<\[]...[>\]]` since the latter accepts mismatched pairs.
2. Parser surface in `src/timestamp/parser.rs`:
   - `SINGLE_RE` mirrors the bracketing decision.
3. Data model: extend the `Timestamp` type (see `src/timestamp.rs`
   and `src/timestamp/parser.rs`) with an `active: bool` flag (or a
   `Bracket::Active | Bracket::Inactive` enum). All consumers --
   `extract_tasks`, `build_*_agenda`, JSON output -- must propagate
   it so downstream tools can filter on it.
4. Agenda semantics:
   - Inactive timestamps must NOT participate in agenda windows
     (`build_week_agenda`, `build_day_agenda`, "tasks" mode that
     pulls SCHEDULED/DEADLINE). The agenda-eligible filter mirrors
     Emacs' `org-ts-regexp` (active only).
   - `CLOSED:` is by convention inactive in Emacs but never feeds
     agenda regardless; the flag just lets consumers display it.
   - `CREATED:` is by convention inactive; same treatment.
5. JSON output: expose the bracket form so the editor can preserve
   it on round-trip. Versioning of the JSON schema may be needed
   (consider whether existing consumers tolerate a new field).
6. CLOCK: already accepts both forms (`src/clock.rs`, `CLOCK_RE`).
   Audit whether the `active` flag needs to surface for CLOCK
   entries too, or whether the existing dual matching is enough.
7. Tests:
   - Unit tests under `tests/` (or wherever timestamp tests live)
     covering every keyword × both forms.
   - Round-trip tests: parse `[CLOSED: [...]]`, serialise, parse
     again -- form is preserved.
   - Agenda invariant tests: `[SCHEDULED: [...]]` is **not** picked
     up by agenda windows; `<SCHEDULED: <...>>` is. Wait,
     `SCHEDULED:` with inactive brackets is unusual in Emacs --
     verify in `/home/vyt/devel/org-mode/lisp/org.el` whether
     `org-scheduled-time-regexp` matches `[...]` at all; if not,
     decide whether this crate is stricter than Emacs (recommend:
     follow Emacs and accept only `<...>` for SCHEDULED/DEADLINE,
     accept both for CLOSED/CREATED/inline; document in the new
     ADR).

### ADR work (do NOT skip)

`docs/adr/0002-supported-org-mode-subset.md` currently lists
inactive `[...]` as out of scope. That decision is reversed (in
part). Pick one of:

- New ADR `docs/adr/0014-active-and-inactive-timestamps.md`,
  amending ADR-0002 in the Status section ("Amends ADR-0002").
- Or amend ADR-0002 in place with a dated note and bump its
  status to "Amended by ADR-0014".

ADR-0002 explicitly says (Consequences section): "Adding a new
Org-mode form requires an explicit decision (new ADR or amendment)
rather than silently accumulating syntax." Honour that.

The new ADR must cover:

- Which keywords accept which bracket forms (table, with
  cross-reference to the Emacs regex constants that justify each
  row -- see `org-deadline-time-regexp`, `org-scheduled-time-regexp`,
  `org-closed-time-regexp` in `lisp/org.el`).
- Agenda semantics: inactive timestamps never drive agenda.
- Round-trip guarantee: the form a producer wrote is the form a
  consumer reads back.
- Migration impact: old consumers that have a hard `<...>` regex
  will silently miss `[...]` lines -- call out the JSON schema
  field that signals which form was matched, and recommend
  consumers grep that field rather than re-parsing the timestamp
  string.

### Out of scope for this task

- Custom TODO state sequences via `#+TODO:` directives (still out
  of scope per ADR-0002, no change).
- Properties drawers and `:CREATED:` inside them (separate task).
- Date-range timestamps across the active/inactive boundary
  (`<a>--[b]` etc.) -- Emacs does not produce these; we don't need
  to either.

### Release & coordination

- Bump version to a new minor (e.g. 0.5.0). Mention the new ADR in
  CHANGELOG.
- Coordinate with `markdown-org-vscode`: after release, that
  project bumps `x-markdown-org.extractorVersion` in its
  `package.json` and adopts the new JSON schema field.

### References

- Emacs Org regexes for keyword variants:
  `lisp/org.el:547` (deadline), `:563` (scheduled), `:572` (closed)
  in `/home/vyt/devel/org-mode`.
- Emacs Org regexes for plain timestamps: `lisp/org.el:425-432`
  (`org-ts-regexp`, `org-ts-regexp-inactive`, `org-ts-regexp-both`).
- `org-toggle-timestamp-type` in `lisp/org.el:15510` for the
  precedent on switching forms in place.
- Consumer-side notes:
  `markdown-org-vscode/TODO.md` (square-bracket section, to be
  added in the same work cycle).

## Open info-level review notes

Items from `docs/reviews/2026-05-21-1811-review.md` that were
deliberately deferred at the close of the audit round. Each is
non-blocking, info-severity, and recorded here so the rationale
does not get lost.

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
- **`O_NOFOLLOW` on `--output` open (error-handling I02)** — the
  TOCTOU window between `validate_output_path` and `fs::write` is
  documented in the function comment. Closing it needs an
  `OpenOptions` path with `O_NOFOLLOW` (Unix-only) and a fallback
  on Windows. Defer until the CLI runs in a context where the
  attacker does not already own the target directory.
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
- **`TS_WARNINGS_EMITTED` global counter (observability INFO-4)** —
  the static `AtomicUsize` is shared process-wide. Fine for a
  one-shot CLI; matters if the parser is ever lifted into a
  `[lib]` target. Replace with a counter threaded through
  `extract_tasks` at that point.
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
