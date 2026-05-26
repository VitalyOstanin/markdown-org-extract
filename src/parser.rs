use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

use crate::clock::{calculate_total_minutes, extract_clocks, format_duration};
use crate::regex_limits::compile_bounded;
use crate::timestamp::{
    extract_created_normalized, extract_timestamp_normalized, normalize_weekdays,
    parse_timestamp_fields_normalized,
};
use crate::types::{Priority, Task, TaskType, MAX_DIAGNOSTIC_ITEMS};

// Per-call cap on invalid-timestamp warnings reuses `MAX_DIAGNOSTIC_ITEMS` so
// both diagnostic surfaces (failed-path list and parse-warning stream) stay
// aligned: "20 entries is already noisy". The counter is owned by the caller
// -- typically `ProcessingStats::ts_warnings_emitted` for a CLI run -- so
// long-running library use cases and parallel scans do not pollute each
// other's budget. The previous process-global `AtomicUsize` was replaced as
// part of the 0.5.0 review (M1).
fn warn_invalid_timestamp(counter: &mut usize, path: &Path, line: u32, ts: &str) {
    let n = *counter;
    *counter = counter.saturating_add(1);
    if n < MAX_DIAGNOSTIC_ITEMS {
        tracing::warn!(
            file = %path.display(),
            line,
            timestamp = ts.trim(),
            "cannot parse timestamp"
        );
    } else if n == MAX_DIAGNOSTIC_ITEMS {
        tracing::warn!(
            limit = MAX_DIAGNOSTIC_ITEMS,
            "more invalid timestamps suppressed (showed first {MAX_DIAGNOSTIC_ITEMS})"
        );
    }
}

/// Optional TODO/DONE keyword anchored to the start of a heading.
///
/// Matches `TODO` or `DONE` followed by at least one whitespace character.
/// Used as the first step of heading parsing — see `parse_heading`.
static HEADING_TODO_RE: LazyLock<Regex> = LazyLock::new(|| compile_bounded(r"^(TODO|DONE)\s+"));

/// Priority cookie `[#X]` with an optional trailing space, matching anywhere
/// in the heading text.
///
/// Mirrors emacs org-mode's `org-priority-regexp` semantics: the priority
/// cookie may appear at any position in the (remaining) heading title, and the
/// title content before it is dropped. The value is either an uppercase ASCII
/// letter or a one- or two-digit integer; the integer range is validated by
/// `Priority::parse` (only `0..=64` is accepted).
///
/// Two-digit alternatives are listed before single-digit `[0-9]` so the
/// matcher prefers the longest valid run (e.g. matches `15`, not just `1`).
static HEADING_PRIORITY_RE: LazyLock<Regex> =
    LazyLock::new(|| compile_bounded(r"\[#([A-Z]|6[0-4]|[1-5][0-9]|[0-9])\] ?"));

/// Extract tasks from markdown content with a caller-owned warning counter.
///
/// Production callers (see `main.rs::scan_files`) pass
/// `&mut ProcessingStats::ts_warnings_emitted` so the per-`MAX_DIAGNOSTIC_ITEMS`
/// cap on invalid-timestamp warnings spans every file in the run. Library
/// callers can pass their own counter to scope the budget per scan.
///
/// # Arguments
/// * `path` - Path to the markdown file. Stored verbatim in `Task.file` for output.
/// * `content` - File content (UTF-8).
/// * `mappings` - Weekday name mappings for localization.
/// * `max_tasks` - Per-file cap. Parsing stops as soon as this many tasks accumulate.
/// * `ts_warning_counter` - Mutable counter used to gate invalid-timestamp warnings.
///
/// # Returns
/// Vector of extracted tasks, capped at `max_tasks`.
pub fn extract_tasks_with_counter(
    path: &Path,
    content: &str,
    mappings: &[(&str, &str)],
    max_tasks: usize,
    ts_warning_counter: &mut usize,
) -> Vec<Task> {
    let arena = Arena::new();
    let root = parse_document(&arena, content, &safe_comrak_options());

    let mut tasks = Vec::new();
    let mut current_heading: Option<HeadingInfo> = None;

    for node in root.children() {
        process_node(
            node,
            path,
            &mut tasks,
            &mut current_heading,
            mappings,
            ts_warning_counter,
        );

        if tasks.len() >= max_tasks {
            tracing::warn!(
                file = %path.display(),
                limit = max_tasks,
                "reached per-file task limit"
            );
            break;
        }
    }

    // Flush remaining heading
    if let Some(info) = current_heading.take() {
        if let Some(task) = finalize_task(path, info, ts_warning_counter) {
            tasks.push(task);
        }
    }

    tracing::debug!(
        file = %path.display(),
        bytes = content.len(),
        tasks = tasks.len(),
        "parsed file"
    );

    tasks
}

