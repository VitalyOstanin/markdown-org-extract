use crate::types::{DayAgenda, Task, TaskWithOffset};

/// Render day agendas as Markdown
pub fn render_days_markdown(days: &[DayAgenda]) -> String {
    let mut output = String::from("# Agenda\n\n");
    
    for day in days {
        output.push_str(&format!("## {}\n\n", day.date));
        
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
    
    output.push_str(&format!("#### {}", task.heading));
    if let Some(offset) = task_with_offset.days_offset {
        let label = if offset > 0 {
            format!(" (in {offset} days)")
        } else {
            format!(" ({} days ago)", -offset)
        };
        output.push_str(&label);
    }
    output.push('\n');
    
    output.push_str(&format!("**File:** {}:{}\n", task.file, task.line));
    if let Some(ref t) = task.task_type {
        output.push_str(&format!("**Type:** {t:?}\n"));
    }
    if let Some(ref p) = task.priority {
        output.push_str(&format!("**Priority:** {p:?}\n"));
    }
    if let Some(ref ts) = task.timestamp {
        output.push_str(&format!("**Time:** {ts}\n"));
    }
    if !task.content.is_empty() {
        output.push_str(&format!("\n{}\n\n", task.content));
    } else {
        output.push('\n');
    }
}

/// Render day agendas as HTML
pub fn render_days_html(days: &[DayAgenda]) -> String {
    let mut output = String::from("<html><body><h1>Agenda</h1>\n");
    
    for day in days {
        output.push_str(&format!("<h2>{}</h2>\n", html_escape(&day.date)));
        
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
    
    output.push_str(&format!("<h4>{}", html_escape(&task.heading)));
    if let Some(offset) = task_with_offset.days_offset {
        let label = if offset > 0 {
            format!(" (in {offset} days)")
        } else {
            format!(" ({} days ago)", -offset)
        };
        output.push_str(&html_escape(&label));
    }
    output.push_str("</h4>\n");
    
    output.push_str(&format!(
        "<p><strong>File:</strong> {}:{}</p>\n",
        html_escape(&task.file),
        task.line
    ));
    if let Some(ref t) = task.task_type {
        output.push_str(&format!("<p><strong>Type:</strong> {t:?}</p>\n"));
    }
    if let Some(ref p) = task.priority {
        output.push_str(&format!("<p><strong>Priority:</strong> {p:?}</p>\n"));
    }
    if let Some(ref ts) = task.timestamp {
        output.push_str(&format!("<p><strong>Time:</strong> {}</p>\n", html_escape(ts)));
    }
    if !task.content.is_empty() {
        output.push_str(&format!("<p>{}</p>\n", html_escape(&task.content)));
    }
}

/// Render tasks as Markdown
pub fn render_markdown(tasks: &[Task]) -> String {
    let mut output = String::from("# Tasks\n\n");
    for task in tasks {
        output.push_str(&format!("## {}\n", task.heading));
        output.push_str(&format!("**File:** {}:{}\n", task.file, task.line));
        if let Some(ref t) = task.task_type {
            output.push_str(&format!("**Type:** {t:?}\n"));
        }
        if let Some(ref p) = task.priority {
            output.push_str(&format!("**Priority:** {p:?}\n"));
        }
        if let Some(ref c) = task.created {
            output.push_str(&format!("**Created:** {c}\n"));
        }
        if let Some(ref ts) = task.timestamp {
            output.push_str(&format!("**Time:** {ts}\n"));
        }
        if let Some(ref total) = task.total_clock_time {
            output.push_str(&format!("**Total Time:** {total}\n"));
        }
        if let Some(ref clocks) = task.clocks {
            output.push_str("\n**Clock:**\n");
            for clock in clocks {
                if let Some(ref end) = clock.end {
                    if let Some(ref dur) = clock.duration {
                        output.push_str(&format!("- {} → {} ({})\n", clock.start, end, dur));
                    } else {
                        output.push_str(&format!("- {} → {}\n", clock.start, end));
                    }
                } else {
                    output.push_str(&format!("- {} (active)\n", clock.start));
                }
            }
        }
        if !task.content.is_empty() {
            output.push_str(&format!("\n{}\n\n", task.content));
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
        output.push_str(&format!("<h2>{}</h2>\n", html_escape(&task.heading)));
        output.push_str(&format!(
            "<p><strong>File:</strong> {}:{}</p>\n",
            html_escape(&task.file),
            task.line
        ));
        if let Some(ref t) = task.task_type {
            output.push_str(&format!("<p><strong>Type:</strong> {t:?}</p>\n"));
        }
        if let Some(ref p) = task.priority {
            output.push_str(&format!("<p><strong>Priority:</strong> {p:?}</p>\n"));
        }
        if let Some(ref c) = task.created {
            output.push_str(&format!("<p><strong>Created:</strong> {}</p>\n", html_escape(c)));
        }
        if let Some(ref ts) = task.timestamp {
            output.push_str(&format!("<p><strong>Time:</strong> {}</p>\n", html_escape(ts)));
        }
        if let Some(ref total) = task.total_clock_time {
            output.push_str(&format!("<p><strong>Total Time:</strong> {}</p>\n", html_escape(total)));
        }
        if let Some(ref clocks) = task.clocks {
            output.push_str("<p><strong>Clock:</strong></p>\n<ul>\n");
            for clock in clocks {
                if let Some(ref end) = clock.end {
                    if let Some(ref dur) = clock.duration {
                        output.push_str(&format!(
                            "<li>{} → {} ({})</li>\n",
                            html_escape(&clock.start),
                            html_escape(end),
                            html_escape(dur)
                        ));
                    } else {
                        output.push_str(&format!(
                            "<li>{} → {}</li>\n",
                            html_escape(&clock.start),
                            html_escape(end)
                        ));
                    }
                } else {
                    output.push_str(&format!("<li>{} (active)</li>\n", html_escape(&clock.start)));
                }
            }
            output.push_str("</ul>\n");
        }
        if !task.content.is_empty() {
            output.push_str(&format!("<p>{}</p>\n", html_escape(&task.content)));
        }
    }
    output.push_str("</body></html>");
    output
}

/// Escape HTML special characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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
        assert!(output.contains("**Type:** Todo"));
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
