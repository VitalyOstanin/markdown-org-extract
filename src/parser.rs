use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

use crate::clock::{calculate_total_minutes, extract_clocks, format_duration};
use crate::timestamp::{extract_created, extract_timestamp, parse_timestamp_fields};
use crate::types::{Priority, Task, TaskType, MAX_TASKS};

/// Regex for parsing task headings: `TODO/DONE [#A] Task title`
static HEADING_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(TODO|DONE)\s+(?:\[#([A-Z])\]\s+)?(.+)$").expect("Invalid HEADING_RE regex")
});

/// Extract tasks from markdown content.
///
/// # Arguments
/// * `path` - Path to the markdown file. Stored verbatim in `Task.file` for output.
/// * `content` - File content (UTF-8).
/// * `mappings` - Weekday name mappings for localization.
///
/// # Returns
/// Vector of extracted tasks, capped at `MAX_TASKS` per file.
pub fn extract_tasks(path: &Path, content: &str, mappings: &[(&str, &str)]) -> Vec<Task> {
    let arena = Arena::new();
    let root = parse_document(&arena, content, &safe_comrak_options());

    let mut tasks = Vec::new();
    let mut current_heading: Option<HeadingInfo> = None;

    for node in root.children() {
        process_node(node, &mut tasks, &mut current_heading, mappings);

        if tasks.len() >= MAX_TASKS {
            eprintln!(
                "Warning: reached per-file task limit ({MAX_TASKS}) in {}",
                path.display()
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

/// Comrak parsing options. Currently `Options::default()` — raw HTML stays escaped.
/// Wrapped in a helper to make the security-critical default explicit.
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
    tasks: &mut Vec<Task>,
    current_heading: &mut Option<HeadingInfo>,
    mappings: &[(&str, &str)],
) {
    // Take the borrow only once
    let value_clone = node.data.borrow().value.clone();
    match value_clone {
        NodeValue::Heading(_) => {
            // Finalize previous heading first
            if let Some(info) = current_heading.take() {
                if let Some(task) = finalize_task_no_path(info, mappings) {
                    tasks.push(task);
                }
            }

            let text = extract_text(node);
            let (task_type, priority, heading) = parse_heading(&text);
            let line = node.data.borrow().sourcepos.start.line as u32;
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
                if !content.is_empty() && info.content.is_empty() {
                    info.content = content;
                }
            }
        }
        NodeValue::CodeBlock(code) => {
            if let Some(ref mut info) = current_heading {
                let literal = code.literal.trim().trim_matches('`');
                let created = extract_created(literal, mappings);
                let timestamp = extract_timestamp(literal, mappings);

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

/// Finalize heading info into a task (path-agnostic version)
fn finalize_task_no_path(info: HeadingInfo, mappings: &[(&str, &str)]) -> Option<Task> {
    if info.task_type.is_none() && info.created.is_none() && info.timestamp.is_none() {
        return None;
    }

    let (ts_type, ts_date, ts_time, ts_end_time) = if let Some(ref ts) = info.timestamp {
        parse_timestamp_fields(ts, mappings)
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
        file: String::new(), // filled in by caller
        line: info.line,
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

fn finalize_task(path: &Path, info: HeadingInfo, mappings: &[(&str, &str)]) -> Option<Task> {
    let mut t = finalize_task_no_path(info, mappings)?;
    t.file = path.display().to_string();
    Some(t)
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
                if created.is_none() {
                    created = extract_created(&code.literal, mappings);
                }
                if timestamp.is_none() {
                    timestamp = extract_timestamp(&code.literal, mappings);
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
        let tasks = extract_tasks(Path::new("t.md"), content, &[]);
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
        let tasks = extract_tasks(Path::new("t.md"), content, &[]);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].heading, "Important task");
    }

    #[test]
    fn extract_tasks_ignores_non_task_headings_without_timestamps() {
        let content = "### Just a heading\n\nSome text.\n";
        let tasks = extract_tasks(Path::new("t.md"), content, &[]);
        assert!(tasks.is_empty());
    }
}
