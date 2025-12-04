use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Task status type (TODO or DONE)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TaskType {
    Todo,
    Done,
}

impl TaskType {
    /// Parse task type from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "TODO" => Some(TaskType::Todo),
            "DONE" => Some(TaskType::Done),
            _ => None,
        }
    }
}

/// Task priority (A is highest, C is lowest)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    A,
    B,
    C,
    Other(char),
}

impl Priority {
    /// Create priority from character (validates A-Z only)
    pub fn from_char(c: char) -> Option<Self> {
        if !c.is_ascii_uppercase() {
            return None;
        }
        match c {
            'A' => Some(Priority::A),
            'B' => Some(Priority::B),
            'C' => Some(Priority::C),
            'D'..='Z' => Some(Priority::Other(c)),
            _ => None,
        }
    }

    /// Get numeric order for sorting (lower is higher priority)
    pub fn order(&self) -> u32 {
        match self {
            Priority::A => 0,
            Priority::B => 1,
            Priority::C => 2,
            Priority::Other(c) => (*c as u32) - ('A' as u32),
        }
    }
}

/// Extracted task from markdown file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub file: String,
    pub line: u32,
    pub heading: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<TaskType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_end_time: Option<String>,
}

/// Maximum file size to process (10 MB)
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Maximum number of tasks to extract
pub const MAX_TASKS: usize = 10_000;

/// Statistics for file processing
#[derive(Debug, Default)]
pub struct ProcessingStats {
    pub files_processed: usize,
    pub files_skipped_size: usize,
    pub files_failed_search: usize,
    pub files_failed_read: usize,
}

impl ProcessingStats {
    pub fn has_warnings(&self) -> bool {
        self.files_skipped_size > 0 || self.files_failed_search > 0 || self.files_failed_read > 0
    }

    pub fn print_summary(&self) {
        if self.has_warnings() {
            eprintln!("\nProcessing summary:");
            eprintln!("  Files processed: {}", self.files_processed);
            if self.files_skipped_size > 0 {
                eprintln!("  Files skipped (too large): {}", self.files_skipped_size);
            }
            if self.files_failed_search > 0 {
                eprintln!("  Files failed to search: {}", self.files_failed_search);
            }
            if self.files_failed_read > 0 {
                eprintln!("  Files failed to read: {}", self.files_failed_read);
            }
        }
    }
}

/// Task with day offset information
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskWithOffset {
    #[serde(flatten)]
    pub task: Task,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_offset: Option<i64>,
}

/// Day agenda containing tasks for a specific date
#[derive(Debug, Serialize, Deserialize)]
pub struct DayAgenda {
    pub date: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overdue: Vec<TaskWithOffset>,
    pub scheduled_timed: Vec<TaskWithOffset>,
    pub scheduled_no_time: Vec<TaskWithOffset>,
    pub upcoming: Vec<TaskWithOffset>,
}

impl DayAgenda {
    pub fn new(date: NaiveDate) -> Self {
        Self {
            date: date.format("%Y-%m-%d").to_string(),
            overdue: Vec::new(),
            scheduled_timed: Vec::new(),
            scheduled_no_time: Vec::new(),
            upcoming: Vec::new(),
        }
    }
}