/// Extract tasks from markdown content with a per-call warning budget.
///
/// Convenience wrapper around [`extract_tasks_with_counter`] that owns the
/// counter for the duration of one call. Used by the unit-test suite and
/// available to library callers that scope the invalid-timestamp warning
/// cap per file. The production CLI (`main.rs::scan_files`) uses
/// `extract_tasks_with_counter` directly so the cap spans the whole run.
#[cfg_attr(not(test), allow(dead_code))]
pub fn extract_tasks(
    path: &Path,
    content: &str,
    mappings: &[(&str, &str)],
    max_tasks: usize,
) -> Vec<Task> {
    let mut counter = 0_usize;
    extract_tasks_with_counter(path, content, mappings, max_tasks, &mut counter)
}

/// Comrak parsing options.
///
/// **Security note**: this is `Options::default()` deliberately. Defaults:
/// - `render.unsafe_ = false` — raw HTML in markdown is escaped, not passed through.
/// - `extension.tagfilter = false` (filter not applied, since unsafe HTML is already escaped).
/// - No extensions that interpret embedded HTML or scripts are enabled.
///
/// **Do not enable `render.unsafe_` or `extension.tagfilter` here without a
/// security review** — the HTML output goes through `html_escape`, but enabling
/// raw HTML would let untrusted markdown inject arbitrary tags into the rendered
/// page bypassing that escape.
fn safe_comrak_options() -> Options<'static> {
    Options::default()
}

/// Information extracted from a heading
struct HeadingInfo {
    heading: String,
    task_type: Option<TaskType>,
    priority: Option<Priority>,
    line: u32,
    content: String,
    created: Option<String>,
    timestamp: Option<String>,
    clocks: Vec<crate::types::ClockEntry>,
}

/// Process a single markdown node
fn process_node<'a>(
    node: &'a AstNode<'a>,
    path: &Path,
    tasks: &mut Vec<Task>,
    current_heading: &mut Option<HeadingInfo>,
    mappings: &[(&str, &str)],
    ts_warning_counter: &mut usize,
) {
    // Snapshot the borrow once — clone the value (cheap for Heading/Paragraph) and
    // read the sourcepos line in the same scope; drop before any code that
    // recurses into children (which take their own borrows).
    let (value_clone, line) = {
        let data = node.data.borrow();
        (data.value.clone(), data.sourcepos.start.line as u32)
    };
    match value_clone {
        NodeValue::Heading(_) => {
            // Finalize previous heading first
            if let Some(info) = current_heading.take() {
                if let Some(task) = finalize_task(path, info, ts_warning_counter) {
                    tasks.push(task);
                }
            }

            let text = extract_text(node);
            let (task_type, priority, heading) = parse_heading(&text);
            *current_heading = Some(HeadingInfo {
                heading,
                task_type,
                priority,
                line,
                content: String::new(),
                created: None,
                timestamp: None,
                clocks: Vec::new(),
            });
        }
        NodeValue::Paragraph => {
            if let Some(ref mut info) = current_heading {
                let (created, timestamp) = extract_timestamps_from_node(node, mappings);
                let content = extract_paragraph_text(node);

                for child in node.children() {
                    if let NodeValue::Code(code) = &child.data.borrow().value {
                        info.clocks.extend(extract_clocks(&code.literal));
                    }
                }

                if created.is_some() {
                    info.created = created;
                }
                if timestamp.is_some() {
                    info.timestamp = timestamp;
                }
                if !content.is_empty() {
                    if info.content.is_empty() {
                        info.content = content;
                    } else {
                        info.content.push_str("\n\n");
                        info.content.push_str(&content);
                    }
                }
            }
        }
        NodeValue::CodeBlock(code) => {
            if let Some(ref mut info) = current_heading {
                let raw = code.literal.trim();
                // An indented code block (4-space indent) reaches us with
                // the planning line still wrapped in inline-code backticks
                // (`    \`DEADLINE: <...>\``). Comrak strips the indent but
                // leaves the wrapping backticks in `code.literal`, which
                // would otherwise prevent the DEADLINE/SCHEDULED/CREATED
                // regex from anchoring on the keyword. Drop a matched
                // backtick pair before regex matching.
                let literal = strip_wrapping_backticks(raw);
                let normalized = normalize_weekdays(literal, mappings);
                let created = extract_created_normalized(&normalized);
                let timestamp = extract_timestamp_normalized(&normalized);

                info.clocks.extend(extract_clocks(literal));

                if created.is_some() {
                    info.created = created;
                }
                if timestamp.is_some() {
                    info.timestamp = timestamp;
                }
            }
        }
        _ => {}
    }
}

