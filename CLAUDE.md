# Project rules for Claude Code

## TDD: tests on every change

Every code change MUST be accompanied by tests. Not perfunctory ones —
tests that exercise actual behaviour:

1. If the affected behaviour had no tests yet, add them under TDD
   (red → green → refactor): first a failing test, then the smallest
   code change that makes it pass, then refactoring if needed.
2. For bug fixes, first write a test that reproduces the bug (red) and
   only then write the fix (green). A test without a fix proves the
   bug existed; a fix without a test does not prove the bug will not
   return.
3. For CLI flags and validation, cover both the golden path and the
   conflicts/error cases through `assert_cmd` in `tests/cli.rs`.
4. For the parser, agenda, formatting, and other logic — unit tests
   in `#[cfg(test)] mod tests` next to the code, plus snapshot tests
   on byte-exact output where appropriate.
5. Tests must exercise behaviour with concrete inputs and concrete
   expected outputs, not merely check that the code compiles or that
   a function exists.
6. How to run: `cargo test`. Before closing a task, run the full
   suite green, not just the new tests.

Precedent: task 021 (cli-ux conflicts for `--holidays` and `--from` /
`--to`) was initially closed without integration tests on
`conflicts_with`. The conflict declarations could have silently broken
under any later clap-argument refactor. The tests were added after
the fact at the user's request.

## Community-facing documentation: not needed yet

The project does not yet have an external contributor community, so
the standard open-source meta-documentation (such as `CONTRIBUTING.md`,
`CODE_OF_CONDUCT.md`, `SECURITY.md`, issue/PR templates,
`ISSUE_TEMPLATE/`, `PULL_REQUEST_TEMPLATE.md`) is neither needed nor
to be created without an explicit request from the user:

1. Such files go stale quickly without a steady flow of external
   contributors to force them to stay current. Precedent: the removed
   `CONTRIBUTING.md` described the release process incompletely and
   in places incorrectly — the workflow moved on, the document did
   not.
2. Project conventions (TDD, code style, releases) live in the
   project-level `CLAUDE.md` and in the workflow itself
   (`.github/workflows/`). That is the single source of truth for the
   agent and for current developers.
3. If an external community appears in the future, the
   meta-documentation should be created anew from the actual current
   workflow, not restored from the deleted version.

It follows that we do not propose adding `CONTRIBUTING.md` /
`CODE_OF_CONDUCT.md` / issue-PR templates, and we do not close tasks
of the form "add CONTRIBUTING" without an explicit user request.

## Do not duplicate protections already provided by the registry

crates.io treats published versions as immutable: `cargo publish` for
an already-existing `name@version` is rejected by the registry
server-side (step 5 of 5 in the `cargo publish` flow). Therefore we
do NOT add a separate "version already published" check with
skip-following-steps logic to the workflow:

1. The native error `crate version 'X.Y.Z' is already uploaded` is
   clear and unambiguous; a separate check only complicates the YAML.
2. A custom check against the crates.io API can yield a false
   positive on 5xx responses or network failures and block a valid
   release.
3. A "skip + warning" implementation produces a false success: a
   green workflow on a failed release attempt. The user believes the
   release succeeded.
4. Saving 5–10 minutes of CI on erroneous runs is not worth the risk
   of a falsely-green release.

The same rule applies to analogous protections in other registries
(npm, PyPI, Docker Hub): if the registry itself rejects duplicates,
do not duplicate the check on the CI side.

Precedent: task 024 proposed adding a crates.io API probe before
`cargo publish`. After reading the Cargo Book and inspecting how
`cargo publish` itself behaves
(<https://doc.rust-lang.org/cargo/reference/publishing.html>) we
decided against it — crates.io rejects duplicates with a clear error
of its own.

## Do not put test counts in the README

Do not state test counts in the README/documentation in forms such as
"(N tests)", "12 tests covering X", and similar. Describe test
coverage with a bulleted "what is covered" list and no numeric
counters:

1. Numbers go stale fast: every new task adds tests, and the block in
   the README turns into update debt without giving the reader
   anything in return.
2. In this project the README has already drifted from reality: it
   used to say "(9 tests)" / "(6 tests)" / "(2 tests)" while the real
   counts at the time of task 039 were 11 / 24 / 3.
3. A "what is covered" list without numbers stays true as the suite
   grows, and the reader can run `cargo test` to see the current count.

Precedent: task 039 (cli-ux/docs) proposed refreshing the outdated
test counts in the README. The user chose to remove the counts
entirely as "childish posturing" instead of updating them at every
subsequent task.

## Default-value localisation: the author chose an RF context

The project's default values that are tied to the RF context are
deliberate author choices. Do NOT propose changing them without an
explicit request from the user:

1. `--tz` defaulting to `Europe/Moscow` is a deliberate choice for
   the primary target audience. Do not propose switching to
   `local`/`UTC`, do not treat it as a cli-ux bug.
2. Bundled holidays in `holidays_ru.json` form the RF calendar. Do
   not propose calendar localisation or moving the calendar behind a
   separate flag without a request.
3. `--locale ru,en` by default is the same story: the default for the
   expected audience.

It follows that reviewer tasks of the form "default is non-obvious
for an English-speaking audience" / "TZ is hard-wired to RF" should
be closed automatically with a reference to this rule, not
implemented.

Precedent: task 050 (cli-ux) proposed changing the `--tz` default to
`local` or UTC. The user cancelled it — the default is the author's
choice and is not subject to change.
