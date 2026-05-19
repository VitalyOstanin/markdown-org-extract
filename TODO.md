# TODO

Deferred tasks that need separate sign-off or carry a substantial work
package.

## Table of contents

- [Switch to edition 2024](#switch-to-edition-2024)
- [Parallel walker (rayon)](#parallel-walker-rayon)
- [Property-based and fuzz tests](#property-based-and-fuzz-tests)
- [Localising CLI messages](#localising-cli-messages)
- [Benchmarks (criterion)](#benchmarks-criterion)

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
