#!/usr/bin/env bash
# Verify that CHANGELOG.md is ready to publish the given version.
#
# Usage: scripts/check-changelog.sh <VERSION>     (e.g. 0.2.3, no "v" prefix)
#
# Checks performed:
#   1. The file "$CHANGELOG" (default CHANGELOG.md) exists.
#   2. A section "## [<VERSION>] — YYYY-MM-DD" exists. The version is
#      matched literally (dots escaped) so that "0.2.3" does NOT match
#      "## [0X2X3]". The separator must be an em-dash (U+2014) and the
#      date must be in ISO YYYY-MM-DD form — this is the format every
#      historical entry uses, and the release-notes extractor in
#      release.yml relies on it.
#   3. The "## [<VERSION>] — …" section is the FIRST version heading
#      after "## [Unreleased]". An older or stale section (e.g. an
#      aborted release that left "[0.3.0]" above the one we are cutting)
#      between them signals a CHANGELOG that is out of order and would
#      ship release notes for the wrong version.
#   4. The "## [Unreleased]" section body, taken up to the next "## ["
#      heading, contains no entries other than blank lines or the
#      placeholder line "_No user-visible changes yet._".
#
# On failure the script writes a diagnostic to stderr and exits non-zero.
# It NEVER prints to stdout, so it is safe to use inside `id:`-tagged
# workflow steps.

set -euo pipefail

if [ $# -lt 1 ] || [ -z "${1:-}" ]; then
    echo "usage: $0 <VERSION>" >&2
    exit 2
fi

VERSION="$1"
CHANGELOG="${CHANGELOG:-CHANGELOG.md}"
PLACEHOLDER='_No user-visible changes yet._'

if [ ! -f "$CHANGELOG" ]; then
    echo "error: $CHANGELOG not found" >&2
    exit 2
fi

# Literal-match VERSION as a regex by escaping every metacharacter we care
# about. In practice only "." appears in semver, but escape conservatively.
escape_re() {
    # shellcheck disable=SC2001
    printf '%s' "$1" | sed 's/[.[\*^$()+?{|]/\\&/g'
}
VERSION_RE=$(escape_re "$VERSION")

# The required header line shape: "## [<VERSION>] — YYYY-MM-DD" with an
# em-dash (U+2014, encoded as the three bytes E2 80 94 in UTF-8) and a
# strict ISO date. Validated as a single regex so a missing date, a
# range-style date (`2025-12-06..2025-12-09`), or an ASCII hyphen all
# fail the same way.
DATE_RE='[0-9]{4}-[0-9]{2}-[0-9]{2}'
RELEASE_HEADER_RE="^## \[${VERSION_RE}\] — ${DATE_RE}\$"

if ! grep -qE "$RELEASE_HEADER_RE" "$CHANGELOG"; then
    echo "error: $CHANGELOG has no '## [${VERSION}] — YYYY-MM-DD' section" >&2
    echo "       expected exact form: '## [${VERSION}] — 2026-05-19' (em-dash, ISO date)" >&2
    echo "       hint: ASCII hyphen '-' is rejected; copy the em-dash from an existing entry" >&2
    exit 1
fi

# Monotonicity: the first "## [<some-version>]" heading after Unreleased
# must be the one we are releasing. An older heading wedged between them
# means the CHANGELOG is out of order. Reported as a separate error so
# the developer sees which stale section to move.
first_after_unreleased=$(awk '
    /^## \[Unreleased\]/ { flag = 1; next }
    flag && /^## \[/      { print; exit }
' "$CHANGELOG")

if [ -z "$first_after_unreleased" ]; then
    echo "error: $CHANGELOG has '## [Unreleased]' but no version section after it" >&2
    exit 1
fi

if ! printf '%s\n' "$first_after_unreleased" | grep -qE "$RELEASE_HEADER_RE"; then
    {
        echo "error: section right after '## [Unreleased]' is not '## [${VERSION}] — YYYY-MM-DD':"
        printf '       found: %s\n' "$first_after_unreleased"
        echo "       the new version section must immediately follow Unreleased so release notes are monotonic"
    } >&2
    exit 1
fi

# Extract the body between "## [Unreleased]" and the next "## [" heading.
unreleased_body=$(awk '
    /^## \[Unreleased\]/ { flag = 1; next }
    flag && /^## \[/     { exit }
    flag                  { print }
' "$CHANGELOG")

# Drop blank-only lines and the explicit placeholder. Anything that remains
# is a real entry the developer forgot to move.
leftover=$(printf '%s\n' "$unreleased_body" \
    | grep -vE '^[[:space:]]*$' \
    | grep -vFx "$PLACEHOLDER" \
    || true)

if [ -n "$leftover" ]; then
    {
        echo "error: '## [Unreleased]' section is not empty:"
        printf '%s\n' "$leftover" | sed 's/^/       | /'
        echo "       move these entries into '## [${VERSION}]' before tagging"
    } >&2
    exit 1
fi
