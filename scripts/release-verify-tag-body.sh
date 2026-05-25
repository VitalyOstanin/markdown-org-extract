#!/usr/bin/env bash
# Verify that a release tag conforms to ADR-0011: it is an annotated tag
# (not lightweight) and its message equals the canonical body emitted by
# scripts/release-prep.sh.
#
# Usage: scripts/release-verify-tag-body.sh <VERSION>   (no "v" prefix)
#
# Runs in the release workflow's publish job after the CHANGELOG check and
# before `gh release create`, so a tag whose body drifted from CHANGELOG —
# the v0.5.0 tag that dropped its `### Added` / `### Changed` headings
# (L1 in the 2026-05-25 release review) — stops the release before anything
# is published. Also runnable locally right after tagging.
#
# Exit status: 0 when the tag is annotated and its body mirrors CHANGELOG;
# non-zero otherwise, with a diagnostic and the exact re-create command on
# stderr. stdout is never written to, so the script is safe inside
# `id:`-tagged workflow steps.

set -euo pipefail

if [ $# -lt 1 ] || [ -z "${1:-}" ]; then
    echo "usage: $0 <VERSION>   (e.g. 0.6.0, no 'v' prefix)" >&2
    exit 2
fi

VERSION="$1"
TAG="v${VERSION}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 1. The tag object must be annotated, not lightweight. A lightweight tag has
#    no message at all and cannot carry the CHANGELOG body.
obj_type=$(git cat-file -t "$TAG" 2>/dev/null || true)
if [ "$obj_type" != "tag" ]; then
    {
        echo "error: $TAG is not an annotated tag (git cat-file -t = '${obj_type:-<missing>}')"
        echo "       ADR-0011 requires 'git tag -a'. Re-create it with:"
        echo "         git tag -f -a $TAG --cleanup=verbatim -F <(scripts/release-prep.sh $VERSION)"
    } >&2
    exit 1
fi

# 2. The tag message must equal the canonical body. `$(...)` already trims
#    trailing newlines on both sides, so a stray blank line at EOF does not
#    cause a spurious mismatch.
expected=$("$SCRIPT_DIR/release-prep.sh" "$VERSION")
actual=$(git tag -l --format='%(contents)' "$TAG")

if [ "$expected" != "$actual" ]; then
    {
        echo "error: $TAG body does not mirror the CHANGELOG [$VERSION] section (ADR-0011)."
        echo "       A common cause is the default tag cleanup deleting '### ...' heading"
        echo "       lines (they begin with the comment character). Re-create the tag with"
        echo "       --cleanup=verbatim so the headings survive:"
        echo "         git tag -f -a $TAG --cleanup=verbatim -F <(scripts/release-prep.sh $VERSION)"
        echo "--- expected (scripts/release-prep.sh $VERSION) ---"
        printf '%s\n' "$expected" | sed 's/^/       | /'
        echo "--- actual (git tag contents $TAG) ---"
        printf '%s\n' "$actual" | sed 's/^/       | /'
    } >&2
    exit 1
fi

echo "ok: $TAG is annotated and its body mirrors the CHANGELOG [$VERSION] section" >&2
