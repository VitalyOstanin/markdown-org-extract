#!/usr/bin/env bash
# Refuse to publish a commit the remote's release branch does not contain.
#
# Pushing a tag hands GitHub the commit with it, so `git push origin v0.19.0`
# alone is enough to start the release workflow over a commit no branch of
# `origin` holds: crates.io would receive the release while `master` still
# stands where it did. The branch is deliberately unprotected (ADR-0017), so
# this check is the barrier. Push order is `master` first, then the tag.
#
# Used by `.github/workflows/release.yml`. Run from inside the repository
# whose HEAD is being released; the caller is responsible for a checkout with
# history (a shallow one cannot answer whether a commit is on master).
#
# Usage:
#   scripts/release-check-ancestry.sh [remote] [branch]
#
# Defaults to `origin master`. Exits 0 when HEAD is an ancestor of the remote
# branch, 1 otherwise, with a GitHub-annotated line on stdout so the workflow
# log points at the cause.

set -euo pipefail

remote="${1:-origin}"
branch="${2:-master}"

# `--depth=0` is not "no limit" -- git rejects it outright ("depth 0 is not a
# positive number"), which is how this check failed the first time it ever
# ran. The caller's checkout already fetched the full history; this fetch is
# here for the remote's current branch, and FETCH_HEAD names it without
# depending on a remote-tracking ref.
git fetch --no-tags "$remote" "$branch"

head="$(git rev-parse HEAD)"
if ! git merge-base --is-ancestor "$head" FETCH_HEAD; then
    echo "::error::$head is not on $remote/$branch. Push the branch first, then the tag."
    exit 1
fi
