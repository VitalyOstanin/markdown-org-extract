use chrono::{Datelike, NaiveDate, TimeZone};
use chrono_tz::Tz;
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

    #[arg(long, default_value = "day")]
    agenda: String,

    #[arg(long)]
    date: Option<String>,

    #[arg(long)]
    from: Option<String>,

    #[arg(long)]
    to: Option<String>,

    #[arg(long, default_value = "Europe/Moscow")]
    tz: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Task {
    file: String,
    line: u32,
    heading: String,
    content: String,
    task_type: Option<String>,
    priority: Option<String>,
    created: Option<String>,
    timestamp: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedTimestamp {
    timestamp_type: TimestampType,
    date: NaiveDate,
    #[allow(dead_code)]
    time: Option<String>,
    end_date: Option<NaiveDate>,
    #[allow(dead_code)]
    end_time: Option<String>,
    repeater: Option<Repeater>,
    warning: Option<Warning>,
}

#[derive(Debug, Clone, PartialEq)]
enum TimestampType {
    Scheduled,
    Deadline,
    Closed,
    Plain,
}

#[derive(Debug, Clone)]
struct Repeater {
    interval: i64,
    unit: RepeatUnit,
}

#[derive(Debug, Clone)]
enum RepeatUnit {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone)]
struct Warning {
    days: i64,
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

