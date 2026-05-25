#!/usr/bin/env bash
# Print the canonical annotated-tag message for a release version.
#
# Usage: scripts/release-prep.sh <VERSION>     (no "v" prefix; e.g. 0.6.0)
#
# Output on stdout, exactly the text to feed `git tag -a`:
#
#   v<VERSION>
#   <blank line>
#   <the "## [VERSION] — YYYY-MM-DD" CHANGELOG section body, ### subheadings
#    included, leading/trailing blank lines trimmed>
#
# This mirrors ADR-0011: the tag subject is `v<VERSION>` and the body is the
# unwrapped CHANGELOG section — the same text `.github/workflows/release.yml`
# extracts for the GitHub Release notes. Generating it removes the manual
# copy-paste step that produced the v0.5.0 tag whose `### Added` / `### Changed`
# headings were lost (L1 in the 2026-05-25 release review).
#
# IMPORTANT: create the tag with `--cleanup=verbatim`. The default tag
# message cleanup (`strip`) deletes every line beginning with the comment
# character `#`, which silently removes the `### Added` / `### Fixed`
# headings — exactly the v0.5.0 regression. Verbatim keeps them:
#
#   git tag -a "v<VERSION>" --cleanup=verbatim -F <(scripts/release-prep.sh <VERSION>)
#
# scripts/release-verify-tag-body.sh re-checks the created tag against this
# output, and the release workflow runs that check before publishing.
#
# On any error (CHANGELOG missing, no matching section, empty body) the
# script writes a diagnostic to stderr and exits non-zero; on success it
# prints ONLY the message to stdout.

set -euo pipefail

if [ $# -lt 1 ] || [ -z "${1:-}" ]; then
    echo "usage: $0 <VERSION>   (e.g. 0.6.0, no 'v' prefix)" >&2
    exit 2
fi

VERSION="$1"
CHANGELOG="${CHANGELOG:-CHANGELOG.md}"

if [ ! -f "$CHANGELOG" ]; then
    echo "error: $CHANGELOG not found" >&2
    exit 2
fi

# Same extraction the GitHub Release notes use (release.yml) and ADR-0011:
# everything between "## [<VERSION>]" and the next "## [" heading, with the
# leading and trailing blank lines trimmed. The output is byte-for-byte the
# same as the workflow's, so the tag body and the GitHub Release notes match;
# the trim is done in awk (not a GNU-only sed blank-line delete) so it also
# runs under BSD sed on the macOS CI runner. release.yml keeps its sed because
# its publish job is Linux-only.
body=$(awk -v ver="$VERSION" '
    $0 ~ "^## \\["ver"\\]" { flag=1; next }
    flag && /^## \[/ { exit }
    flag {
        # Drop leading blank lines, then buffer the rest while tracking the
        # last non-blank line so trailing blanks are dropped in END. Internal
        # blank lines are preserved.
        if ($0 == "" && !started) next
        started = 1
        buf[++n] = $0
        if ($0 != "") last = n
    }
    END {
        for (i = 1; i <= last; i++) print buf[i]
    }
' "$CHANGELOG")

if [ -z "$body" ]; then
    echo "error: $CHANGELOG has no non-empty '## [${VERSION}]' section" >&2
    exit 1
fi

printf 'v%s\n\n%s\n' "$VERSION" "$body"
