#!/usr/bin/env bash
# Verify that CHANGELOG.md is ready to publish the given version.
#
# Usage: scripts/check-changelog.sh <VERSION>     (e.g. 0.2.3, no "v" prefix)
#
# Checks performed:
#   1. The file "$CHANGELOG" (default CHANGELOG.md) exists.
#   2. A section "## [<VERSION>]" exists. The version is matched literally
#      (dots escaped) so that "0.2.3" does NOT match "## [0X2X3]".
#   3. The "## [Unreleased]" section body, taken up to the next "## ["
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

if ! grep -qE "^## \[${VERSION_RE}\](\$| )" "$CHANGELOG"; then
    echo "error: $CHANGELOG has no section '## [${VERSION}]'" >&2
    echo "       move entries from '## [Unreleased]' into a new '## [${VERSION}]' section before tagging" >&2
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