    // Apply agenda filtering
    tasks = filter_agenda(tasks, &cli)?;

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

fn filter_agenda(mut tasks: Vec<Task>, cli: &Cli) -> Result<Vec<Task>, Box<dyn std::error::Error>> {
    let tz: Tz = cli.tz.parse()?;
    
    match cli.agenda.as_str() {
        "day" => {
            let target_date = if let Some(ref date_str) = cli.date {
                NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?
            } else {
                tz.from_utc_datetime(&chrono::Utc::now().naive_utc()).date_naive()
            };
            tasks.retain(|t| task_matches_date(t, &target_date));
        }
        "week" => {
            let (start_date, end_date) = if let (Some(from), Some(to)) = (&cli.from, &cli.to) {
                (
                    NaiveDate::parse_from_str(from, "%Y-%m-%d")?,
                    NaiveDate::parse_from_str(to, "%Y-%m-%d")?,
                )
            } else {
                get_current_week(&tz)
            };
            tasks.retain(|t| task_in_range(t, &start_date, &end_date));
        }
        "tasks" => {
            tasks.retain(|t| t.task_type.as_deref() == Some("TODO"));
            tasks.sort_by(|a, b| {
                let priority_order = |p: &Option<String>| match p.as_deref() {
                    Some("A") => 0,
                    Some("B") => 1,
                    Some("C") => 2,
                    Some(x) if x.len() == 1 => (x.chars().next().unwrap() as u32) - ('A' as u32),
                    _ => 999,
                };
                priority_order(&a.priority).cmp(&priority_order(&b.priority))
            });
        }
        _ => return Err("Invalid agenda mode. Use: day, week, tasks".into()),
    }
    Ok(tasks)
}

fn get_current_week(tz: &Tz) -> (NaiveDate, NaiveDate) {
    let today = tz.from_utc_datetime(&chrono::Utc::now().naive_utc()).date_naive();
    let weekday = today.weekday();
    let days_from_monday = weekday.num_days_from_monday();
    let monday = today - chrono::Duration::days(days_from_monday as i64);
    let sunday = monday + chrono::Duration::days(6);
    (monday, sunday)
}

fn task_matches_date(task: &Task, target_date: &NaiveDate) -> bool {
    if let Some(ref ts) = task.timestamp {
        if let Some(parsed) = parse_org_timestamp(ts) {
            return timestamp_matches_date(&parsed, target_date);
        }
    }
    false
}

fn task_in_range(task: &Task, start: &NaiveDate, end: &NaiveDate) -> bool {
    if let Some(ref ts) = task.timestamp {
        if let Some(parsed) = parse_org_timestamp(ts) {
            return timestamp_in_range(&parsed, start, end);
        }
    }
    false
}

fn timestamp_matches_date(parsed: &ParsedTimestamp, target_date: &NaiveDate) -> bool {
    match parsed.timestamp_type {
        TimestampType::Deadline => {
            // DEADLINE shows warning days before
            let warning_days = parsed.warning.as_ref().map(|w| w.days).unwrap_or(14); // default 14 days
            let warning_start = parsed.date - chrono::Duration::days(warning_days);
            *target_date >= warning_start && *target_date <= parsed.date
        }
        TimestampType::Scheduled => {
            // SCHEDULED shows from date onwards (with repeater support)
            if let Some(ref repeater) = parsed.repeater {
                check_repeater_match(&parsed.date, repeater, target_date)
            } else {
                *target_date >= parsed.date
            }
        }
        TimestampType::Closed => {
            // CLOSED shows only on exact date
            parsed.date == *target_date
        }
        TimestampType::Plain => {
            // Plain timestamp or date range
            if let Some(end_date) = parsed.end_date {
                *target_date >= parsed.date && *target_date <= end_date
            } else {
                parsed.date == *target_date
            }
        }
    }
}

fn timestamp_in_range(parsed: &ParsedTimestamp, start: &NaiveDate, end: &NaiveDate) -> bool {
    match parsed.timestamp_type {
        TimestampType::Deadline => {
            let warning_days = parsed.warning.as_ref().map(|w| w.days).unwrap_or(14);
            let warning_start = parsed.date - chrono::Duration::days(warning_days);
            !(parsed.date < *start || warning_start > *end)
        }
        TimestampType::Scheduled => {
            if let Some(ref repeater) = parsed.repeater {
                check_repeater_in_range(&parsed.date, repeater, start, end)
            } else {
                parsed.date >= *start && parsed.date <= *end
            }
        }
        TimestampType::Closed => {
            parsed.date >= *start && parsed.date <= *end
        }
        TimestampType::Plain => {
            if let Some(end_date) = parsed.end_date {
                !(end_date < *start || parsed.date > *end)
            } else {
                parsed.date >= *start && parsed.date <= *end
            }
        }
    }
}

fn check_repeater_match(base_date: &NaiveDate, repeater: &Repeater, target_date: &NaiveDate) -> bool {
    if *target_date < *base_date {
        return false;
    }
    
    let days_diff = (*target_date - *base_date).num_days();
    let interval_days = match repeater.unit {
        RepeatUnit::Day => repeater.interval,
        RepeatUnit::Week => repeater.interval * 7,
        RepeatUnit::Month => repeater.interval * 30, // approximate
        RepeatUnit::Year => repeater.interval * 365, // approximate
    };
    
    days_diff % interval_days == 0
}

fn check_repeater_in_range(base_date: &NaiveDate, repeater: &Repeater, start: &NaiveDate, end: &NaiveDate) -> bool {
    if *base_date > *end {
        return false;
    }
    
    let mut current = *base_date;
    while current <= *end {
        if current >= *start && current <= *end {
            return true;
        }
        current = match repeater.unit {
            RepeatUnit::Day => current + chrono::Duration::days(repeater.interval),
            RepeatUnit::Week => current + chrono::Duration::weeks(repeater.interval),
            RepeatUnit::Month => {
                let month = current.month() as i64 + repeater.interval;
                let year = current.year() as i64 + (month - 1) / 12;
                let month = ((month - 1) % 12 + 1) as u32;
                NaiveDate::from_ymd_opt(year as i32, month, current.day()).unwrap_or(current)
            }
            RepeatUnit::Year => {
                NaiveDate::from_ymd_opt(
                    current.year() + repeater.interval as i32,
                    current.month(),
                    current.day()
                ).unwrap_or(current)
            }
        };
    }
    false
}

#[allow(dead_code)]
fn extract_date_from_timestamp(ts: &str) -> Option<NaiveDate> {
    let re = Regex::new(r"(\d{4}-\d{2}-\d{2})").unwrap();
    re.captures(ts)
        .and_then(|caps| NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok())
}

fn parse_org_timestamp(ts: &str) -> Option<ParsedTimestamp> {
    // Determine timestamp type
    let timestamp_type = if ts.contains("SCHEDULED:") {
        TimestampType::Scheduled
    } else if ts.contains("DEADLINE:") {
        TimestampType::Deadline
    } else if ts.contains("CLOSED:") {
        TimestampType::Closed
    } else {
        TimestampType::Plain
    };

    // Extract date range: <date>--<date>
    let range_re = Regex::new(r"<(\d{4}-\d{2}-\d{2})(?: [A-Za-z]+)?(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?(?:\s+([+.]+\d+[dwmy]))?(?:\s+-(\d+)d)?>--<(\d{4}-\d{2}-\d{2})(?: [A-Za-z]+)?(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?>").unwrap();
    
    if let Some(caps) = range_re.captures(ts) {
        let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
        let time = caps.get(2).map(|m| m.as_str().to_string());
        let end_date = NaiveDate::parse_from_str(&caps[6], "%Y-%m-%d").ok();
        let end_time = caps.get(7).map(|m| m.as_str().to_string());
        let repeater = caps.get(4).and_then(|m| parse_repeater(m.as_str()));
        let warning = caps.get(5).map(|m| Warning { days: m.as_str().parse().unwrap_or(0) });
        
        return Some(ParsedTimestamp {
            timestamp_type,
            date,
            time,
            end_date,
            end_time,
            repeater,
            warning,
        });
    }

    // Extract single timestamp: <date time repeater warning>
    let single_re = Regex::new(r"<(\d{4}-\d{2}-\d{2})(?: [A-Za-z]+)?(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?(?:\s+([+.]+\d+[dwmy]))?(?:\s+-(\d+)d)?>").unwrap();
    
    if let Some(caps) = single_re.captures(ts) {
        let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
        let time = caps.get(2).map(|m| m.as_str().to_string());
        let end_time = caps.get(3).map(|m| m.as_str().to_string());
        let repeater = caps.get(4).and_then(|m| parse_repeater(m.as_str()));
        let warning = caps.get(5).map(|m| Warning { days: m.as_str().parse().unwrap_or(0) });
        
        return Some(ParsedTimestamp {
            timestamp_type,
            date,
            time,
            end_date: None,
            end_time,
            repeater,
            warning,
        });
    }

    None
}

fn parse_repeater(s: &str) -> Option<Repeater> {
    let re = Regex::new(r"[+.](\d+)([dwmy])").unwrap();
    let caps = re.captures(s)?;
    let interval: i64 = caps[1].parse().ok()?;
    let unit = match &caps[2] {
        "d" => RepeatUnit::Day,
        "w" => RepeatUnit::Week,
        "m" => RepeatUnit::Month,
        "y" => RepeatUnit::Year,
        _ => return None,
    };
    Some(Repeater { interval, unit })
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
    let mut current_heading: Option<(String, Option<String>, Option<String>, u32)> = None;
    
    for node in root.children() {
        process_top_level_node(node, path, &mut tasks, &mut current_heading, mappings);
    }
    
    tasks
}

fn process_top_level_node<'a>(
    node: &'a AstNode<'a>,
    path: &PathBuf,
    tasks: &mut Vec<Task>,
    current_heading: &mut Option<(String, Option<String>, Option<String>, u32)>,
    mappings: &[(&str, &str)],
) {
    match &node.data.borrow().value {
        NodeValue::Heading(_) => {
            let text = extract_text(node);
            let (task_type, priority, heading) = parse_heading(&text);
            let line = node.data.borrow().sourcepos.start.line as u32;
            *current_heading = Some((heading, task_type, priority, line));
        }
        NodeValue::Paragraph => {
            if let Some((heading, task_type, priority, line)) = current_heading {
                let (created, timestamp) = extract_timestamps_from_node(node, mappings);
                if created.is_some() || timestamp.is_some() {
                    let content = extract_paragraph_text(node);
                    tasks.push(Task {
                        file: path.display().to_string(),
                        line: *line,
                        heading: heading.clone(),
                        content,
                        task_type: task_type.clone(),
                        priority: priority.clone(),
                        created,
                        timestamp,
                    });
                    *current_heading = None;
                }
            }
        }
        _ => {}
    }
    
    // Also check if heading itself should be added (TODO/DONE without timestamp)
    if let NodeValue::Heading(_) = &node.data.borrow().value {
        if let Some((heading, Some(task_type), priority, line)) = current_heading {
            // Check next sibling for timestamp
            let mut has_timestamp = false;
            if let Some(next) = node.next_sibling() {
                if let NodeValue::Paragraph = &next.data.borrow().value {
                    let (created, timestamp) = extract_timestamps_from_node(next, mappings);
                    if created.is_some() || timestamp.is_some() {
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
                    priority: priority.clone(),
                    created: None,
                    timestamp: None,
                });
                *current_heading = None;
            }
        }
    }
}

