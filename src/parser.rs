use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::PathBuf;

use crate::timestamp::{extract_created, extract_timestamp, parse_timestamp_fields};
use crate::types::{Priority, Task, TaskType};

static HEADING_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(TODO|DONE)\s+(?:\[#([A-Z])\]\s+)?(.+)$").unwrap()
});

pub fn extract_tasks(path: &PathBuf, content: &str, mappings: &[(&str, &str)]) -> Vec<Task> {
    let arena = Arena::new();
    let root = parse_document(&arena, content, &Options::default());

    let mut tasks = Vec::new();
    let mut current_heading: Option<HeadingInfo> = None;
    
    for node in root.children() {
        process_node(node, path, &mut tasks, &mut current_heading, mappings);
    }
    
    tasks
}

struct HeadingInfo {
    heading: String,
    task_type: Option<TaskType>,
    priority: Option<Priority>,
    line: u32,
}

fn process_node<'a>(
    node: &'a AstNode<'a>,
    path: &PathBuf,
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
            if let Some(info) = current_heading.take() {
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
                        heading: info.heading,
                        content,
                        task_type: info.task_type,
                        priority: info.priority,
                        created,
                        timestamp,
                        timestamp_type: ts_type,
                        timestamp_date: ts_date,
                        timestamp_time: ts_time,
                        timestamp_end_time: ts_end_time,
                    });
                } else if info.task_type.is_some() {
                    tasks.push(Task {
                        file: path.display().to_string(),
                        line: info.line,
                        heading: info.heading,
                        content: String::new(),
                        task_type: info.task_type,
                        priority: info.priority,
                        created: None,
                        timestamp: None,
                        timestamp_type: None,
                        timestamp_date: None,
                        timestamp_time: None,
                        timestamp_end_time: None,
                    });
                }
            }
        }
        _ => {}
    }
}

fn parse_heading(text: &str) -> (Option<TaskType>, Option<Priority>, String) {
    if let Some(caps) = HEADING_RE.captures(text) {
        let task_type = TaskType::from_str(&caps[1]);
        let priority = caps.get(2).map(|m| Priority::from_char(m.as_str().chars().next().unwrap()));
        let heading = caps[3].to_string();
        (task_type, priority, heading)
    } else {
        (None, None, text.to_string())
    }
}

fn extract_timestamps_from_node<'a>(node: &'a AstNode<'a>, mappings: &[(&str, &str)]) -> (Option<String>, Option<String>) {
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

fn extract_paragraph_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    for child in node.children() {
        if let NodeValue::Text(t) = &child.data.borrow().value {
            text.push_str(t);
        }
    }
    text.trim().to_string()
}

fn extract_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    for child in node.children() {
        if let NodeValue::Text(ref t) = child.data.borrow().value {
            text.push_str(t);
        }
    }
    text
}
