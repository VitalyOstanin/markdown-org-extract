#!/usr/bin/env bash
# Install a git pre-commit hook that runs scripts/check.sh.
#
# Usage:
#   scripts/install-hooks.sh           # refuse if .git/hooks/pre-commit exists
#   scripts/install-hooks.sh --force   # overwrite an existing hook
#
# The hook itself is a thin wrapper: it cds to the repo root and execs
# scripts/check.sh. Failure of any step aborts the commit (`set -e` and the
# hook's non-zero exit propagate to git).

set -euo pipefail

force=0
for arg in "$@"; do
    case "$arg" in
        --force) force=1 ;;
        -h|--help)
            sed -n 's/^# \{0,1\}//;2,12p' "$0"
            exit 0
            ;;
        *)
            echo "error: unknown argument: $arg" >&2
            echo "       usage: $0 [--force]" >&2
            exit 2
            ;;
    esac
done

if ! repo_root=$(git rev-parse --show-toplevel 2>/dev/null); then
    echo "error: not inside a git repository — run from a checkout" >&2
    exit 1
fi

hook_dir="$repo_root/.git/hooks"
hook="$hook_dir/pre-commit"

mkdir -p "$hook_dir"

if [ -e "$hook" ] && [ "$force" -eq 0 ]; then
    echo "error: $hook already exists — pass --force to overwrite" >&2
    exit 1
fi

# Heredoc body: re-exec scripts/check.sh from the repo root so the hook
# behaves identically whether `git commit` is launched from a subdirectory
# or the top level.
cat > "$hook" <<'HOOK'
#!/usr/bin/env bash
# Installed by scripts/install-hooks.sh. Do not edit by hand — re-run the
# installer with --force to refresh.
set -euo pipefail
repo_root=$(git rev-parse --show-toplevel)
exec "$repo_root/scripts/check.sh"
HOOK

chmod +x "$hook"

echo "installed: $hook" >&2
