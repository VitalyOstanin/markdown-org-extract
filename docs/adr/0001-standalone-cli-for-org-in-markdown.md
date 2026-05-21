# ADR-0001: Standalone CLI for org-mode in markdown

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

Users of Emacs Org-mode increasingly keep their notes and tasks in
markdown files so the same content can be consumed by markdown-first
editors (VS Code, Obsidian, web previews) while still being parsed by
Emacs' `org-agenda`, `org-clock-report`, and friends. The community
needs a fast, reliable way to scan such a workspace -- often dozens to
thousands of markdown files -- and produce structured data
(tasks, status, priorities, timestamps, CLOCK entries, holidays) that
any front-end can render.

The same scanning logic could live inside each consumer (VS Code
extension, Obsidian plugin, custom script). Three rough shapes were
on the table:

1. Re-implement the scanner per consumer (TypeScript inside the VS
   Code extension, JavaScript inside Obsidian, Python in a CLI,
   etc.).
2. Provide a shared library in one language and require each consumer
   to embed it.
3. Provide a standalone native binary that emits a stable JSON
   contract on stdout, and let consumers shell out to it.

Option 1 duplicates work and inevitably diverges. Option 2 forces a
language choice on every consumer. Option 3 gives the lowest per-call
cost (no Node startup, no extension-host blocking) and lets any
consumer in any language reuse the same scanner.

## Decision

This project is a standalone Rust CLI distributed via crates.io as
`markdown-org-extract`. Its public surface is the command-line
invocation and the JSON output on stdout.

The contract is:

- Invocation:
  `markdown-org-extract --dir <ws> --format json [--agenda <mode>] [--date <iso>] [--from <iso>] [--to <iso>] [--current-date <iso>] [--tasks] [--holidays <year>] [--absolute-paths]`.
- Output: a JSON document on stdout, schemas described in
  `src/types.rs`.
- Failure modes: non-zero exit + a human-readable message on stderr;
  hard errors never go to stdout.
- Performance: cold scans are designed to be fast on large repos by
  using `ignore` for `.gitignore`-aware traversal and `grep-searcher`
  for regex pre-filtering.

`markdown-org-vscode` is the first known consumer; see its
[ADR-0001](https://github.com/VitalyOstanin/markdown-org-vscode/blob/master/docs/adr/0001-external-rust-extractor.md)
for the consumer-side view of the same contract.

## Consequences

Easier:

- Cold scans of large repos are much faster than a TypeScript or
  Python walk would be, thanks to Rust + `ignore` + `grep-searcher`.
- The scanner is reusable: any editor / shell / agent can invoke the
  same binary and get the same JSON shape.
- The consumer's host process (VS Code extension host, etc.) stays
  responsive because parsing runs in a separate process.
- Distribution piggybacks on the existing Rust toolchain
  (`cargo install markdown-org-extract`, prebuilt binaries via
  GitHub Releases when available).

Harder:

- Users must install the binary separately. Each consumer documents
  how to do this and surfaces a clear error if the binary is
  missing.
- The JSON wire format and the on-disk org-mode wire format are now
  external contracts. Changes require coordinated updates across
  this repo and every consumer; the bar for changes is high. See
  [ADR-0002](0002-supported-org-mode-subset.md) and
  [ADR-0003](0003-clock-metadata-support.md) for the on-disk
  format.
- Security: consumers execute this binary on user data. The
  README recommends pointing only at trusted binary paths;
  `markdown-org-vscode` enforces this in untrusted workspaces.

## References

- Output types: [`src/types.rs`](../../src/types.rs)
- Argument parsing: [`src/cli.rs`](../../src/cli.rs)
- Scanner entry point: [`src/main.rs`](../../src/main.rs)
- Walker: [`src/walker.rs`](../../src/walker.rs)
- First known consumer: [github.com/VitalyOstanin/markdown-org-vscode](https://github.com/VitalyOstanin/markdown-org-vscode)
- Crate: [crates.io/crates/markdown-org-extract](https://crates.io/crates/markdown-org-extract)
- Upstream reference for Org-mode semantics: [orgmode.org](https://orgmode.org/)
