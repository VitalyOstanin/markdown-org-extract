# ADR-0016: RUST_LOG environment variable overrides --verbose / --quiet

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

The CLI initialises its `tracing` subscriber via `Cli::init_tracing`
in [`src/cli.rs`](../../src/cli.rs). The current code reads:

```rust
let env_filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new(self.log_level().to_string().to_lowercase()));
```

`EnvFilter::try_from_default_env` consults the `RUST_LOG` environment
variable and falls back to the CLI-derived level only when `RUST_LOG`
is unset or unparseable. As a result, `RUST_LOG` always wins over
`--verbose` / `--quiet`: `RUST_LOG=error` mutes `-vv`, and an empty
or absent `RUST_LOG` lets `--verbose` / `--quiet` take effect.

The 2026-05-25 review flagged this as an observability finding (O1)
because the precedence was not pinned anywhere outside the
`--verbose` clap help text (`src/cli.rs:154-156`). Three options were
considered:

1. **Flip the precedence** so CLI flags always win. This would change
   the contract for users that have `RUST_LOG=info` in their shell
   profile or CI, and it would diverge from the standard
   `tracing-subscriber` idiom that documents
   `try_from_default_env` as the canonical entry point.
2. **Emit a `warn!` on conflict.** This adds a runtime self-reference
   to the logging filter (the warn itself is subject to the filter
   it is warning about), inflates stderr noise on every CI run that
   exports `RUST_LOG=info`, and does not actually change the
   effective behaviour.
3. **Pin the status quo with an ADR and a regression test.** Keep
   `RUST_LOG > CLI` as it is today, document the contract in an ADR
   the way ADR-0014 documents bracket policy, and add an integration
   test that fails if the precedence ever silently flips.

Option 3 is consistent with [ADR-0006](0006-no-registry-duplicate-guard.md)'s
spirit -- don't add a parallel mechanism for something the ecosystem
already provides -- and with the project's pattern of pinning
contractual behaviour through tests rather than runtime
self-checks.

## Decision

`RUST_LOG` takes precedence over `--verbose` and `--quiet`. The
`tracing-subscriber` `EnvFilter::try_from_default_env` call in
`Cli::init_tracing` is the canonical entry point and is not
replaced.

Specifically:

- When `RUST_LOG` is **unset** (or empty / unparseable), the
  effective level is derived from CLI flags: `--quiet` -> `error`,
  no flag -> `warn`, `-v` -> `info`, `-vv` -> `debug`, `-vvv` -> `trace`.
- When `RUST_LOG` **is set**, the CLI flags are ignored for the
  purpose of the filter. `RUST_LOG=error -vv` is silent at info
  level; `RUST_LOG=trace --quiet` is loud at trace level.

The clap help text on `--verbose` (`src/cli.rs:154-156`) already
states this in user-visible form. This ADR is the policy-level
reference.

### Regression test

`tests/cli.rs::rust_log_env_overrides_verbose_flag` pins the
contract:

- Baseline run with `-vv` and `RUST_LOG` removed from the
  environment must emit the `tracing::info!("scan finished")` line
  from `src/main.rs` on stderr.
- Same arguments with `RUST_LOG=error` must not emit that line.

A future change that flips the precedence will fail this test and
require either reverting the change or superseding this ADR.

### Out of scope

- Changing the levels mapped to `--verbose` repetitions.
- Adding per-target filtering on top of the level filter
  (`RUST_LOG` already supports that syntax via `tracing-subscriber`).
- The interaction between `RUST_LOG` and `--color` / `--no-color`
  (handled in [`src/cli.rs`](../../src/cli.rs) under different
  flags; ANSI is orthogonal to level).

## Consequences

Easier:

- The contract is mechanical and documented: a reviewer or external
  contributor reading the binary's help text or this ADR knows
  exactly which knob wins.
- The implementation stays a one-line idiomatic call to
  `try_from_default_env`; no special-case branching, no warn
  emission, no env-aware fallback chain to maintain.
- Users who set `RUST_LOG=info` in CI to debug an unrelated tool do
  not have their CLI behaviour silently inverted by adding `--quiet`
  to a wrapper script -- the env wins, as documented.

Harder:

- A user who expects `--quiet` to override an inherited
  `RUST_LOG=info` will be surprised. The clap help text mitigates
  this; the surprise is one-time per user.
- The regression test reads stderr looking for `"scan finished"`. If
  that exact phrase is ever reworded, the test must be updated in
  the same commit. The test asserts the substring rather than the
  full line specifically to keep that update small.

## References

- Current implementation:
  [`src/cli.rs:348-359`](../../src/cli.rs)
  (`Cli::init_tracing`),
  [`src/cli.rs:154-168`](../../src/cli.rs)
  (`--verbose` / `--quiet` definitions).
- Tracing emit site used by the regression test:
  [`src/main.rs:130`](../../src/main.rs)
  (`tracing::info!("scan finished")`).
- Regression test:
  [`tests/cli.rs::rust_log_env_overrides_verbose_flag`](../../tests/cli.rs).
- `tracing-subscriber` API:
  [`EnvFilter::try_from_default_env`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#method.try_from_default_env).
- 2026-05-25 review O1 finding:
  [`docs/reviews/2026-05-25-1450-review.md`](../reviews/2026-05-25-1450-review.md).
- Related ADRs:
  [ADR-0006](0006-no-registry-duplicate-guard.md) (the "don't
  duplicate ecosystem mechanisms" spirit applied here to logging
  filter configuration).
