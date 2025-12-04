use crate::types::Task;

pub fn render_markdown(tasks: &[Task]) -> String {
    let mut output = String::from("# Tasks\n\n");
    for task in tasks {
        output.push_str(&format!("## {}\n", task.heading));
        output.push_str(&format!("**File:** {}:{}\n", task.file, task.line));
        if let Some(ref t) = task.task_type {
            output.push_str(&format!("**Type:** {:?}\n", t));
        }
        if let Some(ref p) = task.priority {
            output.push_str(&format!("**Priority:** {:?}\n", p));
        }
        if let Some(ref c) = task.created {
            output.push_str(&format!("**Created:** {}\n", c));
        }
        if let Some(ref ts) = task.timestamp {
            output.push_str(&format!("**Time:** {}\n", ts));
        }
        if !task.content.is_empty() {
            output.push_str(&format!("\n{}\n\n", task.content));
        } else {
            output.push('\n');
        }
    }
    output
}

pub fn render_html(tasks: &[Task]) -> String {
    let mut output = String::from("<html><body><h1>Tasks</h1>\n");
    for task in tasks {
        output.push_str(&format!("<h2>{}</h2>\n", task.heading));
        output.push_str(&format!("<p><strong>File:</strong> {}:{}</p>\n", task.file, task.line));
        if let Some(ref t) = task.task_type {
            output.push_str(&format!("<p><strong>Type:</strong> {:?}</p>\n", t));
        }
        if let Some(ref p) = task.priority {
            output.push_str(&format!("<p><strong>Priority:</strong> {:?}</p>\n", p));
        }
        if let Some(ref c) = task.created {
            output.push_str(&format!("<p><strong>Created:</strong> {}</p>\n", c));
        }
        if let Some(ref ts) = task.timestamp {
            output.push_str(&format!("<p><strong>Time:</strong> {}</p>\n", ts));
        }
        if !task.content.is_empty() {
            output.push_str(&format!("<p>{}</p>\n", task.content));
        }
    }
    output.push_str("</body></html>");
    output
}
