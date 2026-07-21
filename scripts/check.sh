#!/usr/bin/env bash
# Run the same checks CI runs, in the same order, locally.
#
# Usage: scripts/check.sh
#
# Sequence (fail-fast):
#   1. cargo fmt    --all -- --check
#   2. yamllint     .github/workflows/
#   3. cargo clippy --all-targets --all-features -- -D warnings
#   4. cargo doc    --no-deps --all-features  (RUSTDOCFLAGS="-D warnings")
#   5. cargo test   --all-features
#
# Exits with the failing step's status code, prefixed by a diagnostic on
# stderr. Intended as the body of a git pre-commit hook (see
# scripts/install-hooks.sh) and as a one-liner before opening a PR.
#
# The test step is wrapped in coreutils `timeout` so a hung or deadlocked test
# cannot block a commit or a local run indefinitely (CI caps this per-job with
# `timeout-minutes`, but the local script had no equivalent guard). Override the
# limit with TEST_TIMEOUT (a `timeout` DURATION, e.g. `300s`, `10m`); set it to
# `0` to disable the cap. If `timeout` is unavailable, the tests run unwrapped.

set -euo pipefail

# Per-invocation wall-clock cap for the test step. 600s comfortably exceeds a
# clean-build test run while still bounding a genuine hang.
TEST_TIMEOUT="${TEST_TIMEOUT:-600s}"

step() {
    # Print a uniform banner so the failing step is easy to spot when the
    # output of the underlying cargo invocation is long.
    printf '==> %s\n' "$*" >&2
}

run_step() {
    local label="$1"
    shift
    step "$label"
    # Disable `set -e` for the inner call so we can capture the exit code
    # and emit a uniform diagnostic instead of dying silently.
    set +e
    "$@"
    local rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        printf 'error: %s failed (exit code %d); fix and re-run scripts/check.sh\n' \
            "$label" "$rc" >&2
        exit "$rc"
    fi
}

run_step "cargo fmt --check"  cargo fmt  --all -- --check
run_step "yamllint"           yamllint .github/workflows/ .github/actions/
run_step "cargo clippy"       cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" \
run_step "cargo doc"          cargo doc  --no-deps --all-features
# Wrap tests in `timeout` when it is available and the cap is non-zero. A hung
# test then dies with exit code 124 (timeout's convention) instead of blocking
# forever. `timeout` is coreutils on Linux and `gtimeout` (coreutils) on macOS.
timeout_cmd=""
if [ "$TEST_TIMEOUT" != "0" ]; then
    if command -v timeout >/dev/null 2>&1; then
        timeout_cmd="timeout $TEST_TIMEOUT"
    elif command -v gtimeout >/dev/null 2>&1; then
        timeout_cmd="gtimeout $TEST_TIMEOUT"
    else
        step "note: 'timeout' not found; running tests without a time cap"
    fi
fi
# shellcheck disable=SC2086 # intentional word-splitting of the optional wrapper
run_step "cargo test"         $timeout_cmd cargo test --all-features

step "all checks passed"
