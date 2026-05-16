use std::fmt::Write;

use crate::types::{DayAgenda, Task, TaskWithOffset};

/// Render day agendas as Markdown
pub fn render_days_markdown(days: &[DayAgenda]) -> String {
    let mut output = String::from("# Agenda\n\n");

    for day in days {
        let _ = writeln!(output, "## {}\n", day.date);

        if !day.overdue.is_empty() {
            output.push_str("### Overdue\n\n");
            for task_with_offset in &day.overdue {
                render_task_with_offset_md(&mut output, task_with_offset);
            }
            output.push('\n');
        }

        if !day.scheduled_timed.is_empty() {
            output.push_str("### Scheduled\n\n");
            for task_with_offset in &day.scheduled_timed {
                render_task_with_offset_md(&mut output, task_with_offset);
            }
            output.push('\n');
        }

        if !day.scheduled_no_time.is_empty() {
            if day.scheduled_timed.is_empty() {
                output.push_str("### Scheduled\n\n");
            }
            for task_with_offset in &day.scheduled_no_time {
                render_task_with_offset_md(&mut output, task_with_offset);
            }
            output.push('\n');
        }

        if !day.upcoming.is_empty() {
            output.push_str("### Upcoming\n\n");
            for task_with_offset in &day.upcoming {
                render_task_with_offset_md(&mut output, task_with_offset);
            }
            output.push('\n');
        }
    }

    output
}

fn render_task_with_offset_md(output: &mut String, task_with_offset: &TaskWithOffset) {
    let task = &task_with_offset.task;

    let _ = write!(output, "#### {}", task.heading);
    if let Some(offset) = task_with_offset.days_offset {
        if offset > 0 {
            let _ = write!(output, " (in {offset} days)");
        } else {
            let _ = write!(output, " ({} days ago)", -offset);
        }
    }
    output.push('\n');

    let _ = writeln!(output, "**File:** {}:{}", task.file, task.line);
    if let Some(ref t) = task.task_type {
        let _ = writeln!(output, "**Type:** {t}");
    }
    if let Some(ref p) = task.priority {
        let _ = writeln!(output, "**Priority:** {p}");
    }
    if let Some(ref ts) = task.timestamp {
        let _ = writeln!(output, "**Time:** {ts}");
    }
    if !task.content.is_empty() {
        let _ = write!(output, "\n{}\n\n", task.content);
    } else {
        output.push('\n');
    }
}

/// Render day agendas as HTML
pub fn render_days_html(days: &[DayAgenda]) -> String {
    let mut output = String::from("<html><body><h1>Agenda</h1>\n");

    for day in days {
        let _ = writeln!(output, "<h2>{}</h2>", html_escape(&day.date));

        if !day.overdue.is_empty() {
            output.push_str("<h3>Overdue</h3>\n");
            for task_with_offset in &day.overdue {
                render_task_with_offset_html(&mut output, task_with_offset);
            }
        }

        if !day.scheduled_timed.is_empty() {
            output.push_str("<h3>Scheduled</h3>\n");
            for task_with_offset in &day.scheduled_timed {
                render_task_with_offset_html(&mut output, task_with_offset);
            }
        }

        if !day.scheduled_no_time.is_empty() {
            if day.scheduled_timed.is_empty() {
                output.push_str("<h3>Scheduled</h3>\n");
            }
            for task_with_offset in &day.scheduled_no_time {
                render_task_with_offset_html(&mut output, task_with_offset);
            }
        }

        if !day.upcoming.is_empty() {
            output.push_str("<h3>Upcoming</h3>\n");
            for task_with_offset in &day.upcoming {
                render_task_with_offset_html(&mut output, task_with_offset);
            }
        }
    }

    output.push_str("</body></html>");
    output
}

fn render_task_with_offset_html(output: &mut String, task_with_offset: &TaskWithOffset) {
    let task = &task_with_offset.task;

    let _ = write!(output, "<h4>{}", html_escape(&task.heading));
    if let Some(offset) = task_with_offset.days_offset {
        let label = if offset > 0 {
            format!(" (in {offset} days)")
        } else {
            format!(" ({} days ago)", -offset)
        };
        let _ = write!(output, "{}", html_escape(&label));
    }
    output.push_str("</h4>\n");

    let _ = writeln!(
        output,
        "<p><strong>File:</strong> {}:{}</p>",
        html_escape(&task.file),
        task.line
    );
    if let Some(ref t) = task.task_type {
        let _ = writeln!(output, "<p><strong>Type:</strong> {t}</p>");
    }
    if let Some(ref p) = task.priority {
        let _ = writeln!(output, "<p><strong>Priority:</strong> {p}</p>");
    }
    if let Some(ref ts) = task.timestamp {
        let _ = writeln!(output, "<p><strong>Time:</strong> {}</p>", html_escape(ts));
    }
    if !task.content.is_empty() {
        let _ = writeln!(output, "<p>{}</p>", html_escape(&task.content));
    }
}

