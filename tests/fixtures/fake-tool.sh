#!/usr/bin/env bash
# Stand-in for `cargo` and `yamllint` in the scripts/check.sh tests.
#
# Which tool it plays is read from the name it was invoked by, so one file can
# be symlinked under both names. It lives in the repository rather than being
# written per test on purpose: a file being written cannot be executed while
# another thread's fork still holds the descriptor, and the tests in this file
# spawn processes constantly. That collision surfaced as an exit code of 126
# and an empty log, which read as if check.sh had skipped its steps.
#
# Environment:
#   FAKE_LOG            file to append one line per invocation to
#   FAKE_CARGO_FAIL     cargo subcommand that must exit 1 (default: none)
#   FAKE_YAMLLINT_EXIT  exit code for yamllint (default: 0)

set -u

if [ "$(basename "$0")" = "yamllint" ]; then
    echo "yamllint $*" >>"$FAKE_LOG"
    exit "${FAKE_YAMLLINT_EXIT:-0}"
fi

echo "$*" >>"$FAKE_LOG"
if [ "${1:-}" = "${FAKE_CARGO_FAIL:-__none__}" ]; then
    exit 1
fi
exit 0
