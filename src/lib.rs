#![warn(missing_docs)]
//! Extract Emacs Org-mode tasks from markdown files.
//!
//! This is the library behind the `markdown-org-extract` command-line tool.
//! Both entry points run the same code: the binary parses arguments and writes
//! bytes, everything below that line lives here. See [`README.md`] at the
//! repository root for the user-facing description of the format itself.
//!
//! # Pipeline
//!
//! A run is two steps, and they are separate so a caller can hold on to the
//! tasks and build several agendas from one scan:
//!
//! 1. [`scan_directory`] walks a directory and returns every [`Task`] it finds.
//! 2. [`filter_agenda`] turns those tasks into an [`AgendaOutput`] for a given
//!    [`AgendaScope`] and date window.
//!
//! ```no_run
//! use markdown_org_extract::{filter_agenda, scan_directory, AgendaDates, AgendaScope, ScanOptions};
//!
//! # fn main() -> Result<(), markdown_org_extract::AppError> {
//! let outcome = scan_directory("notes".as_ref(), &ScanOptions::default(), None)?;
//! let agenda = filter_agenda(
//!     outcome.tasks,
//!     AgendaScope::Day,
//!     AgendaDates::default(),
//!     "Europe/Moscow",
//!     false,
//!     false,
//!     true,
//! )?;
//! # let _ = agenda;
//! # Ok(())
//! # }
//! ```
//!
//! # Determinism
//!
//! Nothing here reads the wall clock unless it has to: `AgendaDates::current_date`
//! overrides "today" so a caller can render the agenda as it would look on any
//! date. Consumers are expected to pass it rather than rely on the host clock.
//!
//! [`README.md`]: https://github.com/VitalyOstanin/markdown-org-extract

pub mod agenda;
pub mod clock;
pub mod error;
pub mod holidays;
pub mod locale;
pub mod parser;
mod regex_limits;
pub mod render;
pub mod scan;
pub mod timestamp;
pub mod types;

// The flat facade is the surface embedders are expected to use; the modules
// stay public so anything not re-exported here is still reachable without
// waiting for a release.
pub use crate::agenda::{filter_agenda, AgendaDates, AgendaOutput, AgendaScope};
pub use crate::error::AppError;
pub use crate::holidays::HolidayCalendar;
pub use crate::locale::get_weekday_mappings;
pub use crate::parser::{
    extract_tasks, extract_tasks_with_counter, parse_heading_line, HeadingLine, HeadingToken,
};
pub use crate::render::{render_days_html, render_days_markdown, render_html, render_markdown};
pub use crate::scan::{scan_directory, ScanOptions, ScanOutcome};
pub use crate::types::{
    ClockEntry, DayAgenda, Priority, ProcessingStats, Task, TaskType, TaskWithOffset,
    DEFAULT_MAX_TASKS, MAX_FILE_SIZE,
};
