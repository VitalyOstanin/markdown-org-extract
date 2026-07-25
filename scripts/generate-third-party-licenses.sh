#!/usr/bin/env bash
# Render THIRD-PARTY-LICENSES.txt: the third-party crates that end up in a
# published binary, with the full text of every licence file they ship.
#
# The published archives contain a statically linked binary, so MIT,
# BSD-2/3-Clause, Apache-2.0 and the Unicode licences all require their
# copyright notices and texts to travel with it. `LICENSE` in the archive
# covers this project only; this file covers everything linked into it.
#
# Usage: scripts/generate-third-party-licenses.sh [output-path]
#
# Environment:
#   CARGO        cargo executable to call (default `cargo`)
#   OUTPUT_ROOT  directory the default output path is relative to
#                (default: the repository root, i.e. this script's parent)
#
# The output is committed and shipped inside the release archives; CI
# regenerates it and fails when the result differs, so nothing here may
# depend on run order, filesystem order or the machine.
#
# Requires `jq` and either `sha256sum` or `shasum`.

set -euo pipefail

CARGO=${CARGO:-cargo}

script_dir=$(cd "$(dirname "$0")" && pwd)
repo_root=${OUTPUT_ROOT:-$(cd "${script_dir}/.." && pwd)}
output=${1:-${repo_root}/THIRD-PARTY-LICENSES.txt}

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required to read cargo metadata" >&2
  exit 1
fi

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    # macOS ships `shasum` but not GNU coreutils' `sha256sum`.
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

# The crate list is the union over every target: one notice file then covers
# all four published archives, at the cost of naming crates (other platforms',
# proc-macro ones) that a given binary does not link. Over-inclusion is the
# safe direction for an attribution notice.
"$CARGO" tree --edges normal --target all --prefix none --format '{p}' --locked \
  --color never >"${tmpdir}/tree.raw"
# CI sets CARGO_TERM_COLOR=always, and the markers below then arrive wrapped
# in ANSI escapes. `--color never` above already covers cargo, but the strip
# stays as the actual defence: a coloured marker does not fail loudly, it
# silently sticks to the version and turns into "crate is in the tree but not
# in cargo metadata".
sed -e 's/\x1b\[[0-9;]*m//g' "${tmpdir}/tree.raw" >"${tmpdir}/tree.txt"
"$CARGO" metadata --format-version 1 --locked >"${tmpdir}/metadata.json"

# The workspace member itself is the only entry cargo prints with a local path
# suffix; it is the crate being licensed, not a third party.
root_line=$(grep -m1 -E ' \(/' "${tmpdir}/tree.txt" || true)
if [ -z "$root_line" ]; then
  echo "error: could not find the workspace root in cargo tree output" >&2
  exit 1
fi
root_name=${root_line%% *}

# `name vX.Y.Z` plus optional ` (*)` (repeat) / ` (proc-macro)` markers.
# Deliberately no version for the root package: pinning it here would make the
# file stale on every release bump, for no licence-related reason.
grep -v ' (/' "${tmpdir}/tree.txt" |
  sed -e 's/ (\*)$//' -e 's/ (proc-macro)$//' -e 's/ (.*)$//' |
  sed -e 's/ v/\t/' |
  grep -v '^[[:space:]]*$' |
  LC_ALL=C sort -u >"${tmpdir}/crates.tsv"

jq -r '.packages[] | [.name, .version, (.license // ""), (.repository // ""), .manifest_path] | @tsv' \
  "${tmpdir}/metadata.json" >"${tmpdir}/metadata.tsv"

# Join the two: every crate in the tree must be known to metadata, otherwise
# the notice would silently miss a licence.
awk -F'\t' '
  NR == FNR { lic[$1 "\t" $2] = $3; repo[$1 "\t" $2] = $4; man[$1 "\t" $2] = $5; next }
  {
    key = $1 "\t" $2
    if (!(key in man)) {
      printf "error: %s %s is in the dependency tree but not in cargo metadata\n", $1, $2 > "/dev/stderr"
      exit 1
    }
    printf "%s\t%s\t%s\t%s\t%s\n", $1, $2, lic[key], repo[key], man[key]
  }
' "${tmpdir}/metadata.tsv" "${tmpdir}/crates.tsv" >"${tmpdir}/joined.tsv"

: >"${tmpdir}/crate-list.txt"
notice_count=0

