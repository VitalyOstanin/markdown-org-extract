# ADR-0020: Task properties via an org-properties fenced code block

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted. Amends [ADR-0002](0002-supported-org-mode-subset.md): that ADR
lists Org-mode "properties drawers" as out of scope; this ADR adds an
equivalent per-task properties capability in a markdown-native shape (a
fenced code block) rather than the Emacs `:PROPERTIES:` drawer. The JSON
field addition is governed by [ADR-0015](0015-json-schema-evolution.md).

## Context

Tasks need a place to carry structured per-task metadata on disk. The
immediate driver is an optional Google Calendar sync in the consumer
extension that must persist a calendar event id (and later an ETag)
against the task that produced the event. Rather than a single-purpose
hidden field, a general per-task properties mechanism is introduced,
analogous to the Org-mode PROPERTIES drawer.

A bare Emacs `:PROPERTIES:`/`:END:` drawer was rejected: these files are
not opened from Emacs (Emacs interop is not a factor here), and a drawer
would land in the parsed task `content` and need extra filtering. A
fenced code block is already a `CodeBlock` node kept out of `content`,
the parser already has a `CodeBlock` arm, and consumers fold fenced
blocks natively.

## Decision

A property block is a fenced code block whose info string is exactly
`org-properties`, holding bare `KEY: value` lines, placed under the
heading and its planning lines:

    ### TODO Ship release
    `SCHEDULED: <2026-06-01 Mon 10:00>`
    ```org-properties
    GCAL_EVENT_ID: abc123/primary
    ```

Rules:

- **Info string**: exactly `org-properties` after trimming. Namespaced
  (not plain `properties`) so a user's `properties` config example in a
  note is never mistaken for a task's property block. A block whose info
  string carries extra attributes is a plain code block, not a property
  block.
- **Keys**: `UPPER_SNAKE_CASE` convention. The key is the text before the
  first `:`, trimmed; case is significant and preserved. The value is the
  remainder, trimmed. An empty value is allowed (`KEY:` -> `""`). An empty
  key, or a line with no `:`, is skipped and reported via a capped
  `tracing::warn!`.
- **Calendar keys** (first consumer): `GCAL_` prefix -- `GCAL_EVENT_ID`,
  later `GCAL_ETAG`, and `GCAL_CALENDAR_ID` if needed. The `ID` key
  (an org-id UUID) is preserved as a stable per-task identifier. The
  mechanism itself allows any key.
- **Multiplicity**: multiple blocks on one task are merged into a single
  map with last-wins on duplicate keys.
- **Wire format**: parsed into a new optional `properties` object on each
  task in the JSON output (a `BTreeMap`, deterministic key order). Absent
  when a task has no properties. Non-breaking under ADR-0015.

The grep pre-filter (`src/main.rs`) is **not** widened for
`org-properties`: a task with properties always also carries a TODO/DONE
marker or a planning timestamp, so its file still passes the pre-filter,
and widening it would pull in unrelated files and scan more of the tree.

## Consequences

Easier:

- Arbitrary structured metadata travels with a task on disk, in a form
  that renders as a folded block in markdown viewers.
- The first consumer (calendar sync) has a stable place to store an event
  id keyed to the task.

Harder:

- The on-disk format gains one more construct that producers and
  consumers must agree on; the consumer extension documents the writer
  side in its own ADR.
- A heading carrying only a property block and no marker/timestamp is not
  reached by the pre-filter (accepted limitation).

## References

- Parser: [`src/parser.rs`](../../src/parser.rs) (the `org-properties`
  branch of the `CodeBlock` arm in `process_node`, and
  `parse_org_properties`).
- Task type: [`src/types.rs`](../../src/types.rs) (`Task.properties`).
- Amended: [ADR-0002](0002-supported-org-mode-subset.md) (supported
  subset; properties drawers were listed out of scope).
- Schema rule: [ADR-0015](0015-json-schema-evolution.md) (non-breaking
  optional field addition).
- Consumer-side format ADR: `markdown-org-vscode` ADR-0009.