fn finalize_task(path: &Path, info: HeadingInfo, ts_warning_counter: &mut usize) -> Option<Task> {
    if info.task_type.is_none() && info.created.is_none() && info.timestamp.is_none() {
        return None;
    }

    let line = info.line;
    let (ts_type, ts_date, ts_time, ts_end_time, ts_active) = if let Some(ref ts) = info.timestamp {
        // `info.timestamp` is assembled from `extract_timestamp_normalized`
        // regex captures over an already-`normalize_weekdays`d string in
        // both `process_node` branches, so a second normalisation here
        // would be redundant work on every task.
        let parsed = parse_timestamp_fields_normalized(ts);
        if parsed.1.is_none() {
            warn_invalid_timestamp(ts_warning_counter, path, line, ts);
        }
        parsed
    } else {
        (None, None, None, None, None)
    };

    let (clocks_opt, total_time) = if !info.clocks.is_empty() {
        let total = calculate_total_minutes(&info.clocks).map(format_duration);
        (Some(info.clocks), total)
    } else {
        (None, None)
    };

    Some(Task {
        file: path.display().to_string(),
        line,
        heading: info.heading,
        content: info.content,
        task_type: info.task_type,
        priority: info.priority,
        created: info.created,
        timestamp: info.timestamp,
        timestamp_type: ts_type,
        timestamp_active: ts_active,
        timestamp_date: ts_date,
        timestamp_time: ts_time,
        timestamp_end_time: ts_end_time,
        clocks: clocks_opt,
        total_clock_time: total_time,
        properties: None,
    })
}

/// Parse heading text to extract task type, priority, and title.
///
/// Follows the emacs org-mode parser
/// (`org-element--headline-parse-title` / `org-priority-regexp`):
///
/// 1. Strip an optional `TODO` / `DONE` keyword anchored at the start.
/// 2. Search the remaining text for the first `[#X]` cookie at any position,
///    where `X` is `A-Z` or an integer `0..=64`. If found, that becomes the
///    priority; the title is everything after `[#X]` + optional space.
///    Text before the cookie (between TODO and `[#X]`) is **discarded** —
///    this matches emacs's `goto-char (match-end 0)` behaviour.
/// 3. Whatever remains is trimmed and returned as the heading.
///
/// A heading without TODO/DONE and without a priority cookie is returned
/// verbatim (trimmed).
fn parse_heading(text: &str) -> (Option<TaskType>, Option<Priority>, String) {
    // Step 1: optional TODO/DONE prefix.
    let (task_type, rest) = if let Some(caps) = HEADING_TODO_RE.captures(text) {
        let kw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let m = caps
            .get(0)
            .expect("Captures::get(0) is Some when captures() succeeds");
        (TaskType::from_keyword(kw), &text[m.end()..])
    } else {
        (None, text)
    };

    // Step 2: optional priority cookie anywhere in the remainder.
    if let Some(caps) = HEADING_PRIORITY_RE.captures(rest) {
        let value = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        if let Some(priority) = Priority::parse(value) {
            let whole = caps
                .get(0)
                .expect("Captures::get(0) is Some when captures() succeeds");
            let after = &rest[whole.end()..];
            return (task_type, Some(priority), after.trim().to_string());
        }
    }

    (task_type, None, rest.trim().to_string())
}

