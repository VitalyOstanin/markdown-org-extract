use clap::Parser;
use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};
use glob::glob;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "markdown-extract")]
#[command(about = "Extract tasks from markdown files")]
struct Cli {
    #[arg(long, default_value = ".")]
    dir: PathBuf,

    #[arg(long, default_value = "*.md")]
    glob: String,

    #[arg(long, default_value = "json")]
    format: String,

    #[arg(long)]
    output: Option<PathBuf>,

    #[arg(long, default_value = "ru,en")]
    locale: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Task {
    file: String,
    line: u32,
    heading: String,
    content: String,
    task_type: Option<String>,
    timestamp: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mappings = get_weekday_mappings(&cli.locale);

    let pattern = format!("{}/**/{}", cli.dir.display(), cli.glob);
    let mut tasks = Vec::new();

    for entry in glob(&pattern)? {
        let path = entry?;
        if let Ok(content) = fs::read_to_string(&path) {
            if has_pattern(&content) {
                tasks.extend(extract_tasks(&path, &content, &mappings));
            }
        }
    }

    let output = match cli.format.as_str() {
        "json" => serde_json::to_string_pretty(&tasks)?,
        "md" => render_markdown(&tasks),
        "html" => render_html(&tasks),
        _ => return Err("Invalid format".into()),
    };

    if let Some(out_path) = cli.output {
        fs::write(out_path, output)?;
    } else {
        io::stdout().write_all(output.as_bytes())?;
    }

    Ok(())
}

fn get_weekday_mappings(locale: &str) -> Vec<(&'static str, &'static str)> {
    let locales: Vec<&str> = locale.split(',').map(|s| s.trim()).collect();
    let mut mappings = Vec::new();
    
    for loc in locales {
        match loc {
            "ru" => {
                // Сначала полные названия (длинные), потом сокращения
                mappings.extend_from_slice(&[
                    ("Понедельник", "Monday"), ("Вторник", "Tuesday"),
                    ("Среда", "Wednesday"), ("Четверг", "Thursday"),
                    ("Пятница", "Friday"), ("Суббота", "Saturday"),
                    ("Воскресенье", "Sunday"),
                    ("Пн", "Mon"), ("Вт", "Tue"), ("Ср", "Wed"), 
                    ("Чт", "Thu"), ("Пт", "Fri"), ("Сб", "Sat"), ("Вс", "Sun"),
                ]);
            }
            "en" => {
                // Английский уже в нужном формате
            }
            _ => {}
        }
    }
    mappings
}

fn normalize_weekdays(text: &str, mappings: &[(&str, &str)]) -> String {
    let mut result = text.to_string();
    for (from, to) in mappings {
        result = result.replace(from, to);
    }
    result
}

fn has_pattern(content: &str) -> bool {
    let re = Regex::new(r"(?m)^[#*]+\s+(TODO|DONE)\s").unwrap();
    if re.is_match(content) {
        return true;
    }

    let time_re = Regex::new(r"`(?:SCHEDULED|DEADLINE|CLOSED)?:?\s*<\d{4}-\d{2}-\d{2}").unwrap();
    time_re.is_match(content)
}

fn extract_tasks(path: &PathBuf, content: &str, mappings: &[(&str, &str)]) -> Vec<Task> {
    let arena = Arena::new();
    let root = parse_document(&arena, content, &Options::default());

    let mut tasks = Vec::new();
    let mut current_heading: Option<(String, Option<String>, u32)> = None;
    
    for node in root.children() {
        process_top_level_node(node, path, &mut tasks, &mut current_heading, mappings);
    }
    
    tasks
}

fn process_top_level_node<'a>(
    node: &'a AstNode<'a>,
    path: &PathBuf,
    tasks: &mut Vec<Task>,
    current_heading: &mut Option<(String, Option<String>, u32)>,
    mappings: &[(&str, &str)],
) {
    match &node.data.borrow().value {
        NodeValue::Heading(_) => {
            let text = extract_text(node);
            let (task_type, heading) = parse_heading(&text);
            let line = node.data.borrow().sourcepos.start.line as u32;
            *current_heading = Some((heading, task_type, line));
        }
        NodeValue::Paragraph => {
            if let Some((heading, task_type, line)) = current_heading {
                if let Some(timestamp) = extract_timestamp_from_node(node, mappings) {
                    let content = extract_paragraph_text(node);
                    tasks.push(Task {
                        file: path.display().to_string(),
                        line: *line,
                        heading: heading.clone(),
                        content,
                        task_type: task_type.clone(),
                        timestamp: Some(timestamp),
                    });
                    *current_heading = None;
                }
            }
        }
        _ => {}
    }
    
    // Also check if heading itself should be added (TODO/DONE without timestamp)
    if let NodeValue::Heading(_) = &node.data.borrow().value {
        if let Some((heading, Some(task_type), line)) = current_heading {
            // Check next sibling for timestamp
            let mut has_timestamp = false;
            if let Some(next) = node.next_sibling() {
                if let NodeValue::Paragraph = &next.data.borrow().value {
                    if extract_timestamp_from_node(next, mappings).is_some() {
                        has_timestamp = true;
                    }
                }
            }
            
            if !has_timestamp {
                tasks.push(Task {
                    file: path.display().to_string(),
                    line: *line,
                    heading: heading.clone(),
                    content: String::new(),
                    task_type: Some(task_type.clone()),
                    timestamp: None,
                });
                *current_heading = None;
            }
        }
    }
}