# Fields are split by hand rather than with `IFS=$'\t' read`: tab is IFS
# whitespace, so `read` collapses runs of it and a crate with no declared
# repository would shift the manifest path into the wrong variable.
tab=$'\t'
while IFS= read -r record; do
  [ -n "$record" ] || continue
  name=${record%%"$tab"*}
  rest=${record#*"$tab"}
  version=${rest%%"$tab"*}
  rest=${rest#*"$tab"}
  license=${rest%%"$tab"*}
  rest=${rest#*"$tab"}
  repository=${rest%%"$tab"*}
  manifest=${rest#*"$tab"}
  crate_dir=$(dirname "$manifest")

  licence_files=$(
    cd "$crate_dir" && find . -maxdepth 1 ! -type d \
      \( -iname 'license*' -o -iname 'licence*' -o -iname 'copying*' \
      -o -iname 'notice*' -o -iname 'unlicense*' \) |
      sed 's|^\./||' | LC_ALL=C sort
  )
  if [ -z "$licence_files" ]; then
    echo "error: ${name} ${version} ships no licence text in ${crate_dir}" >&2
    echo "  its licence expression is: ${license:-<none>}" >&2
    echo "  add the text by hand or drop the dependency -- a binary release" >&2
    echo "  must carry the notices of everything linked into it" >&2
    exit 1
  fi

  refs=""
  while IFS= read -r licence_file; do
    [ -n "$licence_file" ] || continue
    digest=$(hash_file "${crate_dir}/${licence_file}")
    if [ -f "${tmpdir}/hash-${digest}" ]; then
      index=$(cat "${tmpdir}/hash-${digest}")
    else
      notice_count=$((notice_count + 1))
      index=$notice_count
      printf '%s\n' "$index" >"${tmpdir}/hash-${digest}"
      cp "${crate_dir}/${licence_file}" "${tmpdir}/notice-${index}.txt"
      printf '%s\n' "$licence_file" >"${tmpdir}/notice-${index}.name"
      printf '%s %s\n' "$name" "$version" >"${tmpdir}/notice-${index}.first"
      : >"${tmpdir}/notice-${index}.users"
    fi
    printf '%s %s\n' "$name" "$version" >>"${tmpdir}/notice-${index}.users"
    refs="${refs:+${refs}, }[${index}] ${licence_file}"
  done <<EOF
$licence_files
EOF

  {
    printf '%s %s\n' "$name" "$version"
    printf '    License:  %s\n' "${license:-<not declared>}"
    if [ -n "$repository" ]; then
      printf '    Source:   %s\n' "$repository"
    fi
    printf '    Notices:  %s\n' "$refs"
    printf '\n'
  } >>"${tmpdir}/crate-list.txt"
done <"${tmpdir}/joined.tsv"

rule="================================================================"

{
  printf '%s\n' "THIRD-PARTY LICENSES"
  printf '%s\n\n' "===================="
  cat <<EOF
The released ${root_name} binary is statically linked, so it also
contains code from the third-party crates listed below. Their licence texts
and copyright notices are reproduced in full further down; this file
complements LICENSE, which covers only this project's own code.

Generated by scripts/generate-third-party-licenses.sh. Do not edit by hand:
CI regenerates the file and fails when the result differs from the committed
one.

The crate list is the union of the normal dependency graph over every target
(cargo tree --edges normal --target all), so one file covers every published
archive. It is a superset of what a single binary links: crates for other
platforms, and proc-macro crates that only run during the build, are listed
as well.

EOF
  printf '%s\n' "$rule"
  printf '%s\n' "CRATES"
  printf '%s\n\n' "$rule"
  cat "${tmpdir}/crate-list.txt"

  index=1
  while [ "$index" -le "$notice_count" ]; do
    licence_file=$(cat "${tmpdir}/notice-${index}.name")
    first=$(cat "${tmpdir}/notice-${index}.first")
    users=$(LC_ALL=C sort -u "${tmpdir}/notice-${index}.users" | wc -l | tr -d ' ')
    printf '%s\n' "$rule"
    if [ "$users" -gt 1 ]; then
      printf 'NOTICE [%s] -- %s (%s, shared by %s crates)\n' \
        "$index" "$licence_file" "$first" "$users"
    else
      printf 'NOTICE [%s] -- %s (%s)\n' "$index" "$licence_file" "$first"
    fi
    printf '%s\n\n' "$rule"
    cat "${tmpdir}/notice-${index}.txt"
    printf '\n'
    index=$((index + 1))
  done
} >"${tmpdir}/output.txt"

cp "${tmpdir}/output.txt" "$output"
echo "wrote ${output} (${notice_count} distinct licence texts)" >&2