/// Strip a matched pair of inline-code backtick fences from the trimmed
/// content of an indented code block.
///
/// Markdown's indented code blocks preserve the literal source minus the
/// leading 4-space indent, so a line like `    \`DEADLINE: <...>\`` arrives
/// here with the wrapping backticks intact. Those wrappers are not part of
/// the planning-line keyword grammar — they're inline-code framing that the
/// user added to keep the line visually attached to the heading in their
/// editor — so we peel one balanced run of backticks before regex matching.
///
/// Returns `s` unchanged when the wrapping is asymmetric or absent.
fn strip_wrapping_backticks(s: &str) -> &str {
    let bytes = s.as_bytes();
    let n_leading = bytes.iter().take_while(|&&b| b == b'`').count();
    if n_leading == 0 {
        return s;
    }
    let n_trailing = bytes.iter().rev().take_while(|&&b| b == b'`').count();
    // Require equal-length fences with at least one non-fence byte between
    // them; otherwise the input is just a run of backticks and stripping
    // would over-consume.
    if n_trailing != n_leading || bytes.len() < 2 * n_leading + 1 {
        return s;
    }
    s[n_leading..bytes.len() - n_leading].trim()
}

/// Extract timestamps (CREATED and others) from paragraph node
fn extract_timestamps_from_node<'a>(
    node: &'a AstNode<'a>,
    mappings: &[(&str, &str)],
) -> (Option<String>, Option<String>) {
    let mut created = None;
    let mut timestamp = None;

    if let NodeValue::Paragraph = &node.data.borrow().value {
        for child in node.children() {
            if let NodeValue::Code(code) = &child.data.borrow().value {
                // Normalize the literal once per inline-code node; both extractors
                // would otherwise scan the same string in lockstep.
                let normalized = normalize_weekdays(&code.literal, mappings);
                if created.is_none() {
                    created = extract_created_normalized(&normalized);
                }
                if timestamp.is_none() {
                    timestamp = extract_timestamp_normalized(&normalized);
                }
            }
        }
    }
    (created, timestamp)
}

/// Extract plain text from paragraph, including text inside Emph/Strong/Link nodes
fn extract_paragraph_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    collect_text_recursive(node, &mut text);
    text.trim().to_string()
}

/// Extract all text from a heading node, including text inside Emph/Strong
fn extract_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    collect_text_recursive(node, &mut text);
    text
}

fn collect_text_recursive<'a>(node: &'a AstNode<'a>, out: &mut String) {
    for child in node.children() {
        let value = child.data.borrow().value.clone();
        match value {
            NodeValue::Text(t) => out.push_str(&t),
            NodeValue::Emph | NodeValue::Strong | NodeValue::Link(_) | NodeValue::Strikethrough => {
                collect_text_recursive(child, out)
            }
            _ => {}
        }
    }
}

