use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Task status type (TODO or DONE)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TaskType {
    Todo,
    Done,
}

impl fmt::Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TaskType::Todo => "TODO",
            TaskType::Done => "DONE",
        })
    }
}

impl TaskType {
    /// Parse task type from an org-mode keyword (`TODO` / `DONE`)
    pub fn from_keyword(s: &str) -> Option<Self> {
        match s {
            "TODO" => Some(TaskType::Todo),
            "DONE" => Some(TaskType::Done),
            _ => None,
        }
    }
}

/// Task priority (A is highest, C is lowest; D-Z preserved as `Other`)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    A,
    B,
    C,
    Other(char),
}

impl Priority {
    /// Create priority from character. Only ASCII upper-case letters A-Z are accepted.
    pub fn from_char(c: char) -> Option<Self> {
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

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Priority::A => f.write_str("A"),
            Priority::B => f.write_str("B"),
            Priority::C => f.write_str("C"),
            Priority::Other(c) => write!(f, "{c}"),
        }
    }
}

/// Clock entry representing time tracking.
///
/// Mirrors org-mode CLOCK lines. The entry has two shapes:
/// - **Closed clock** — `CLOCK: [start]--[end] =>  HH:MM`. All three fields
///   are present: `start`, `end = Some(_)`, `duration = Some(_)`.
/// - **Open clock** — `CLOCK: [start]`. Only `start` is set; `end` and
///   `duration` are `None`. An open clock represents an in-progress
///   interval whose endpoint has not been recorded yet, so the consumer
///   is responsible for deciding how (or whether) to render it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClockEntry {
    pub start: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
}

/// A single task extracted from a markdown file.
///
/// All optional fields are skipped on serialization when `None`, so the JSON
/// output stays compact and stable.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clocks: Option<Vec<ClockEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_clock_time: Option<String>,
}

/// Maximum file size to process (10 MB)
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Default value for the `--max-tasks` CLI flag.
///
/// Acts as both a per-file and a global cap during scanning, configurable via
/// `--max-tasks`. The default is conservative; legitimate workloads stay well
/// under it, while pathological / hostile inputs hit it quickly.
pub const DEFAULT_MAX_TASKS: usize = 10_000;

/// File processing statistics surfaced to stderr after a run.
#[derive(Debug, Default)]
pub struct ProcessingStats {
    pub files_processed: usize,
    pub files_skipped_size: usize,
    pub files_failed_search: usize,
    pub files_failed_read: usize,
    pub max_tasks_reached: bool,
    /// Configured task limit (from `--max-tasks`). Reported in the summary so
    /// users know which limit they hit and can rerun with a higher value.
    pub max_tasks_limit: usize,
    /// Paths of files that could not be read or searched. Capped to avoid unbounded growth.
    pub failed_paths: Vec<String>,
}

impl ProcessingStats {
    pub fn has_warnings(&self) -> bool {
        self.files_skipped_size > 0
            || self.files_failed_search > 0
            || self.files_failed_read > 0
            || self.max_tasks_reached
    }

    pub fn record_failed_path(&mut self, path: &str) {
        const MAX_REPORT: usize = 20;
        if self.failed_paths.len() < MAX_REPORT {
            self.failed_paths.push(path.to_string());
        }
    }

    pub fn print_summary(&self) {
        if !self.has_warnings() {
            return;
        }
        tracing::warn!(
            files_processed = self.files_processed,
            files_skipped_size = self.files_skipped_size,
            files_failed_search = self.files_failed_search,
            files_failed_read = self.files_failed_read,
            max_tasks_reached = self.max_tasks_reached,
            max_tasks_limit = self.max_tasks_limit,
            "processing summary"
        );
        if !self.failed_paths.is_empty() {
            tracing::warn!(
                count = self.failed_paths.len(),
                "failed paths (up to first 20):"
            );
            for p in &self.failed_paths {
                tracing::warn!(path = %p, "failed path");
            }
        }
    }
}

/// Task paired with the number of days from the current date.
/// Used for agenda rendering (overdue / upcoming).
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskWithOffset {
    #[serde(flatten)]
    pub task: Task,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_offset: Option<i64>,
}

/// Tasks aggregated for a specific date, split into overdue / scheduled / upcoming buckets.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_type_from_keyword() {
        assert_eq!(TaskType::from_keyword("TODO"), Some(TaskType::Todo));
        assert_eq!(TaskType::from_keyword("DONE"), Some(TaskType::Done));
        assert_eq!(TaskType::from_keyword("MAYBE"), None);
    }

    #[test]
    fn priority_from_char_letters() {
        assert_eq!(Priority::from_char('A'), Some(Priority::A));
        assert_eq!(Priority::from_char('B'), Some(Priority::B));
        assert_eq!(Priority::from_char('C'), Some(Priority::C));
        assert_eq!(Priority::from_char('Z'), Some(Priority::Other('Z')));
    }

    #[test]
    fn priority_from_char_rejects_lower_and_digits() {
        assert_eq!(Priority::from_char('a'), None);
        assert_eq!(Priority::from_char('1'), None);
        assert_eq!(Priority::from_char('@'), None);
    }

    #[test]
    fn priority_order() {
        assert!(Priority::A.order() < Priority::B.order());
        assert!(Priority::B.order() < Priority::C.order());
        assert!(Priority::C.order() < Priority::Other('D').order());
    }
}
