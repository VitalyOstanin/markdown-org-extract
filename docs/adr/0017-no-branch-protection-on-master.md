# ADR-0017: No branch protection on master; pre-commit hook policy

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

The 2026-05-25 infra-CI-tests review (F1) flagged that
`master` has no branch protection rule (`gh api
repos/VitalyOstanin/markdown-org-extract/branches/master/protection`
returns 404). The cited precedent is the 0.5.0 release cycle, where
`cargo fmt --check` failed on CI after the release commit was pushed
and the maintainer had to fix forward in `master`.

GitHub's branch-protection feature offers two modes that fit a
solo-maintained repository:

1. **`enforce_admins=false` + `required_status_checks`** -- the
   admin (the repository owner, in this case) bypasses the rule.
   The protection only blocks force-push and branch deletion;
   direct push by the owner still works regardless of CI state, so
   the 0.5.0 fmt-fail-after-push regression would not have been
   prevented.
2. **`enforce_admins=true` + `required_status_checks`** -- the
   admin is also forced through pull requests. Every change
   (including documentation, release commits, and small fixes) has
   to be made on a branch, pushed, opened as a PR, wait for CI to
   pass (currently 5-10 minutes), then merged and the branch
   deleted. For a single-maintainer repository with no external
   contributors and no parallel work, this is a substantial
   workflow tax for a problem with a cheaper, local solution.

The cheaper local solution -- already in the repository -- is
`scripts/install-hooks.sh`, which installs a git `pre-commit` hook
that delegates to `scripts/check.sh`. The hook runs
`cargo fmt --check`, `yamllint`, `cargo clippy -D warnings`,
`cargo doc -D warnings`, and `cargo test` *before* the commit is
created. A `fmt`-only failure cannot reach `master` at all -- it
fails the commit locally. The 0.5.0 incident is exactly the case
this hook prevents, in the order of seconds rather than minutes,
without any GitHub-side configuration.

This ADR records the decision so a future reviewer does not
re-raise the same finding and so the chosen mitigation (the
pre-commit hook) is the documented answer rather than a verbal
convention.

## Decision

No branch-protection rule is configured for `master` on
`github.com/VitalyOstanin/markdown-org-extract`. The maintainer
relies on the local `pre-commit` hook installed by
`scripts/install-hooks.sh` to keep `master` green.

### Rationale

- The repository is single-maintainer with no external
  contributors today (`docs/adr/0005-no-community-meta-docs.md`
  records the related decision not to ship community meta-docs).
- The lenient `enforce_admins=false` mode protects only
  force-push and branch deletion, neither of which is a recurring
  problem -- the maintainer does not force-push to `master`, and
  `master` deletion is implicitly blocked by GitHub's default
  refusal to let the only admin delete the default branch.
- The strict `enforce_admins=true` mode trades five minutes of CI
  wait against five seconds of local hook execution for the same
  guarantee on the fmt / clippy / test surface that actually
  regressed.
- Adding a PR-only workflow for a solo repository contradicts the
  spirit of [ADR-0005](0005-no-community-meta-docs.md) (do not
  add multi-contributor scaffolding before there are multiple
  contributors).

### Required mitigation

The maintainer **must** have the pre-commit hook installed on
every working checkout. Running `scripts/install-hooks.sh` on a
fresh clone of the repository is therefore part of the developer
setup. The script is idempotent and accepts `--force` to overwrite
an existing hook.

A future reviewer finding of the form "branch protection is
missing" or "set up required status checks on master" closes with
a pointer to this ADR. Reopening requires either (a) the
repository gaining a second active maintainer / external
contributors, or (b) evidence that the pre-commit hook policy
failed in a way that branch protection would have caught.

### When to reconsider

- A second active maintainer joins. Branch protection then
  prevents one maintainer from accidentally pushing a regression
  the other was about to fix.
- The repository starts accepting external pull requests. Branch
  protection (with `enforce_admins=false`) becomes the cheapest
  way to keep "the CI must be green before merge" non-bypassable
  for non-admin contributors.
- The pre-commit hook is repeatedly skipped (`git commit
  --no-verify`) in a way that puts `master` red. At that point
  the trade-off shifts and `enforce_admins=true` may be cheaper
  than the discipline of always running the hook.

Until one of these triggers fires, the decision stands.

## Consequences

Easier:

- The day-to-day commit / push loop stays one command
  (`git push`), with the pre-commit hook running locally in
  seconds instead of waiting for CI.
- Hotfix-forwarding a CI-only issue (e.g. a transient flake on
  the macOS runner) is direct -- no PR ceremony for a fix that
  was already validated locally by `scripts/check.sh`.
- The release process described in
  [ADR-0011](0011-release-commit-and-tag-format.md) keeps a
  linear master history (release commit -> annotated tag) without
  intermediate merge commits.

Harder:

- The maintainer must remember to run `scripts/install-hooks.sh`
  on every fresh clone. Forgetting it removes the safety net.
  The README's "Helper scripts" subsection now documents the
  script so this step is visible during onboarding.
- A pre-commit hook can be bypassed with `git commit --no-verify`.
  The maintainer must not use that flag without an explicit
  reason and a follow-up `scripts/check.sh` run.
- If a regression slips past the hook (network-dependent test,
  filesystem-dependent test, etc.), CI catches it post-push and
  the maintainer fix-forwards. This is the 0.5.0 pattern; the
  rollback policy ([ADR-0010](0010-rollback-policy.md)) already
  governs whether such a post-push fix is a patch release or a
  yank.

## References

- 2026-05-25 infra-CI-tests review (F1):
  [`docs/reviews/2026-05-25-1450-review.md`](../reviews/2026-05-25-1450-review.md).
- Pre-commit hook installer:
  [`scripts/install-hooks.sh`](../../scripts/install-hooks.sh).
- Full local CI script the hook delegates to:
  [`scripts/check.sh`](../../scripts/check.sh).
- Related ADRs:
  [ADR-0005](0005-no-community-meta-docs.md) (no
  multi-contributor scaffolding until a community exists; the
  same reasoning applies to PR-only workflow),
  [ADR-0010](0010-rollback-policy.md) (how a regression that
  slips past local checks is handled at release time),
  [ADR-0011](0011-release-commit-and-tag-format.md) (the linear
  release-commit / annotated-tag flow that this ADR keeps
  intact).
- GitHub branch-protection API reference:
  [Branch protection rules](https://docs.github.com/en/rest/branches/branch-protection)
  (consulted from general knowledge; not fetched in this commit).
