use std::fmt::Write;

use crate::types::{ClockEntry, DayAgenda, Task, TaskWithOffset};

/// Escape markdown special characters in plain text. Used for headings and
/// labels that originate from user input — keeps formatting from being broken
/// or hijacked (e.g. a heading containing `*` would otherwise render as italic).
fn md_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' | '`' | '*' | '_' | '#' | '[' | ']' | '<' | '>' | '|' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Escape HTML special characters in pre-existing text content.
/// Also drops C0 control characters (except `\t \n \r`) to protect downstream
/// renderers from null bytes and other invisible glyphs sneaked through markdown.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\t' | '\n' | '\r' => out.push(ch),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {}
            _ => out.push(ch),
        }
    }
    out
}

fn offset_suffix(days_offset: Option<i64>) -> Option<String> {
    days_offset.map(|offset| {
        if offset > 0 {
            format!(" (in {offset} days)")
        } else {
            format!(" ({} days ago)", -offset)
        }
    })
}

/// Common formatting strategy for one output format (Markdown or HTML).
///
/// All `render_*` entry points delegate field traversal to `write_task`, which
/// drives this trait's methods. Adding a new `Task` field means touching
/// `write_task` once instead of four renderers.
trait TaskFormat {
    fn doc_open(&self, title: &str) -> String;
    fn doc_close(&self, out: &mut String);
    fn day_header(&self, out: &mut String, date: &str);
    fn section(&self, out: &mut String, title: &str);
    fn after_section(&self, out: &mut String);
    fn task_heading(&self, out: &mut String, level: u8, heading: &str, days_offset: Option<i64>);
    /// Single `Label: value` field. `code` requests inline-code wrapping
    /// for formats that support it (Markdown); HTML ignores the hint.
    fn field(&self, out: &mut String, label: &str, value: &str, code: bool);
    fn clocks_open(&self, out: &mut String);
    fn clock_complete(&self, out: &mut String, start: &str, end: &str, duration: Option<&str>);
    fn clock_active(&self, out: &mut String, start: &str);
    fn clocks_close(&self, out: &mut String);
    fn content(&self, out: &mut String, body: &str);
}

struct MdFormat;
struct HtmlFormat;

impl TaskFormat for MdFormat {
    fn doc_open(&self, title: &str) -> String {
        format!("# {title}\n\n")
    }
    fn doc_close(&self, _out: &mut String) {}

    fn day_header(&self, out: &mut String, date: &str) {
        let _ = writeln!(out, "## {date}\n");
    }
    fn section(&self, out: &mut String, title: &str) {
        let _ = write!(out, "### {title}\n\n");
    }
    fn after_section(&self, out: &mut String) {
        out.push('\n');
    }

    fn task_heading(&self, out: &mut String, level: u8, heading: &str, days_offset: Option<i64>) {
        let hashes: String = "#".repeat(level as usize);
        let _ = write!(out, "{hashes} {}", md_escape(heading));
        if let Some(suffix) = offset_suffix(days_offset) {
            let _ = write!(out, "{suffix}");
        }
        out.push('\n');
    }

    fn field(&self, out: &mut String, label: &str, value: &str, code: bool) {
        if code {
            let _ = writeln!(out, "**{label}:** `{value}`");
        } else {
            let _ = writeln!(out, "**{label}:** {value}");
        }
    }

    fn clocks_open(&self, out: &mut String) {
        out.push_str("\n**Clock:**\n");
    }
    fn clock_complete(&self, out: &mut String, start: &str, end: &str, duration: Option<&str>) {
        match duration {
            Some(dur) => {
                let _ = writeln!(out, "- `{start}` → `{end}` ({dur})");
            }
            None => {
                let _ = writeln!(out, "- `{start}` → `{end}`");
            }
        }
    }
    fn clock_active(&self, out: &mut String, start: &str) {
        let _ = writeln!(out, "- `{start}` (active)");
    }
    fn clocks_close(&self, _out: &mut String) {}

    fn content(&self, out: &mut String, body: &str) {
        if body.is_empty() {
            out.push('\n');
        } else {
            let _ = write!(out, "\n{body}\n\n");
        }
    }
}

impl TaskFormat for HtmlFormat {
    fn doc_open(&self, title: &str) -> String {
        format!("<html><body><h1>{title}</h1>\n")
    }
    fn doc_close(&self, out: &mut String) {
        out.push_str("</body></html>");
    }

    fn day_header(&self, out: &mut String, date: &str) {
        let _ = writeln!(out, "<h2>{}</h2>", html_escape(date));
    }
    fn section(&self, out: &mut String, title: &str) {
        let _ = writeln!(out, "<h3>{title}</h3>");
    }
    fn after_section(&self, _out: &mut String) {}

    fn task_heading(&self, out: &mut String, level: u8, heading: &str, days_offset: Option<i64>) {
        let _ = write!(out, "<h{level}>{}", html_escape(heading));
        if let Some(suffix) = offset_suffix(days_offset) {
            let _ = write!(out, "{}", html_escape(&suffix));
        }
        let _ = writeln!(out, "</h{level}>");
    }

    fn field(&self, out: &mut String, label: &str, value: &str, _code: bool) {
        let _ = writeln!(
            out,
            "<p><strong>{label}:</strong> {}</p>",
            html_escape(value)
        );
    }

    fn clocks_open(&self, out: &mut String) {
        out.push_str("<p><strong>Clock:</strong></p>\n<ul>\n");
    }
    fn clock_complete(&self, out: &mut String, start: &str, end: &str, duration: Option<&str>) {
        match duration {
            Some(dur) => {
                let _ = writeln!(
                    out,
                    "<li>{} → {} ({})</li>",
                    html_escape(start),
                    html_escape(end),
                    html_escape(dur)
                );
            }
            None => {
                let _ = writeln!(
                    out,
                    "<li>{} → {}</li>",
                    html_escape(start),
                    html_escape(end)
                );
            }
        }
    }
    fn clock_active(&self, out: &mut String, start: &str) {
        let _ = writeln!(out, "<li>{} (active)</li>", html_escape(start));
    }
    fn clocks_close(&self, out: &mut String) {
        out.push_str("</ul>\n");
    }

    fn content(&self, out: &mut String, body: &str) {
        if !body.is_empty() {
            let _ = writeln!(out, "<p>{}</p>", html_escape(body));
        }
    }
}

/// Write one Task to `out` using the supplied format strategy.
///
/// `level` controls heading depth (2 for top-level lists, 4 for day-agenda
/// sub-sections). `include_history` toggles fields that are only meaningful in
/// the "all tasks" view -- `Created`, `Total Time`, `Clock:` -- so day agendas
/// stay focused on the schedule.
fn write_task<F: TaskFormat>(
    out: &mut String,
    task: &Task,
    days_offset: Option<i64>,
    level: u8,
    include_history: bool,
    fmt: &F,
) {
    fmt.task_heading(out, level, &task.heading, days_offset);

    let file_value = format!("{}:{}", task.file, task.line);
    fmt.field(out, "File", &file_value, true);

    if let Some(ref t) = task.task_type {
        fmt.field(out, "Type", &t.to_string(), false);
    }
    if let Some(ref p) = task.priority {
        fmt.field(out, "Priority", &p.to_string(), false);
    }
    if include_history {
        if let Some(ref c) = task.created {
            fmt.field(out, "Created", c, true);
        }
    }
    if let Some(ref ts) = task.timestamp {
        fmt.field(out, "Time", ts, true);
    }
    if include_history {
        if let Some(ref total) = task.total_clock_time {
            fmt.field(out, "Total Time", total, false);
        }
        if let Some(ref clocks) = task.clocks {
            write_clocks(out, clocks, fmt);
        }
    }

    fmt.content(out, &task.content);
}

fn write_clocks<F: TaskFormat>(out: &mut String, clocks: &[ClockEntry], fmt: &F) {
    fmt.clocks_open(out);
    for clock in clocks {
        match (&clock.end, &clock.duration) {
            (Some(end), Some(dur)) => fmt.clock_complete(out, &clock.start, end, Some(dur)),
            (Some(end), None) => fmt.clock_complete(out, &clock.start, end, None),
            (None, _) => fmt.clock_active(out, &clock.start),
        }
    }
    fmt.clocks_close(out);
}

fn write_day_section<F: TaskFormat>(
    out: &mut String,
    title: &str,
    tasks: &[TaskWithOffset],
    fmt: &F,
) {
    if tasks.is_empty() {
        return;
    }
    fmt.section(out, title);
    for two in tasks {
        write_task(out, &two.task, two.days_offset, 4, false, fmt);
    }
    fmt.after_section(out);
}

fn render_days<F: TaskFormat>(days: &[DayAgenda], fmt: &F) -> String {
    let mut output = fmt.doc_open("Agenda");

    for day in days {
        fmt.day_header(&mut output, &day.date);

        write_day_section(&mut output, "Overdue", &day.overdue, fmt);

        // "Scheduled" header is shared by timed + no-time groups: print it once
        // if either is non-empty, then list both without a second header.
        if !day.scheduled_timed.is_empty() || !day.scheduled_no_time.is_empty() {
            fmt.section(&mut output, "Scheduled");
            for two in &day.scheduled_timed {
                write_task(&mut output, &two.task, two.days_offset, 4, false, fmt);
            }
            for two in &day.scheduled_no_time {
                write_task(&mut output, &two.task, two.days_offset, 4, false, fmt);
            }
            fmt.after_section(&mut output);
        }

        write_day_section(&mut output, "Upcoming", &day.upcoming, fmt);
    }

    fmt.doc_close(&mut output);
    output
}

fn render_tasks<F: TaskFormat>(tasks: &[Task], fmt: &F) -> String {
    let mut output = fmt.doc_open("Tasks");
    for task in tasks {
        write_task(&mut output, task, None, 2, true, fmt);
    }
    fmt.doc_close(&mut output);
    output
}

/// Render day agendas as Markdown
pub fn render_days_markdown(days: &[DayAgenda]) -> String {
    render_days(days, &MdFormat)
}

/// Render day agendas as HTML
pub fn render_days_html(days: &[DayAgenda]) -> String {
    render_days(days, &HtmlFormat)
}

/// Render tasks as Markdown
pub fn render_markdown(tasks: &[Task]) -> String {
    render_tasks(tasks, &MdFormat)
}

/// Render tasks as HTML
pub fn render_html(tasks: &[Task]) -> String {
    render_tasks(tasks, &HtmlFormat)
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
    fn test_html_escape_strips_control_chars() {
        assert_eq!(html_escape("A\u{0000}B"), "AB");
        assert_eq!(html_escape("A\u{0007}B"), "AB"); // BEL
        assert_eq!(html_escape("A\u{007f}B"), "AB"); // DEL
        assert_eq!(html_escape("line1\nline2\tx"), "line1\nline2\tx");
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
    fn test_md_escape_specials() {
        assert_eq!(md_escape("plain"), "plain");
        assert_eq!(md_escape("a*b"), "a\\*b");
        assert_eq!(md_escape("a_b"), "a\\_b");
        assert_eq!(md_escape("# hi"), "\\# hi");
        assert_eq!(md_escape("[link]"), "\\[link\\]");
        assert_eq!(md_escape("<tag>"), "\\<tag\\>");
        assert_eq!(md_escape("a|b"), "a\\|b");
        assert_eq!(md_escape("`code`"), "\\`code\\`");
        assert_eq!(md_escape("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_render_markdown_escapes_heading() {
        let tasks = vec![Task {
            file: "test.md".to_string(),
            line: 1,
            heading: "Fix *important* [#issue]".to_string(),
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
        let out = render_markdown(&tasks);
        assert!(
            out.contains("## Fix \\*important\\* \\[\\#issue\\]"),
            "heading must be escaped: {out}"
        );
    }

    fn fixture_task() -> Task {
        Task {
            file: "notes.md".to_string(),
            line: 42,
            heading: "Test task".to_string(),
            content: "Body text.".to_string(),
            task_type: Some(TaskType::Todo),
            priority: Some(Priority::A),
            created: Some("CREATED: <2025-09-01 Mon>".to_string()),
            timestamp: Some("DEADLINE: <2025-10-01 Wed>".to_string()),
            timestamp_type: Some("DEADLINE".to_string()),
            timestamp_date: Some("2025-10-01".to_string()),
            timestamp_time: None,
            timestamp_end_time: None,
            clocks: None,
            total_clock_time: None,
        }
    }

    #[test]
    fn snapshot_render_markdown_full_task() {
        let out = render_markdown(&[fixture_task()]);
        let expected = "# Tasks\n\n\
## Test task\n\
**File:** `notes.md:42`\n\
**Type:** TODO\n\
**Priority:** A\n\
**Created:** `CREATED: <2025-09-01 Mon>`\n\
**Time:** `DEADLINE: <2025-10-01 Wed>`\n\
\n\
Body text.\n\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn snapshot_render_html_full_task() {
        let out = render_html(&[fixture_task()]);
        let expected = "<html><body><h1>Tasks</h1>\n\
<h2>Test task</h2>\n\
<p><strong>File:</strong> notes.md:42</p>\n\
<p><strong>Type:</strong> TODO</p>\n\
<p><strong>Priority:</strong> A</p>\n\
<p><strong>Created:</strong> CREATED: &lt;2025-09-01 Mon&gt;</p>\n\
<p><strong>Time:</strong> DEADLINE: &lt;2025-10-01 Wed&gt;</p>\n\
<p>Body text.</p>\n\
</body></html>";
        assert_eq!(out, expected);
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