/// Render tasks as Markdown
pub fn render_markdown(tasks: &[Task]) -> String {
    let mut output = String::from("# Tasks\n\n");
    for task in tasks {
        let _ = writeln!(output, "## {}", task.heading);
        let _ = writeln!(output, "**File:** {}:{}", task.file, task.line);
        if let Some(ref t) = task.task_type {
            let _ = writeln!(output, "**Type:** {t}");
        }
        if let Some(ref p) = task.priority {
            let _ = writeln!(output, "**Priority:** {p}");
        }
        if let Some(ref c) = task.created {
            let _ = writeln!(output, "**Created:** {c}");
        }
        if let Some(ref ts) = task.timestamp {
            let _ = writeln!(output, "**Time:** {ts}");
        }
        if let Some(ref total) = task.total_clock_time {
            let _ = writeln!(output, "**Total Time:** {total}");
        }
        if let Some(ref clocks) = task.clocks {
            output.push_str("\n**Clock:**\n");
            for clock in clocks {
                if let Some(ref end) = clock.end {
                    if let Some(ref dur) = clock.duration {
                        let _ = writeln!(output, "- {} → {} ({})", clock.start, end, dur);
                    } else {
                        let _ = writeln!(output, "- {} → {}", clock.start, end);
                    }
                } else {
                    let _ = writeln!(output, "- {} (active)", clock.start);
                }
            }
        }
        if !task.content.is_empty() {
            let _ = write!(output, "\n{}\n\n", task.content);
        } else {
            output.push('\n');
        }
    }
    output
}

/// Render tasks as HTML
pub fn render_html(tasks: &[Task]) -> String {
    let mut output = String::from("<html><body><h1>Tasks</h1>\n");
    for task in tasks {
        let _ = writeln!(output, "<h2>{}</h2>", html_escape(&task.heading));
        let _ = writeln!(
            output,
            "<p><strong>File:</strong> {}:{}</p>",
            html_escape(&task.file),
            task.line
        );
        if let Some(ref t) = task.task_type {
            let _ = writeln!(output, "<p><strong>Type:</strong> {t}</p>");
        }
        if let Some(ref p) = task.priority {
            let _ = writeln!(output, "<p><strong>Priority:</strong> {p}</p>");
        }
        if let Some(ref c) = task.created {
            let _ = writeln!(
                output,
                "<p><strong>Created:</strong> {}</p>",
                html_escape(c)
            );
        }
        if let Some(ref ts) = task.timestamp {
            let _ = writeln!(output, "<p><strong>Time:</strong> {}</p>", html_escape(ts));
        }
        if let Some(ref total) = task.total_clock_time {
            let _ = writeln!(
                output,
                "<p><strong>Total Time:</strong> {}</p>",
                html_escape(total)
            );
        }
        if let Some(ref clocks) = task.clocks {
            output.push_str("<p><strong>Clock:</strong></p>\n<ul>\n");
            for clock in clocks {
                if let Some(ref end) = clock.end {
                    if let Some(ref dur) = clock.duration {
                        let _ = writeln!(
                            output,
                            "<li>{} → {} ({})</li>",
                            html_escape(&clock.start),
                            html_escape(end),
                            html_escape(dur)
                        );
                    } else {
                        let _ = writeln!(
                            output,
                            "<li>{} → {}</li>",
                            html_escape(&clock.start),
                            html_escape(end)
                        );
                    }
                } else {
                    let _ = writeln!(output, "<li>{} (active)</li>", html_escape(&clock.start));
                }
            }
            output.push_str("</ul>\n");
        }
        if !task.content.is_empty() {
            let _ = writeln!(output, "<p>{}</p>", html_escape(&task.content));
        }
    }
    output.push_str("</body></html>");
    output
}

/// Escape HTML special characters in pre-existing text content
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Priority, TaskType};

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("A & B"), "A &amp; B");
    }

    #[test]
    fn test_render_markdown_basic() {
        let tasks = vec![Task {
            file: "test.md".to_string(),
            line: 1,
            heading: "Test Task".to_string(),
            content: "Description".to_string(),
            task_type: Some(TaskType::Todo),
            priority: Some(Priority::A),
            created: None,
            timestamp: None,
            timestamp_type: None,
            timestamp_date: None,
            timestamp_time: None,
            timestamp_end_time: None,
            clocks: None,
            total_clock_time: None,
        }];

        let output = render_markdown(&tasks);
        assert!(output.contains("# Tasks"));
        assert!(output.contains("## Test Task"));
        assert!(output.contains("**Type:** TODO"));
        assert!(output.contains("**Priority:** A"));
    }

    #[test]
    fn test_render_html_escapes() {
        let tasks = vec![Task {
            file: "<script>.md".to_string(),
            line: 1,
            heading: "Test & Task".to_string(),
            content: String::new(),
            task_type: None,
            priority: None,
            created: None,
            timestamp: None,
            timestamp_type: None,
            timestamp_date: None,
            timestamp_time: None,
            timestamp_end_time: None,
            clocks: None,
            total_clock_time: None,
        }];

        let output = render_html(&tasks);
        assert!(output.contains("&lt;script&gt;"));
        assert!(output.contains("Test &amp; Task"));
    }
}
