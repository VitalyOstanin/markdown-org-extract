use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;

use crate::timestamp::{extract_created, extract_timestamp, parse_timestamp_fields};
use crate::types::{Priority, Task, TaskType, MAX_TASKS};

/// Regex for parsing task headings: TODO/DONE [#A] Task title
static HEADING_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(TODO|DONE)\s+(?:\[#([A-Z])\]\s+)?(.+)$")
        .expect("Invalid HEADING_RE regex")
});

/// Extract tasks from markdown content
///
/// # Arguments
/// * `path` - Path to the markdown file
/// * `content` - File content
/// * `mappings` - Weekday name mappings for localization
///
/// # Returns
/// Vector of extracted tasks (limited to MAX_TASKS)
pub fn extract_tasks(path: &Path, content: &str, mappings: &[(&str, &str)]) -> Vec<Task> {
    let arena = Arena::new();
    let root = parse_document(&arena, content, &Options::default());

    let mut tasks = Vec::new();
    let mut current_heading: Option<HeadingInfo> = None;

    for node in root.children() {
        process_node(node, path, &mut tasks, &mut current_heading, mappings);
        
        // Safety limit to prevent memory exhaustion
        if tasks.len() >= MAX_TASKS {
            eprintln!("Warning: Reached maximum task limit ({}) in {}", MAX_TASKS, path.display());
            break;
        }
    }

    tasks
}

/// Information extracted from a heading
struct HeadingInfo {
    heading: String,
    task_type: Option<TaskType>,
    priority: Option<Priority>,
    line: u32,
}

/// Process a single markdown node
fn process_node<'a>(
    node: &'a AstNode<'a>,
    path: &Path,
    tasks: &mut Vec<Task>,
    current_heading: &mut Option<HeadingInfo>,
    mappings: &[(&str, &str)],
) {
    match &node.data.borrow().value {
        NodeValue::Heading(_) => {
            let text = extract_text(node);
            let (task_type, priority, heading) = parse_heading(&text);
            let line = node.data.borrow().sourcepos.start.line as u32;
            *current_heading = Some(HeadingInfo {
                heading,
                task_type,
                priority,
                line,
            });
        }
        NodeValue::Paragraph => {
            if let Some(ref info) = current_heading {
                let (created, timestamp) = extract_timestamps_from_node(node, mappings);

                if created.is_some() || timestamp.is_some() {
                    let content = extract_paragraph_text(node);
                    let (ts_type, ts_date, ts_time, ts_end_time) = if let Some(ref ts) = timestamp {
                        parse_timestamp_fields(ts, mappings)
                    } else {
                        (None, None, None, None)
                    };

                    tasks.push(Task {
                        file: path.display().to_string(),
                        line: info.line,
                        heading: info.heading.clone(),
                        content,
                        task_type: info.task_type.clone(),
                        priority: info.priority.clone(),
                        created,
                        timestamp,
                        timestamp_type: ts_type,
                        timestamp_date: ts_date,
                        timestamp_time: ts_time,
                        timestamp_end_time: ts_end_time,
                    });
                    *current_heading = None;
                } else if info.task_type.is_some() {
                    tasks.push(Task {
                        file: path.display().to_string(),
                        line: info.line,
                        heading: info.heading.clone(),
                        content: String::new(),
                        task_type: info.task_type.clone(),
                        priority: info.priority.clone(),
                        created: None,
                        timestamp: None,
                        timestamp_type: None,
                        timestamp_date: None,
                        timestamp_time: None,
                        timestamp_end_time: None,
                    });
                    *current_heading = None;
                }
            }
        }
        NodeValue::CodeBlock(code) => {
            if let Some(ref info) = current_heading {
                let literal = code.literal.trim().trim_matches('`');
                let created = extract_created(literal, mappings);
                let timestamp = extract_timestamp(literal, mappings);

                if created.is_some() || timestamp.is_some() {
                    let (ts_type, ts_date, ts_time, ts_end_time) = if let Some(ref ts) = timestamp {
                        parse_timestamp_fields(ts, mappings)
                    } else {
                        (None, None, None, None)
                    };

                    tasks.push(Task {
                        file: path.display().to_string(),
                        line: info.line,
                        heading: info.heading.clone(),
                        content: String::new(),
                        task_type: info.task_type.clone(),
                        priority: info.priority.clone(),
                        created,
                        timestamp,
                        timestamp_type: ts_type,
                        timestamp_date: ts_date,
                        timestamp_time: ts_time,
                        timestamp_end_time: ts_end_time,
                    });
                    *current_heading = None;
                } else if info.task_type.is_some() {
                    tasks.push(Task {
                        file: path.display().to_string(),
                        line: info.line,
                        heading: info.heading.clone(),
                        content: String::new(),
                        task_type: info.task_type.clone(),
                        priority: info.priority.clone(),
                        created: None,
                        timestamp: None,
                        timestamp_type: None,
                        timestamp_date: None,
                        timestamp_time: None,
                        timestamp_end_time: None,
                    });
                    *current_heading = None;
                }
            }
        }
        _ => {}
    }
}

/// Parse heading text to extract task type, priority, and title
fn parse_heading(text: &str) -> (Option<TaskType>, Option<Priority>, String) {
    if let Some(caps) = HEADING_RE.captures(text) {
        let task_type = TaskType::from_str(&caps[1]);
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

/// Extract plain text from paragraph (excluding code blocks)
fn extract_paragraph_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    for child in node.children() {
        if let NodeValue::Text(t) = &child.data.borrow().value {
            text.push_str(t);
        }
    }
    text.trim().to_string()
}

/// Extract all text from a node (for headings)
fn extract_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    for child in node.children() {
        if let NodeValue::Text(ref t) = child.data.borrow().value {
            text.push_str(t);
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comrak_indented_code() {
        let arena = Arena::new();
        let content = "### День Рождения тест\n    `DEADLINE: <2025-12-11 Thu +1y>`\n";
        let root = parse_document(&arena, content, &Options::default());
        
        eprintln!("=== Tree structure ===");
        for (i, node) in root.children().enumerate() {
            eprintln!("Child {}: {:?}", i, node.data.borrow().value);
            for (j, child) in node.children().enumerate() {
                eprintln!("  Grandchild {}: {:?}", j, child.data.borrow().value);
            }
        }
        
        let mut found_codeblock = false;
        let mut found_paragraph = false;
        
        for node in root.descendants() {
            match &node.data.borrow().value {
                NodeValue::CodeBlock(c) => {
                    found_codeblock = true;
                    eprintln!("CodeBlock: {:?}", c.literal);
                }
                NodeValue::Paragraph => {
                    found_paragraph = true;
                    eprintln!("Paragraph found");
                }
                _ => {}
            }
        }
        
        eprintln!("found_codeblock={}, found_paragraph={}", found_codeblock, found_paragraph);
        assert!(found_codeblock || found_paragraph, "Should find either code block or paragraph");
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
}
