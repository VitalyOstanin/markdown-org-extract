# ADR-0003: CLOCK metadata support

## Table of Contents

- [Status](#status)
- [Context](#context)
- [Decision](#decision)
- [Consequences](#consequences)
- [References](#references)

## Status

Accepted.

## Context

Org-mode tracks time spent on a task with `CLOCK:` entries that
mirror the output of Emacs' `org-clock-in` / `org-clock-out`
commands. A typical task block looks like:

```
*** TODO Write report
    SCHEDULED: <2024-12-10 Tue>
    CLOCK: [2024-12-09 Mon 10:00]--[2024-12-09 Mon 12:30] =>  2:30
    CLOCK: [2024-12-09 Mon 14:00]--[2024-12-09 Mon 16:15] =>  2:15
```

For a CLI that powers an agenda on top of markdown files, surfacing
this information unlocks:

- Total time per task and per file in JSON / Markdown / HTML output.
- Per-day time-tracking views in consumers.
- Round-trip with Emacs `org-clock-report`.

Markdown producers wrap the lines in inline code so they render as a
styled token instead of plain text:

```markdown
`CLOCK: <2024-12-09 Mon 10:00>--<2024-12-09 Mon 12:30> => 2:30`
`CLOCK: <2024-12-09 Mon 14:00>`
```

Skipping CLOCK would either force every consumer to re-implement the
parser or to hand-roll regex inside a UI; neither is acceptable.

## Decision

The scanner parses `CLOCK:` lines and exposes them on each task.

Recognised forms:

- **Closed CLOCK** with start, end, and duration:
  `CLOCK: [2024-12-09 Mon 10:00]--[2024-12-09 Mon 12:30] =>  2:30`.
- **Open CLOCK** with only a start time:
  `CLOCK: [2024-12-09 Mon 14:00]`.
- Both square brackets `[...]` and angle brackets `<...>` are
  accepted, so files written by either Emacs or the markdown-side
  producers parse identically.
- CLOCK lines are recognised both inside inline code (backticks) and
  inside fenced code blocks.

Data shape (Rust):

```rust
pub struct ClockEntry {
    pub start: String,
    pub end: Option<String>,
    pub duration: Option<String>,
}

pub struct Task {
    // ... other fields
    pub clocks: Option<Vec<ClockEntry>>,
    pub total_clock_time: Option<String>,
}
```

`total_clock_time` is the sum of durations from closed CLOCK
entries, formatted as `H:MM` to match
`org-clock-update-time-maybe`. Open CLOCK entries contribute
nothing to the total.

Render output:

- JSON: `clocks` and `total_clock_time` are emitted as optional
  fields; tasks without CLOCK keep their previous shape.
- Markdown: a `**Clock:**` bullet list plus a `**Total Time:**`
  line, both omitted when there are no CLOCK entries.
- HTML: equivalent of the Markdown rendering, structured as a `<ul>`
  list plus a `<p>` paragraph for total time.

## Consequences

Easier:

- Consumers (`markdown-org-vscode` and others) can render
  per-task time tracking without re-parsing CLOCK strings.
- Files round-trip through Emacs `org-clock-report` because both
  bracket styles and both placements (inline / fenced) are accepted
  on input.
- Backward compatibility: `clocks` and `total_clock_time` are
  optional, so existing consumers that only look at the older fields
  keep working.

Harder:

- The total-time calculation assumes Emacs' "minutes since midnight,
  rolled forward across midnight" semantics. Edge cases (timezone
  shifts inside a CLOCK span, DST) are not handled and may need a
  dedicated ADR if they come up.
- Two bracket styles double the regex surface for CLOCK, which
  makes the patterns slightly harder to maintain. The trade-off was
  accepted in favour of round-trip with Emacs.

## References

- CLOCK extraction and total-time math: [`src/clock.rs`](../../src/clock.rs)
- Task struct: [`src/types.rs`](../../src/types.rs)
- Parser integration: [`src/parser.rs`](../../src/parser.rs)
- Render output: [`src/render.rs`](../../src/render.rs)
- Producer-side wire format: [ADR-0003 in markdown-org-vscode](https://github.com/VitalyOstanin/markdown-org-vscode/blob/master/docs/adr/0003-org-mode-wire-format.md)
- Upstream `org-clock-update-time-maybe`: [orgmode.org docs on clocking](https://orgmode.org/manual/Clocking-Work-Time.html)
