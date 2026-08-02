# ADR-0026: Several roots in one scan, and the `root` field that names them

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted (2026-08-02). Non-breaking JSON addition governed by
[ADR-0015](0015-json-schema-evolution.md).

## Context

Notes are not always kept in one place. A work repository on one git
server and a private one on another, a vault shared with a team and a
personal one, a collection on the device and one on a card — and the
agenda over them is the agenda of all of them together, not of whichever
one is configured at the moment. The Android client
([`markdown-org-android`](https://github.com/VitalyOstanin/markdown-org-android))
is the first consumer to need it; the VS Code extension has the same
shape of problem in a workspace with several folders.

Until now a run scanned exactly one directory: `scan_directory(dir, …)`
and `--dir <PATH>`. A consumer wanting several of them had one way open
to it — call the scan per directory and merge the results itself. That
merge is not a concatenation:

- **The task cap is a budget for the run.** `--max-tasks` bounds what one
  run collects; three separate runs under a cap of ten thousand collect
  thirty thousand, and `max_tasks_reached` stops meaning "the list you
  are holding is truncated".
- **The statistics are one report.** `files_not_utf8`, `files_failed`,
  `nonutf8_paths` and the rest describe a pass over the notes. Per root
  they leave the consumer to add up "one file in another encoding" three
  times and to decide what the sum means.
- **The order is the agenda's.** `filter_agenda` sorts and buckets a flat
  list of tasks; merging afterwards means re-implementing that ordering
  in every client, which is exactly the divergence
  [ADR-0025](0025-library-crate-with-thin-cli.md) keeps the library for.

What the merge does need from outside is the identity of the file a task
came from. `Task::file` is relative to the scanned root, so the same
`inbox/notes.md` in two roots is two different files with one path, and
an edit aimed at one of them by path alone reaches the wrong note.

Three ways to carry that were considered:

1. **A `root` field holding the directory's canonical path.**
2. **A `source` field holding a name the caller gives each root**
   (`--dir work=~/notes`).
3. **No new field: emit absolute paths as soon as there is more than one
   root.**

Option 3 makes the meaning of `file` depend on how many `--dir` flags
were passed, which is a change of semantics — breaking under ADR-0015 —
and it takes away the relative path the Android client re-reads single
files by. Option 2 puts a naming scheme and an escaping question (a path
containing `=`) into the CLI for something the consumer already has:
whatever it calls a collection, it holds the path it configured. Option 1
adds one field whose value the caller passed in and can match on.

## Decision

### `scan_directories` alongside `scan_directory`

`scan_directories(dirs: &[PathBuf], options, interrupt)` walks several
roots as one run and answers with the same `ScanOutcome`: one task list,
one `ProcessingStats`. The single-root `scan_directory` stays as it is —
it is what every existing embedder calls, and it is now a thin wrapper
over the same walk.

Within a run:

- The roots are walked in the order they are given, and each is
  canonicalised and validated **before any of them is walked**: a root
  that has been unmounted or renamed is refused rather than skipped,
  because a collection that silently reads as empty produces an agenda
  missing half the notes with nothing to say why.
- A root named twice is walked once. The same directory configured as two
  sources would otherwise show every task in it twice, which reads as
  duplicated notes rather than as a duplicated setting.
- A root nested inside another one is **not** detected, and notes under it
  are read by both walks. Nesting is a choice the caller makes, and
  refusing it would rule out a collection that deliberately holds a
  smaller one.
- `--max-tasks` is the budget for the whole run; reaching it stops the
  walk and leaves the remaining roots unwalked, with
  `max_tasks_reached` set. An interrupt behaves the same way.
- An empty list of roots is an error, not an empty result: nothing asked
  for a scan of nothing.

### The new optional field `root`

Each `Task` gains `root: Option<String>`, the canonical path of the
directory its `file` is relative to. It is filled in by
`scan_directories` and left `None` by `scan_directory`, so the output of
a single-directory run is unchanged, field for field. Serialised with
`skip_serializing_if`, so it is absent rather than null.

The value is the path, not a name. A name for a collection belongs to
the consumer that configured it — the Android client calls one "work"
and another "personal" — and it maps back to the path it stored. Putting
the name in the wire format would make the library carry a label it can
neither produce nor verify.

### `--dir` becomes repeatable

`--dir` may be given more than once; the CLI merges the roots exactly as
the library does. One `--dir` goes through `scan_directory`, so a
single-directory invocation emits the JSON it always did.

## Consequences

Easier:

- A consumer with several collections gets one agenda, one cap and one
  summary, and does not re-implement the ordering `filter_agenda` owns.
- The CLI can answer "what is on today across all my notes" in one run,
  which was previously a shell loop plus a merge the shell cannot do.

Harder:

- `Task::file` is no longer a whole answer for a consumer that scans
  several roots: it has to join `root` back on. That is the point of the
  field, but it is a second thing to remember, and a consumer that
  ignores it silently addresses the wrong file when two roots share a
  relative path.
- The `root` value repeats on every task of a collection. The cost is one
  `String` per task; a shared handle would save it but would leave the
  JSON shape unchanged, so it is not worth the type.
- Two roots on different filesystems are walked with the same
  `same_file_system(true)` rule as before — each within itself. A root
  that is a mount point of another still ends its own walk at the
  boundary.

## References

- Implementation: [`src/scan.rs`](../../src/scan.rs)
  (`scan_directories`, `Run`), [`src/types.rs`](../../src/types.rs)
  (`Task::root`), [`src/main.rs`](../../src/main.rs) (the repeated
  `--dir`).
- Tests: `tests/lib_api.rs` (`scan_directories_*`), `tests/cli.rs`
  (`several_dirs_*`, `one_dir_emits_no_root_field`).
- Related ADRs:
  [ADR-0015](0015-json-schema-evolution.md) (the rule under which `root`
  is a non-breaking addition),
  [ADR-0025](0025-library-crate-with-thin-cli.md) (why the merge belongs
  to the library rather than to each client),
  [ADR-0019](0019-input-encoding-expectations.md) (`nonutf8_paths`, one
  of the counters now summed over the roots).
