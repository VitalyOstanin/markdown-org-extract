#!/usr/bin/env bash
# Run a RustSec advisory scan over the dependency tree, locally.
#
# Usage:
#   scripts/audit.sh            # cargo audit with default settings
#   scripts/audit.sh --json     # any extra args are forwarded to cargo audit
#
# This is intentionally NOT part of scripts/check.sh (and therefore not part
# of the pre-commit hook): `cargo audit` fetches the RustSec advisory
# database over the network, which would add latency and an online
# dependency to every commit. ADR-0017 keeps the commit loop fast and local,
# so the advisory scan lives here for a deliberate pre-push / pre-release
# run. CI runs the same check on every push via `rustsec/audit-check`.
#
# If `cargo-audit` is not installed the script prints how to install it and
# exits 0 (a missing optional tool is not a failure of the caller's change).

set -euo pipefail

if ! command -v cargo-audit >/dev/null 2>&1; then
    printf '==> cargo audit skipped: cargo-audit is not installed\n' >&2
    printf '    install it with: cargo install --locked cargo-audit\n' >&2
    exit 0
fi

printf '==> cargo audit\n' >&2
exec cargo audit "$@"