// Need clone for NodeValue match — comrak nodes are RefCell-borrowed
// Re-import for clone derive if not present.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DEFAULT_MAX_TASKS;

    #[test]
    fn warn_invalid_timestamp_advances_per_call_counter() {
        // The 0.5.0 review (M1) replaced a process-global
        // `TS_WARNINGS_EMITTED: AtomicUsize` with a counter owned by the
        // caller (typically `ProcessingStats::ts_warnings_emitted`).
        // This test pins the per-call advance: each call bumps the
        // counter by exactly one.
        let mut counter = 0_usize;
        let path = Path::new("t.md");
        for i in 1..=25 {
            warn_invalid_timestamp(&mut counter, path, i, "<bad>");
        }
        assert_eq!(counter, 25);
    }

    #[test]
    fn warn_invalid_timestamp_counters_are_independent() {
        // Independent counters do not pollute each other: e.g. a library
        // consumer running two separate scans, or unit tests in the same
        // binary, each see a fresh budget. With the previous global
        // static this assertion would not hold across runs in one
        // process.
        let mut counter_a = 0_usize;
        let mut counter_b = 0_usize;
        let path = Path::new("t.md");
        for _ in 0..MAX_DIAGNOSTIC_ITEMS {
            warn_invalid_timestamp(&mut counter_a, path, 1, "<bad>");
        }
        warn_invalid_timestamp(&mut counter_b, path, 1, "<bad>");
        assert_eq!(counter_a, MAX_DIAGNOSTIC_ITEMS);
        assert_eq!(counter_b, 1);
    }

    #[test]
    fn test_parse_heading_with_priority() {
        let (task_type, priority, heading) = parse_heading("TODO [#A] Important task");
        assert_eq!(task_type, Some(TaskType::Todo));
        assert_eq!(priority, Some(Priority::A));
        assert_eq!(heading, "Important task");
    }

    #[test]
    fn test_parse_heading_without_priority() {
        let (task_type, priority, heading) = parse_heading("DONE Simple task");
        assert_eq!(task_type, Some(TaskType::Done));
        assert_eq!(priority, None);
        assert_eq!(heading, "Simple task");
    }

    #[test]
    fn test_parse_heading_no_task() {
        let (task_type, priority, heading) = parse_heading("Regular heading");
        assert_eq!(task_type, None);
        assert_eq!(priority, None);
        assert_eq!(heading, "Regular heading");
    }

    // The next batch mirrors the test matrix from the bug report
    // "markdown-org-extract: приоритет `[#X]` без TODO/DONE не распознаётся".
    // We follow emacs org-mode semantics (`org-priority-regexp`) wherever the
    // bug report diverged from it — concretely case 8 below.

    #[test]
    fn parse_heading_priority_without_todo() {
        // Case 1: `### [#A] Заголовок`.
        let (tt, p, h) = parse_heading("[#A] Заголовок");
        assert_eq!(tt, None);
        assert_eq!(p, Some(Priority::A));
        assert_eq!(h, "Заголовок");
    }

    #[test]
    fn parse_heading_todo_with_priority() {
        // Case 2: `### TODO [#A] Заголовок`.
        let (tt, p, h) = parse_heading("TODO [#A] Заголовок");
        assert_eq!(tt, Some(TaskType::Todo));
        assert_eq!(p, Some(Priority::A));
        assert_eq!(h, "Заголовок");
    }

    #[test]
    fn parse_heading_done_with_priority_b() {
        // Case 3: `### DONE [#B] Заголовок`.
        let (tt, p, h) = parse_heading("DONE [#B] Заголовок");
        assert_eq!(tt, Some(TaskType::Done));
        assert_eq!(p, Some(Priority::B));
        assert_eq!(h, "Заголовок");
    }

    #[test]
    fn parse_heading_plain_text_no_markers() {
        // Case 4: `### Заголовок`.
        let (tt, p, h) = parse_heading("Заголовок");
        assert_eq!(tt, None);
        assert_eq!(p, None);
        assert_eq!(h, "Заголовок");
    }

    #[test]
    fn parse_heading_todo_no_priority() {
        // Case 5: `### TODO Заголовок`.
        let (tt, p, h) = parse_heading("TODO Заголовок");
        assert_eq!(tt, Some(TaskType::Todo));
        assert_eq!(p, None);
        assert_eq!(h, "Заголовок");
    }

    #[test]
    fn parse_heading_numeric_priority() {
        // Case 6: `### [#1] Заголовок`.
        let (tt, p, h) = parse_heading("[#1] Заголовок");
        assert_eq!(tt, None);
        assert_eq!(p, Some(Priority::Numeric(1)));
        assert_eq!(h, "Заголовок");
    }

    #[test]
    fn parse_heading_extra_whitespace_around_priority() {
        // Case 7: `###     [#A]     Заголовок` — comrak normalises the leading
        // whitespace after the `###` marker, so the heading text reaching us
        // starts at `[#A]`. Trailing extra spaces around the heading are
        // trimmed.
        let (tt, p, h) = parse_heading("[#A]     Заголовок");
        assert_eq!(tt, None);
        assert_eq!(p, Some(Priority::A));
        assert_eq!(h, "Заголовок");
    }

    #[test]
    fn parse_heading_priority_in_the_middle_org_semantics() {
        // Case 8: `### Без приоритета и [#A] внутри`.
        // The bug report's table proposed keeping `[#A]` as part of the
        // heading, but emacs org-mode parses any `[#X]` cookie inside the
        // title as the priority via the `.*?` prefix in `org-priority-regexp`
        // and drops the text that precedes it. By project decision we follow
        // the reference parser here.
        let (tt, p, h) = parse_heading("Без приоритета и [#A] внутри");
        assert_eq!(tt, None);
        assert_eq!(p, Some(Priority::A));
        assert_eq!(h, "внутри");
    }

    #[test]
    fn parse_heading_two_digit_numeric_priority() {
        let (tt, p, h) = parse_heading("[#15] Mid range");
        assert_eq!(tt, None);
        assert_eq!(p, Some(Priority::Numeric(15)));
        assert_eq!(h, "Mid range");

        let (tt, p, h) = parse_heading("[#64] At upper bound");
        assert_eq!(tt, None);
        assert_eq!(p, Some(Priority::Numeric(64)));
        assert_eq!(h, "At upper bound");
    }

    #[test]
    fn parse_heading_rejects_numeric_out_of_range() {
        // `[#65]` and higher are not a valid org-mode priority. The cookie
        // stays inside the heading text verbatim.
        let (tt, p, h) = parse_heading("[#65] Above range");
        assert_eq!(tt, None);
        assert_eq!(p, None);
        assert_eq!(h, "[#65] Above range");
    }

    #[test]
    fn parse_heading_rejects_lowercase_priority() {
        let (tt, p, h) = parse_heading("[#a] Lowercase");
        assert_eq!(tt, None);
        assert_eq!(p, None);
        assert_eq!(h, "[#a] Lowercase");
    }

    #[test]
    fn parse_heading_todo_then_priority_with_intervening_text() {
        // Direct consequence of the emacs `.*?` semantics: the text between
        // TODO and `[#X]` is discarded.
        let (tt, p, h) = parse_heading("TODO Купить [#A] фильтр");
        assert_eq!(tt, Some(TaskType::Todo));
        assert_eq!(p, Some(Priority::A));
        assert_eq!(h, "фильтр");
    }

    #[test]
    fn parse_heading_priority_without_trailing_space() {
        // `\] ?` makes the post-cookie space optional.
        let (tt, p, h) = parse_heading("[#A]NoSpace");
        assert_eq!(tt, None);
        assert_eq!(p, Some(Priority::A));
        assert_eq!(h, "NoSpace");
    }

    #[test]
    fn extract_tasks_marks_scheduled_angle_bracket_as_active() {
        // End-to-end: a SCHEDULED line with `<...>` must surface
        // `timestamp_active = Some(true)` in the resulting Task, so
        // downstream consumers can branch on bracket form without
        // re-parsing the timestamp string. See ADR-0014.
        let content = "### TODO Pin me\n`SCHEDULED: <2026-05-21 Thu>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].timestamp_active, Some(true));
    }

    #[test]
    fn extract_tasks_marks_missing_timestamp_active_as_none() {
        // Heading without a timestamp must keep `timestamp_active = None`,
        // matching the rule that absent optional fields skip JSON
        // serialisation (ADR-0015).
        let content = "### Project kickoff\n\n`CREATED: [2025-09-01 Mon]`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].timestamp_active, None);
    }

    #[test]
    fn extract_tasks_basic_todo_with_deadline() {
        let content = "\
### TODO [#A] Write docs\n\
`DEADLINE: <2025-12-10 Wed>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.task_type, Some(TaskType::Todo));
        assert_eq!(t.priority, Some(Priority::A));
        assert_eq!(t.heading, "Write docs");
        assert_eq!(t.timestamp_type, Some("DEADLINE".to_string()));
        assert_eq!(t.timestamp_date, Some("2025-12-10".to_string()));
    }

    #[test]
    fn extract_tasks_extracts_emph_text_in_heading() {
        // Regression: previously emphasised text inside heading was dropped.
        let content = "### TODO **Important** task\n`DEADLINE: <2025-12-10 Wed>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].heading, "Important task");
    }

    #[test]
    fn extract_tasks_ignores_non_task_headings_without_timestamps() {
        let content = "### Just a heading\n\nSome text.\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert!(tasks.is_empty());
    }

    #[test]
    fn extract_tasks_keeps_created_without_todo() {
        // Heading without TODO/DONE keyword but with a CREATED line is still a task.
        let content = "### Project kickoff\n\n`CREATED: [2025-09-01 Mon]`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_type, None);
        assert_eq!(tasks[0].created, Some("CREATED: [2025-09-01 Mon]".into()));
    }

    #[test]
    fn extract_tasks_concatenates_multiple_paragraphs() {
        // Regression: previously only the first paragraph was kept as content.
        let content = "\
### TODO Multi-line task\n\
First paragraph.\n\
\n\
Second paragraph.\n\
";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        assert!(tasks[0].content.contains("First paragraph"));
        assert!(tasks[0].content.contains("Second paragraph"));
    }

    #[test]
    fn extract_tasks_extracts_clock_from_inline_code() {
        let content = "\
### TODO Track time\n\
`CLOCK: [2025-09-01 Mon 10:00]--[2025-09-01 Mon 11:30] => 1:30`\n\
";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert!(t.clocks.is_some());
        assert_eq!(t.total_clock_time.as_deref(), Some("1:30"));
    }

    #[test]
    fn extract_tasks_handles_done_priority() {
        let content = "### DONE [#B] Wrap up\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_type, Some(TaskType::Done));
        assert_eq!(tasks[0].priority, Some(Priority::B));
    }

    #[test]
    fn extract_tasks_priority_without_todo_with_scheduled() {
        // Bug report case: priority cookie before SCHEDULED heading, no TODO.
        // After the fix the heading must surface as a task with priority=A and
        // task_type=None, since the SCHEDULED line is what makes it agenda-eligible.
        let content = "\
### [#A] Поменять резину до 16.05.2026\n\
`SCHEDULED: <2026-05-09 Sat>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.task_type, None);
        assert_eq!(t.priority, Some(Priority::A));
        assert_eq!(t.heading, "Поменять резину до 16.05.2026");
        assert_eq!(t.timestamp_type, Some("SCHEDULED".to_string()));
        assert_eq!(t.timestamp_date, Some("2026-05-09".to_string()));
    }

    #[test]
    fn extract_tasks_numeric_priority_with_deadline() {
        // Numeric priority `[#1]` without TODO, with a DEADLINE line.
        let content = "\
### [#1] Numeric priority task\n\
`DEADLINE: <2026-05-09 Sat>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.task_type, None);
        assert_eq!(t.priority, Some(Priority::Numeric(1)));
        assert_eq!(t.heading, "Numeric priority task");
    }

    #[test]
    fn extract_tasks_priority_in_middle_drops_prefix() {
        // Org-mode `.*?` semantics: text before `[#A]` is dropped.
        let content = "\
### Без приоритета и [#A] внутри\n\
`SCHEDULED: <2026-05-09 Sat>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.task_type, None);
        assert_eq!(t.priority, Some(Priority::A));
        assert_eq!(t.heading, "внутри");
    }

    #[test]
    fn extract_tasks_bug_report_minimal_reproduction() {
        // Three-heading reproduction from the bug report: priority without
        // TODO, TODO + priority, plain heading. SCHEDULED is wrapped in
        // backticks so the existing inline-code parser picks it up.
        let content = "\
### [#A] Поменять резину до 16.05.2026\n\
`SCHEDULED: <2026-05-09 Sat>`\n\
\n\
### TODO [#A] Поменять масло\n\
`SCHEDULED: <2026-05-09 Sat>`\n\
\n\
### Купить фильтр\n\
`SCHEDULED: <2026-05-09 Sat>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 3);

        assert_eq!(tasks[0].task_type, None);
        assert_eq!(tasks[0].priority, Some(Priority::A));
        assert_eq!(tasks[0].heading, "Поменять резину до 16.05.2026");

        assert_eq!(tasks[1].task_type, Some(TaskType::Todo));
        assert_eq!(tasks[1].priority, Some(Priority::A));
        assert_eq!(tasks[1].heading, "Поменять масло");

        assert_eq!(tasks[2].task_type, None);
        assert_eq!(tasks[2].priority, None);
        assert_eq!(tasks[2].heading, "Купить фильтр");
    }

    // Regression suite for the "indented planning line" cases. A heading
    // followed by a 4-space-indented DEADLINE/SCHEDULED/CREATED line is
    // parsed by comrak as an indented code block; the timestamp must still
    // be recovered, whether or not the planning line is wrapped in inline
    // backticks. Matches what `emacs` org-agenda surfaces.
    // The literals below use real newlines (no `\\\n` Rust string
    // continuation): the continuation form would silently swallow the
    // four leading spaces and reduce the case to "no indent at all".
    #[test]
    fn extract_tasks_indented_inline_code_deadline() {
        let content = "#### Birthday\n    `DEADLINE: <2026-05-07 Thu +1y>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1, "task should not be dropped");
        let t = &tasks[0];
        assert_eq!(t.timestamp_type.as_deref(), Some("DEADLINE"));
        assert_eq!(t.timestamp_date.as_deref(), Some("2026-05-07"));
    }

    #[test]
    fn extract_tasks_todo_indented_inline_code_deadline() {
        let content = "#### TODO Birthday\n    `DEADLINE: <2026-05-07 Thu +1y>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.task_type, Some(TaskType::Todo));
        assert_eq!(t.timestamp_date.as_deref(), Some("2026-05-07"));
    }

    #[test]
    fn extract_tasks_indented_inline_code_blank_lines_between() {
        // Whitespace between heading and planning line (blank lines, tabs,
        // mixed indentation) must not block timestamp recovery.
        let content = "#### Birthday\n\n  \t  \n    `DEADLINE: <2026-05-07 Thu +1y>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.timestamp_date.as_deref(), Some("2026-05-07"));
    }

    #[test]
    fn extract_tasks_with_ru_mappings_reproduces_cli_pipeline() {
        // Reproduce what `main.rs` feeds to `extract_tasks` when the default
        // `--locale ru,en` is in effect. Pulling the table from `cli` keeps
        // this test in sync with whatever `get_weekday_mappings("ru")` would
        // produce in production.
        let content = "#### TODO Birthday\n    `DEADLINE: <2026-05-07 Thu +1y>`\n";
        let tasks = extract_tasks(
            Path::new("t.md"),
            content,
            crate::cli::RU_WEEKDAY_MAPPINGS,
            DEFAULT_MAX_TASKS,
        );
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.task_type, Some(TaskType::Todo));
        assert_eq!(t.timestamp_date.as_deref(), Some("2026-05-07"));
    }

    #[test]
    fn extract_tasks_inline_code_scheduled_no_indent() {
        let content = "#### Followup\n`SCHEDULED: <2026-05-07 Thu>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.timestamp_type.as_deref(), Some("SCHEDULED"));
        assert_eq!(t.timestamp_date.as_deref(), Some("2026-05-07"));
    }

    #[test]
    fn extract_tasks_indented_inline_code_created() {
        let content = "#### Project kickoff\n    `CREATED: [2025-09-01 Mon]`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.created.as_deref(), Some("CREATED: [2025-09-01 Mon]"));
    }
}
