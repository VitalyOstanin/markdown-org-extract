# ADR-0019: Input encoding expectations

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
  - [1. Content is expected in UTF-8, NFC](#1-content-is-expected-in-utf-8-nfc)
  - [2. Non-UTF-8 file paths render lossily with a one-time warning](#2-non-utf-8-file-paths-render-lossily-with-a-one-time-warning)
  - [3. Timestamp body limits are counted in code points](#3-timestamp-body-limits-are-counted-in-code-points)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted. Records the project's text-encoding expectations for input
content and file paths, raised by the 2026-05-25 encoding review.
Relates to [ADR-0002](0002-supported-org-mode-subset.md) (the
supported input subset), [ADR-0015](0015-json-schema-evolution.md)
(why the `file` field stays a plain JSON string), and
[ADR-0008](0008-rf-defaults.md) (the Russian-locale weekday table the
NFC discussion turns on).

## Context

The extractor reads markdown files from a directory tree and emits a
JSON wire contract whose records carry the file path and the parsed
heading / timestamp text. Three encoding questions surfaced in review,
none of which had a written policy:

1. **Unicode normalization form of the content.** Weekday-name
   normalization compares bytes (Aho-Corasick over
   `RU_WEEKDAY_MAPPINGS`). If a file arrived in NFD instead of NFC, a
   decomposed letter would be a different byte sequence than the NFC
   pattern and would not match.
2. **File paths that are not valid UTF-8.** On Linux a filename is an
   arbitrary sequence of non-NUL bytes; on Windows a path is UTF-16 and
   may contain unpaired surrogates. Either is representable in Rust's
   `OsStr` but not as a UTF-8 `&str`. The `file` JSON field is a
   string, so such a path must be converted somehow.
3. **What `TS_BODY_MAX` / `CLOCK_BODY_MAX` actually bound.** The
   bounded quantifiers `[^>]{0,TS_BODY_MAX}` in the timestamp regexes
   count repetitions of a Unicode character class, i.e. code points,
   not bytes.

The one practical defect the review found — a leading UTF-8 BOM on the
first line — was already fixed separately (the BOM is stripped after
the UTF-8 decode). This ADR covers the remaining, deliberate choices.

## Decision

### 1. Content is expected in UTF-8, NFC

Input file content is expected to be valid UTF-8 in Normalization Form
C (NFC). Files that are not valid UTF-8 are skipped and counted in
`files_failed_read` (an existing behaviour). No NFC normalization pass
is added, and the `unicode-normalization` dependency is not taken on.

Rationale, verified against the shipped table rather than assumed:

- None of the 14 entries in `RU_WEEKDAY_MAPPINGS`
  (`Понедельник`…`Воскресенье`, `Пн`…`Вс`) contain a character with a
  canonical decomposition. In Russian only `й`/`Й` (→ `и`/`И` + U+0306)
  and `ё`/`Ё` (→ `е`/`Е` + U+0308) decompose under NFD, and none of
  those letters appear in any weekday name. (`Воскресенье` is spelled
  with `е`, not `ё`.) An NFD-encoded weekday name is therefore
  byte-identical to its NFC form and already matches.
- A normalization pass would add a dependency and a per-file CPU cost
  for no behavioural change on the current table, and — if applied to
  the whole content rather than only a timestamp body — would rewrite
  the author's heading and body bytes, which the extractor must not do.

A future locale table that introduces a `й`- or `ё`-bearing pattern,
combined with NFD input, is the only case that would change this
trade-off; revisit the decision then.

### 2. Non-UTF-8 file paths render lossily with a one-time warning

A processed file whose path is not valid UTF-8 is **still read and its
tasks emitted** — the I/O goes through the `OsStr`/`Path` API, not the
string. The `file` field is produced by `Path::display`, which
substitutes U+FFFD for the invalid bytes, so the value may not
round-trip back to the file for a consumer.

When such a path is detected (`Path::to_str()` returns `None`), the run
emits exactly one `warn` for the first occurrence and counts every
occurrence in `ProcessingStats::nonutf8_paths`, which is surfaced in the
summary record. The file is not skipped: dropping its tasks would be a
worse outcome than a lossy label on a platform where such names are
legal but rare.

The alternative of encoding the path losslessly (WTF-8, percent-escape,
or `serde_bytes` byte array) is rejected: it would change the `file`
field from a plain string to a different shape, a breaking change under
[ADR-0015](0015-json-schema-evolution.md), to serve a path shape that
does not occur on the primary platforms. By platform:

- **Linux** — filenames are arbitrary non-NUL bytes; a non-UTF-8 name
  is possible (legacy encodings, broken symlink targets). This is the
  realistic case.
- **macOS** — APFS/HFS+ require valid Unicode filenames, so
  `to_str()` effectively always succeeds; the lossy branch is not
  reached.
- **Windows** — paths are UTF-16 and may contain unpaired surrogates;
  the lossy branch is reachable only for those, which normal tooling
  does not produce.

Valid paths on all three platforms are unaffected and round-trip
exactly.

### 3. Timestamp body limits are counted in code points

`TS_BODY_MAX` (256) and `CLOCK_BODY_MAX` (128) in
[`src/regex_limits.rs`](../../src/regex_limits.rs) bound the number of
repetitions of a Unicode character class, i.e. **code points, not
bytes**. A Cyrillic timestamp body such as `Понедельник` reaches the
limit in fewer bytes' worth of characters than an ASCII body of the
same code-point count. This is intentional: the bound is a
defense-in-depth cap against pathological input (an unterminated
bracket making the engine scan far), not a byte-size budget. No byte
mode (`(?-u)`) is introduced, because that would force every `[^>]`
class to be rewritten in terms of bytes for no security gain — the cap
already bounds the scan window regardless of how many bytes each code
point occupies.

## Consequences

Easier:

- The encoding contract is explicit: UTF-8 NFC content in, lossy-but-
  flagged paths for the non-UTF-8 edge case, code-point-based regex
  caps. A reviewer re-raising "add `unicode-normalization`", "the
  `file` field can be lossy", or "`TS_BODY_MAX` is not a byte limit"
  closes with a pointer to this ADR.
- No new dependency and no JSON-schema change.
- A non-UTF-8 path is observable (one warn plus a summary count) rather
  than silently mangled, and its tasks are not lost.

Harder:

- A consumer that indexes by the `file` field cannot reopen a file
  whose name was non-UTF-8; it must treat a `nonutf8_paths > 0` summary
  as a signal that some `file` values are lossy. This is a documented
  limitation, not a bug.
- NFD-encoded content with a future `й`/`ё`-bearing locale pattern
  would not match; that scenario must supersede decision (1).

## References

- 2026-05-25 encoding review (`docs/reviews/2026-05-25-1450-review.md`,
  encoding section): BOM, NFD weekday matching, non-UTF-8 paths, and the
  code-point `TS_BODY_MAX` observation.
- Code: the lossy-path detection and one-time warn live in
  [`src/main.rs`](../../src/main.rs) (`scan_files`) and
  `ProcessingStats::note_nonutf8_path` in
  [`src/types.rs`](../../src/types.rs); the regex caps are in
  [`src/regex_limits.rs`](../../src/regex_limits.rs); the weekday table
  is `RU_WEEKDAY_MAPPINGS` in [`src/cli.rs`](../../src/cli.rs).
- Behaviour pins: `non_utf8_path_is_processed_and_warned`
  (`tests/cli.rs`) and `note_nonutf8_path_counts_and_makes_summary_visible`
  (`src/types.rs`).
- Related ADRs: [ADR-0002](0002-supported-org-mode-subset.md),
  [ADR-0015](0015-json-schema-evolution.md),
  [ADR-0008](0008-rf-defaults.md).