fn parse_heading(text: &str) -> (Option<String>, String) {
    let re = Regex::new(r"^(TODO|DONE)\s+(.+)$").unwrap();
    if let Some(caps) = re.captures(text) {
        (Some(caps[1].to_string()), caps[2].to_string())
    } else {
        (None, text.to_string())
    }
}

fn extract_timestamp_from_node<'a>(node: &'a AstNode<'a>, mappings: &[(&str, &str)]) -> Option<String> {
    match &node.data.borrow().value {
        NodeValue::Paragraph => {
            for child in node.children() {
                if let NodeValue::Code(code) = &child.data.borrow().value {
                    if let Some(ts) = extract_timestamp(&code.literal, mappings) {
                        return Some(ts);
                    }
                }
            }
            None
        }
        _ => None
    }
}

fn extract_paragraph_text<'a>(node: &'a AstNode<'a>) -> String {
    let mut text = String::new();
    for child in node.children() {
        match &child.data.borrow().value {
            NodeValue::Text(t) => text.push_str(t),
            NodeValue::Code(_) => {}, // Skip inline code
            _ => {}
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

fn extract_timestamp(text: &str, mappings: &[(&str, &str)]) -> Option<String> {
    // Normalize weekdays first
    let normalized = normalize_weekdays(text, mappings);
    let clean_text = normalized.trim().trim_matches('`').trim();
    
    // Check for planning keywords with timestamps
    let re = Regex::new(
        r"^\s*((?:SCHEDULED|DEADLINE|CLOSED):\s*)<(\d{4}-\d{2}-\d{2}[^>]*)>"
    ).unwrap();
    
    if let Some(caps) = re.captures(clean_text) {
        let prefix = &caps[1];
        let date = &caps[2];
        return Some(format!("{}<{}>", prefix, date));
    }

    // Check for date range
    let range_re = Regex::new(
        r"^\s*<(\d{4}-\d{2}-\d{2}[^>]*)>--<(\d{4}-\d{2}-\d{2}[^>]*)>"
    ).unwrap();
    
    if let Some(caps) = range_re.captures(clean_text) {
        return Some(format!("<{}>--<{}>", &caps[1], &caps[2]));
    }

    // Check for simple timestamp
    let simple_re = Regex::new(
        r"^\s*<(\d{4}-\d{2}-\d{2}[^>]*)>"
    ).unwrap();
    
    if let Some(caps) = simple_re.captures(clean_text) {
        return Some(format!("<{}>", &caps[1]));
    }

    None
}

fn render_markdown(tasks: &[Task]) -> String {
    let mut output = String::from("# Tasks\n\n");
    for task in tasks {
        output.push_str(&format!("## {}\n", task.heading));
        output.push_str(&format!("**File:** {}:{}\n", task.file, task.line));
        if let Some(ref t) = task.task_type {
            output.push_str(&format!("**Type:** {}\n", t));
        }
        if let Some(ref ts) = task.timestamp {
            output.push_str(&format!("**Time:** {}\n", ts));
        }
        output.push_str(&format!("\n{}\n\n", task.content));
    }
    output
}

fn render_html(tasks: &[Task]) -> String {
    let mut output = String::from("<html><body><h1>Tasks</h1>\n");
    for task in tasks {
        output.push_str(&format!("<h2>{}</h2>\n", task.heading));
        output.push_str(&format!("<p><strong>File:</strong> {}:{}</p>\n", task.file, task.line));
        if let Some(ref t) = task.task_type {
            output.push_str(&format!("<p><strong>Type:</strong> {}</p>\n", t));
        }
        if let Some(ref ts) = task.timestamp {
            output.push_str(&format!("<p><strong>Time:</strong> {}</p>\n", ts));
        }
        output.push_str(&format!("<p>{}</p>\n", task.content));
    }
    output.push_str("</body></html>");
    output
}
