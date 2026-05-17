use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};
use regex::Regex;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;

use crate::clock::{calculate_total_minutes, extract_clocks, format_duration};
use crate::regex_limits::compile_bounded;
use crate::timestamp::{
    extract_created_normalized, extract_timestamp_normalized, normalize_weekdays,
    parse_timestamp_fields,
};
use crate::types::{Priority, Task, TaskType};

/// Cap on how many invalid-timestamp warnings we emit per process. Without a cap
/// a corrupt or hostile input could flood stderr; the count is global because
/// warnings come from per-file parsing and we want one bound across all files.
const MAX_TS_WARNINGS: usize = 20;
static TS_WARNINGS_EMITTED: AtomicUsize = AtomicUsize::new(0);

fn warn_invalid_timestamp(path: &Path, line: u32, ts: &str) {
    let n = TS_WARNINGS_EMITTED.fetch_add(1, Ordering::Relaxed);
    if n < MAX_TS_WARNINGS {
        tracing::warn!(
            file = %path.display(),
            line,
            timestamp = ts.trim(),
            "cannot parse timestamp"
        );
    } else if n == MAX_TS_WARNINGS {
        tracing::warn!(
            limit = MAX_TS_WARNINGS,
            "more invalid timestamps suppressed (showed first {MAX_TS_WARNINGS})"
        );
    }
}

/// Regex for parsing task headings: `TODO/DONE [#A] Task title`
static HEADING_RE: LazyLock<Regex> =
    LazyLock::new(|| compile_bounded(r"^(TODO|DONE)\s+(?:\[#([A-Z])\]\s+)?(.+)$"));

/// Extract tasks from markdown content.
///
/// # Arguments
/// * `path` - Path to the markdown file. Stored verbatim in `Task.file` for output.
/// * `content` - File content (UTF-8).
/// * `mappings` - Weekday name mappings for localization.
/// * `max_tasks` - Per-file cap. Parsing stops as soon as this many tasks accumulate.
///
/// # Returns
/// Vector of extracted tasks, capped at `max_tasks`.
pub fn extract_tasks(
    path: &Path,
    content: &str,
    mappings: &[(&str, &str)],
    max_tasks: usize,
) -> Vec<Task> {
    let arena = Arena::new();
    let root = parse_document(&arena, content, &safe_comrak_options());

    let mut tasks = Vec::new();
    let mut current_heading: Option<HeadingInfo> = None;

    for node in root.children() {
        process_node(node, path, &mut tasks, &mut current_heading, mappings);

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
        if let Some(task) = finalize_task(path, info, mappings) {
            tasks.push(task);
        }
    }

    tasks
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
                if let Some(task) = finalize_task(path, info, mappings) {
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

fn finalize_task(path: &Path, info: HeadingInfo, mappings: &[(&str, &str)]) -> Option<Task> {
    if info.task_type.is_none() && info.created.is_none() && info.timestamp.is_none() {
        return None;
    }

    let line = info.line;
    let (ts_type, ts_date, ts_time, ts_end_time) = if let Some(ref ts) = info.timestamp {
        let parsed = parse_timestamp_fields(ts, mappings);
        if parsed.1.is_none() {
            warn_invalid_timestamp(path, line, ts);
        }
        parsed
    } else {
        (None, None, None, None)
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
        timestamp_date: ts_date,
        timestamp_time: ts_time,
        timestamp_end_time: ts_end_time,
        clocks: clocks_opt,
        total_clock_time: total_time,
    })
}

/// Parse heading text to extract task type, priority, and title
fn parse_heading(text: &str) -> (Option<TaskType>, Option<Priority>, String) {
    if let Some(caps) = HEADING_RE.captures(text) {
        let task_type = TaskType::from_keyword(&caps[1]);
        let priority = caps
            .get(2)
            .and_then(|m| m.as_str().chars().next())
            .and_then(Priority::from_char);
        let heading = caps[3].to_string();
        (task_type, priority, heading)
    } else {
        (None, None, text.to_string())
    }
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
        let content = "### Project kickoff\n\n`CREATED: <2025-09-01 Mon>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_type, None);
        assert_eq!(tasks[0].created, Some("CREATED: <2025-09-01 Mon>".into()));
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
        // `--locale ru,en` is in effect. Mappings must not interfere with
        // the indented-inline-code DEADLINE recovery.
        let content = "#### TODO Birthday\n    `DEADLINE: <2026-05-07 Thu +1y>`\n";
        let mappings: &[(&str, &str)] = &[
            ("Понедельник", "Monday"),
            ("Вторник", "Tuesday"),
            ("Среда", "Wednesday"),
            ("Четверг", "Thursday"),
            ("Пятница", "Friday"),
            ("Суббота", "Saturday"),
            ("Воскресенье", "Sunday"),
            ("Пн", "Mon"),
            ("Вт", "Tue"),
            ("Ср", "Wed"),
            ("Чт", "Thu"),
            ("Пт", "Fri"),
            ("Сб", "Sat"),
            ("Вс", "Sun"),
        ];
        let tasks = extract_tasks(Path::new("t.md"), content, mappings, DEFAULT_MAX_TASKS);
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
        let content = "#### Project kickoff\n    `CREATED: <2025-09-01 Mon>`\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[], DEFAULT_MAX_TASKS);
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.created.as_deref(), Some("CREATED: <2025-09-01 Mon>"));
    }
}