fn parse_heading(text: &str) -> (Option<String>, Option<String>, String) {
    let re = Regex::new(r"^(TODO|DONE)\s+(?:\[#([A-Z])\]\s+)?(.+)$").unwrap();
    if let Some(caps) = re.captures(text) {
        let task_type = Some(caps[1].to_string());
        let priority = caps.get(2).map(|m| m.as_str().to_string());
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

fn extract_created(text: &str, mappings: &[(&str, &str)]) -> Option<String> {
    let normalized = normalize_weekdays(text, mappings);
    let clean_text = normalized.trim().trim_matches('`').trim();
    
    let re = Regex::new(r"^\s*CREATED:\s*<(\d{4}-\d{2}-\d{2}[^>]*)>").unwrap();
    if let Some(caps) = re.captures(clean_text) {
        return Some(format!("CREATED: <{}>", &caps[1]));
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
        if let Some(ref p) = task.priority {
            output.push_str(&format!("**Priority:** [#{}]\n", p));
        }
        if let Some(ref c) = task.created {
            output.push_str(&format!("**Created:** {}\n", c));
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
        if let Some(ref p) = task.priority {
            output.push_str(&format!("<p><strong>Priority:</strong> [#{}]</p>\n", p));
        }
        if let Some(ref c) = task.created {
            output.push_str(&format!("<p><strong>Created:</strong> {}</p>\n", c));
        }
        if let Some(ref ts) = task.timestamp {
            output.push_str(&format!("<p><strong>Time:</strong> {}</p>\n", ts));
        }
        output.push_str(&format!("<p>{}</p>\n", task.content));
    }
    output.push_str("</body></html>");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heading_with_priority() {
        let (task_type, priority, heading) = parse_heading("TODO [#A] High priority task");
        assert_eq!(task_type, Some("TODO".to_string()));
        assert_eq!(priority, Some("A".to_string()));
        assert_eq!(heading, "High priority task");
    }

    #[test]
    fn test_parse_heading_without_priority() {
        let (task_type, priority, heading) = parse_heading("TODO Regular task");
        assert_eq!(task_type, Some("TODO".to_string()));
        assert_eq!(priority, None);
        assert_eq!(heading, "Regular task");
    }

    #[test]
    fn test_parse_heading_done_with_priority() {
        let (task_type, priority, heading) = parse_heading("DONE [#B] Completed task");
        assert_eq!(task_type, Some("DONE".to_string()));
        assert_eq!(priority, Some("B".to_string()));
        assert_eq!(heading, "Completed task");
    }

    #[test]
    fn test_parse_heading_no_task_type() {
        let (task_type, priority, heading) = parse_heading("Just a heading");
        assert_eq!(task_type, None);
        assert_eq!(priority, None);
        assert_eq!(heading, "Just a heading");
    }

    #[test]
    fn test_parse_heading_various_priorities() {
        for letter in 'A'..='Z' {
            let input = format!("TODO [#{}] Task", letter);
            let (task_type, priority, _) = parse_heading(&input);
            assert_eq!(task_type, Some("TODO".to_string()));
            assert_eq!(priority, Some(letter.to_string()));
        }
    }

    #[test]
    fn test_extract_created() {
        let mappings = vec![];
        let result = extract_created("`CREATED: <2024-12-01 Mon>`", &mappings);
        assert_eq!(result, Some("CREATED: <2024-12-01 Mon>".to_string()));
    }

    #[test]
    fn test_extract_created_with_time() {
        let mappings = vec![];
        let result = extract_created("`CREATED: <2024-12-01 Mon 10:30>`", &mappings);
        assert_eq!(result, Some("CREATED: <2024-12-01 Mon 10:30>".to_string()));
    }

    #[test]
    fn test_extract_created_russian_weekday() {
        let mappings = vec![("Пн", "Mon")];
        let result = extract_created("`CREATED: <2024-12-01 Пн>`", &mappings);
        assert_eq!(result, Some("CREATED: <2024-12-01 Mon>".to_string()));
    }

    #[test]
    fn test_extract_created_not_created() {
        let mappings = vec![];
        let result = extract_created("`DEADLINE: <2024-12-01 Mon>`", &mappings);
        assert_eq!(result, None);
    }

    // Org-mode timestamp parsing tests
    #[test]
    fn test_parse_deadline_with_warning() {
        let ts = "DEADLINE: <2024-12-15 Sun -7d>";
        let parsed = parse_org_timestamp(ts).unwrap();
        assert_eq!(parsed.timestamp_type, TimestampType::Deadline);
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 15).unwrap());
        assert_eq!(parsed.warning.unwrap().days, 7);
    }

    #[test]
    fn test_parse_scheduled_with_repeater() {
        let ts = "SCHEDULED: <2024-12-01 Mon +1w>";
        let parsed = parse_org_timestamp(ts).unwrap();
        assert_eq!(parsed.timestamp_type, TimestampType::Scheduled);
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 1).unwrap());
        assert!(parsed.repeater.is_some());
        let repeater = parsed.repeater.unwrap();
        assert_eq!(repeater.interval, 1);
    }

    #[test]
    fn test_parse_date_range() {
        let ts = "<2024-12-20 Fri>--<2024-12-22 Sun>";
        let parsed = parse_org_timestamp(ts).unwrap();
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 20).unwrap());
        assert_eq!(parsed.end_date, Some(NaiveDate::from_ymd_opt(2024, 12, 22).unwrap()));
    }

    #[test]
    fn test_parse_timestamp_with_time() {
        let ts = "<2024-12-05 Wed 10:00-12:00>";
        let parsed = parse_org_timestamp(ts).unwrap();
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 5).unwrap());
        assert_eq!(parsed.time, Some("10:00".to_string()));
        assert_eq!(parsed.end_time, Some("12:00".to_string()));
    }

    // Deadline warning logic tests
    #[test]
    fn test_deadline_shows_before_date() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Deadline,
            date: NaiveDate::from_ymd_opt(2024, 12, 15).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: None,
            warning: Some(Warning { days: 7 }),
        };
        
        // Should show 7 days before
        let target = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap();
        assert!(timestamp_matches_date(&parsed, &target));
        
        // Should show on deadline day
        let target = NaiveDate::from_ymd_opt(2024, 12, 15).unwrap();
        assert!(timestamp_matches_date(&parsed, &target));
        
        // Should NOT show 8 days before
        let target = NaiveDate::from_ymd_opt(2024, 12, 7).unwrap();
        assert!(!timestamp_matches_date(&parsed, &target));
        
        // Should NOT show after deadline
        let target = NaiveDate::from_ymd_opt(2024, 12, 16).unwrap();
        assert!(!timestamp_matches_date(&parsed, &target));
    }

    #[test]
    fn test_deadline_default_warning() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Deadline,
            date: NaiveDate::from_ymd_opt(2024, 12, 15).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: None,
            warning: None, // No warning specified, should use default 14 days
        };
        
        // Should show 14 days before (default)
        let target = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        assert!(timestamp_matches_date(&parsed, &target));
    }

    // Scheduled logic tests
    #[test]
    fn test_scheduled_shows_from_date() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Scheduled,
            date: NaiveDate::from_ymd_opt(2024, 12, 10).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: None,
            warning: None,
        };
        
        // Should NOT show before scheduled date
        let target = NaiveDate::from_ymd_opt(2024, 12, 9).unwrap();
        assert!(!timestamp_matches_date(&parsed, &target));
        
        // Should show on scheduled date
        let target = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap();
        assert!(timestamp_matches_date(&parsed, &target));
        
        // Should show after scheduled date
        let target = NaiveDate::from_ymd_opt(2024, 12, 15).unwrap();
        assert!(timestamp_matches_date(&parsed, &target));
    }

    // Repeater tests
    #[test]
    fn test_repeater_daily() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Scheduled,
            date: NaiveDate::from_ymd_opt(2024, 12, 1).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: Some(Repeater { interval: 2, unit: RepeatUnit::Day }),
            warning: None,
        };
        
        // Should match on base date
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 1).unwrap()));
        
        // Should match every 2 days
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 3).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 5).unwrap()));
        
        // Should NOT match on off days
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 2).unwrap()));
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 4).unwrap()));
    }

    #[test]
    fn test_repeater_weekly() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Scheduled,
            date: NaiveDate::from_ymd_opt(2024, 12, 1).unwrap(), // Sunday
            time: None,
            end_date: None,
            end_time: None,
            repeater: Some(Repeater { interval: 1, unit: RepeatUnit::Week }),
            warning: None,
        };
        
        // Should match every Sunday
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 1).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 8).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 15).unwrap()));
        
        // Should NOT match other days
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 2).unwrap()));
    }

    // Date range tests
    #[test]
    fn test_date_range_coverage() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Plain,
            date: NaiveDate::from_ymd_opt(2024, 12, 20).unwrap(),
            time: None,
            end_date: Some(NaiveDate::from_ymd_opt(2024, 12, 22).unwrap()),
            end_time: None,
            repeater: None,
            warning: None,
        };
        
        // Should NOT match before range
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 19).unwrap()));
        
        // Should match all days in range
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 20).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 21).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 22).unwrap()));
        
        // Should NOT match after range
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 23).unwrap()));
    }

    #[test]
    fn test_closed_exact_date_only() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Closed,
            date: NaiveDate::from_ymd_opt(2024, 12, 10).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: None,
            warning: None,
        };
        
        // Should only match exact date
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 10).unwrap()));
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 9).unwrap()));
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 11).unwrap()));
    }
}
