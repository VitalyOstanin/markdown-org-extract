#!/usr/bin/env bash
# Run the same checks CI runs, in the same order, locally.
#
# Usage: scripts/check.sh
#
# Sequence (fail-fast):
#   1. cargo fmt --all -- --check
#   2. cargo clippy --all-targets --all-features -- -D warnings
#   3. cargo test  --all-features
#
# Exits with the failing step's status code, prefixed by a diagnostic on
# stderr. Intended as the body of a git pre-commit hook (see
# scripts/install-hooks.sh) and as a one-liner before opening a PR.

set -euo pipefail

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
run_step "cargo clippy"       cargo clippy --all-targets --all-features -- -D warnings
run_step "cargo test"         cargo test  --all-features

step "all checks passed"
