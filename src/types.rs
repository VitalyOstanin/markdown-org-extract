//! The data model shared by every stage of the pipeline.
//!
//! [`Task`] is what a scan produces and what everything downstream consumes;
//! its serde representation is the JSON contract the CLI emits, so field names
//! and optionality here are load-bearing for external consumers (ADR-0015
//! governs how they may evolve).

use chrono::NaiveDate;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

/// Original spelling of the cancelled TODO keyword, preserved verbatim.
///
/// Both spellings are user conventions (neither is built into upstream
/// Emacs Org-mode); the source file's spelling is kept so reverse-sync
/// consumers do not silently rewrite it. See ADR-0021.
///
/// - `DoubleL` = `CANCELLED`
/// - `SingleL` = `CANCELED` (the spelling used in the upstream manual)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelledSpelling {
    /// `CANCELLED`.
    DoubleL,
    /// `CANCELED`.
    SingleL,
}

/// Task status type (TODO, DONE, or cancelled).
///
/// The cancelled variant carries the original spelling from the source
/// file (`CANCELLED` / `CANCELED`) so it can be reproduced on output. See
/// ADR-0021. Serialises to a plain JSON string (`"TODO"` / `"DONE"` /
/// `"CANCELLED"` / `"CANCELED"`), not the default externally-tagged object
/// form, which is why `Serialize` / `Deserialize` are hand-written rather
/// than derived.
#[derive(Debug, Clone, PartialEq)]
pub enum TaskType {
    /// Open task (`TODO`).
    Todo,
    /// Completed task (`DONE`).
    Done,
    /// Abandoned task, carrying the spelling used in the source file.
    Cancelled(CancelledSpelling),
}

impl fmt::Display for TaskType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TaskType::Todo => "TODO",
            TaskType::Done => "DONE",
            TaskType::Cancelled(CancelledSpelling::DoubleL) => "CANCELLED",
            TaskType::Cancelled(CancelledSpelling::SingleL) => "CANCELED",
        })
    }
}

impl TaskType {
    /// Parse task type from an org-mode keyword
    /// (`TODO` / `DONE` / `CANCELLED` / `CANCELED`). The two cancelled
    /// spellings map to distinct `CancelledSpelling` variants so the
    /// original form is preserved.
    pub fn from_keyword(s: &str) -> Option<Self> {
        match s {
            "TODO" => Some(TaskType::Todo),
            "DONE" => Some(TaskType::Done),
            "CANCELLED" => Some(TaskType::Cancelled(CancelledSpelling::DoubleL)),
            "CANCELED" => Some(TaskType::Cancelled(CancelledSpelling::SingleL)),
            _ => None,
        }
    }
}

impl Serialize for TaskType {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TaskType {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct TaskTypeVisitor;
        impl Visitor<'_> for TaskTypeVisitor {
            type Value = TaskType;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("one of \"TODO\", \"DONE\", \"CANCELLED\", \"CANCELED\"")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<TaskType, E> {
                TaskType::from_keyword(v).ok_or_else(|| {
                    de::Error::unknown_variant(v, &["TODO", "DONE", "CANCELLED", "CANCELED"])
                })
            }
        }
        de.deserialize_str(TaskTypeVisitor)
    }
}

/// Task priority.
///
/// Mirrors org-mode's `org-priority-value-regexp`: a single uppercase Latin
/// letter `A-Z`, or an integer in the range `0..=64`. Lower numeric `order`
/// means higher priority, matching `org-priority-to-value` semantics:
///
/// - `Numeric(n)` → `n` (so `0` is highest, `64` is lowest in the numeric range)
/// - `A` → 65, `B` → 66, `C` → 67, `Other('D')` → 68, …, `Other('Z')` → 90
///
/// Variant order in the `enum` declaration mirrors this priority order, so
/// `derive(Ord)` yields the same comparison as `order()`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Numeric priority `[#0]`..`[#64]`. Outside this range is rejected.
    Numeric(u8),
    /// `[#A]` — the highest letter priority.
    A,
    /// `[#B]`.
    B,
    /// `[#C]` — org-mode's default when a task carries no explicit priority.
    C,
    /// Letters D-Z, preserved verbatim.
    Other(char),
}

impl Priority {
    /// Parse priority from the captured value of `\[#X\]`, i.e. without the
    /// surrounding brackets. Accepts a single uppercase letter `A-Z` or a
    /// decimal integer in the range `0..=64`.
    pub fn parse(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }
        let bytes = s.as_bytes();
        if bytes.len() == 1 {
            let b = bytes[0];
            if b.is_ascii_uppercase() {
                return Some(match b {
                    b'A' => Priority::A,
                    b'B' => Priority::B,
                    b'C' => Priority::C,
                    _ => Priority::Other(b as char),
                });
            }
        }
        // Decimal integer 0..=64. Reject leading zeros longer than one digit
        // ("01") to stay close to org-mode's `[0-9]\|[1-5][0-9]\|6[0-4]`,
        // which never matches a leading-zero two-digit run.
        if bytes.len() > 1 && bytes[0] == b'0' {
            return None;
        }
        if !bytes.iter().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let n: u8 = s.parse().ok()?;
        if n <= 64 {
            Some(Priority::Numeric(n))
        } else {
            None
        }
    }

    /// Get numeric order for sorting (lower is higher priority).
    ///
    /// Implements `org-priority-to-value`: numbers map to themselves,
    /// letters map to their ASCII code (`'A' as u32 == 65`).
    pub fn order(&self) -> u32 {
        match self {
            Priority::Numeric(n) => *n as u32,
            Priority::A => 'A' as u32,
            Priority::B => 'B' as u32,
            Priority::C => 'C' as u32,
            Priority::Other(c) => *c as u32,
        }
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Priority::Numeric(n) => write!(f, "{n}"),
            Priority::A => f.write_str("A"),
            Priority::B => f.write_str("B"),
            Priority::C => f.write_str("C"),
            Priority::Other(c) => write!(f, "{c}"),
        }
    }
}

impl FromStr for Priority {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Priority::parse(s).ok_or(())
    }
}

impl Serialize for Priority {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        ser.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Priority {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct PriorityVisitor;
        impl Visitor<'_> for PriorityVisitor {
            type Value = Priority;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an uppercase letter A-Z or an integer 0..=64")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Priority, E> {
                Priority::parse(v).ok_or_else(|| E::custom(format!("invalid priority: {v}")))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Priority, E> {
                if v <= 64 {
                    Ok(Priority::Numeric(v as u8))
                } else {
                    Err(E::custom(format!("priority out of range: {v}")))
                }
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Priority, E> {
                if (0..=64).contains(&v) {
                    Ok(Priority::Numeric(v as u8))
                } else {
                    Err(E::custom(format!("priority out of range: {v}")))
                }
            }
        }
        de.deserialize_any(PriorityVisitor)
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
    /// Inactive timestamp the interval started at, verbatim from the file.
    pub start: String,
    /// Inactive timestamp the interval ended at; `None` for an open clock.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    /// Elapsed time as `HH:MM`; `None` for an open clock.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
}

/// A single task extracted from a markdown file.
///
/// All optional fields are skipped on serialization when `None`, so the JSON
/// output stays compact and stable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Task {
    /// Path to the file the task was found in — relative to the scanned root
    /// unless [`ScanOptions::absolute_paths`] was set.
    ///
    /// [`ScanOptions::absolute_paths`]: crate::scan::ScanOptions::absolute_paths
    pub file: String,
    /// Canonical path of the root `file` is relative to, when the run scanned
    /// several of them ([`scan_directories`]). `None` for a scan of one
    /// directory, where the caller named the root itself and the field would
    /// only repeat what it already knows.
    ///
    /// The same relative path can occur in two roots and mean two different
    /// files, so a consumer that scans several roots joins this back on before
    /// opening or editing a note. Under
    /// [`ScanOptions::absolute_paths`](crate::scan::ScanOptions::absolute_paths)
    /// `file` already names the file on its own and this field merely says
    /// which collection it belongs to. Added as a non-breaking optional field
    /// under ADR-0015.
    ///
    /// [`scan_directories`]: crate::scan::scan_directories
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// 1-based line number of the heading inside that file.
    pub line: u32,
    /// Heading text with the keyword, priority and tags stripped off.
    pub heading: String,
    /// Body of the section below the heading, up to the next heading.
    pub content: String,
    /// Org keyword the heading carried, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<TaskType>,
    /// Priority cookie `[#A]`, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    /// Creation date recorded for the task, if the file carries one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// The timestamp line verbatim, e.g. `SCHEDULED: <2026-03-02 Mon>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Keyword introducing the timestamp: `SCHEDULED`, `DEADLINE` or `CLOSED`;
    /// `None` for a bare timestamp with no keyword.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_type: Option<String>,
    /// Bracket form of the timestamp: `Some(true)` for active `<...>`,
    /// `Some(false)` for inactive `[...]`, `None` when no timestamp is
    /// present. See ADR-0014 for the per-keyword policy and ADR-0015 for
    /// the schema-evolution rule under which this field was added.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_active: Option<bool>,
    /// Date part of the timestamp as `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_date: Option<String>,
    /// Start time as `HH:MM`; `None` for an all-day timestamp.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_time: Option<String>,
    /// End time as `HH:MM` for a time range (`<... 10:00-11:30>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_end_time: Option<String>,
    /// Canonical org repeater string of the active timestamp (`++7d`,
    /// `.+1m`, `+1wd`), or `None` when the timestamp carries no repeater.
    /// Added as a non-breaking optional field under ADR-0015; consumers
    /// that predate it simply ignore it. Downstream (the VS Code
    /// extension) maps this to a Google Calendar `RRULE`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_repeater: Option<String>,
    /// Resolved next occurrence date (`YYYY-MM-DD`) for a task that carries a
    /// repeater, computed relative to "now": the closest occurrence that is
    /// still upcoming (today-or-later; a timed occurrence counts as upcoming
    /// only until its clock time passes). `None` for non-repeating tasks or
    /// when the date/repeater cannot be resolved. Added as a non-breaking
    /// optional field (same schema-evolution rule as `timestamp_repeater`);
    /// consumers that predate it ignore it. The VS Code agenda shows it in the
    /// repeat tooltip so "next" never names a past occurrence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_next: Option<String>,
    /// Resolved next occurrence date (`YYYY-MM-DD`) **after the day this copy
    /// is dated to**, for a repeating task rendered on one of its occurrences.
    /// Where `timestamp_next` answers "when is the next one from now" and
    /// carries the same value in every cell, this answers "when is the one
    /// after this" and differs per cell.
    ///
    /// Present only on the scheduled buckets, which are the cells a task is
    /// drawn on its own date. It is absent from `overdue` and `upcoming`,
    /// whose entries are copies borrowed into today's agenda: there the
    /// question is still "when next from now", which `timestamp_next` answers.
    ///
    /// Anchored on the task's own date rather than on the rewritten one, for
    /// the reason `annotate_next_occurrences` documents: a monthly repeater
    /// anchored on the 31st loses its day-of-month if it is restarted from a
    /// truncated 28.02. Added as a non-breaking optional field under
    /// ADR-0015; see ADR-0029.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_next_after: Option<String>,
    /// `CLOCK:` entries recorded in the task body, in file order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clocks: Option<Vec<ClockEntry>>,
    /// Sum of every closed clock entry as `HH:MM`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_clock_time: Option<String>,
    /// Per-task properties parsed from an `org-properties` fenced code
    /// block (bare `KEY: value` lines). `None` when the task has no such
    /// block. Added as a non-breaking optional field under ADR-0015; the
    /// on-disk format and parsing rules are ADR-0020. `BTreeMap` gives a
    /// deterministic key order for snapshot/JSON assertions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<BTreeMap<String, String>>,
    /// Occurrences this repeating entry does not have, as `YYYY-MM-DD`,
    /// read from the `EXDATE` property (ADR-0031). `None` when the entry
    /// declares none; an unparsable date is dropped on the way in, so what
    /// is here is always a date the agenda can compare against.
    ///
    /// It travels on the copy of the entry in every agenda cell as well,
    /// where it says nothing that cell needs — the list belongs to the series
    /// rather than to one occurrence of it. Kept there deliberately: taking a
    /// field off an output that has emitted it is a breaking change under
    /// ADR-0015, and the size it costs shows up only on a value nobody
    /// writes by hand (a review measured 200 dates in one `EXDATE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded_dates: Option<Vec<String>>,
    /// The occurrence this entry replaces, as `YYYY-MM-DD` optionally
    /// followed by ` HH:MM` — the start that occurrence *would* have had,
    /// not the date this entry carries. Read from `RECURRENCE_ID`
    /// (ADR-0031); meaningful only together with [`Task::series_id`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence_id: Option<String>,
    /// The `ID` of the series this entry replaces an occurrence of, read
    /// from `SERIES_ID` (ADR-0031). This is RFC 5545's `UID` half of the
    /// (`UID`, `RECURRENCE-ID`) pair.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<String>,
}

/// Maximum file size to process (10 MB)
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Default value for the `--max-tasks` CLI flag.
///
/// Acts as a global cap on the total number of extracted tasks; the same
/// value is reused as a per-file cap so a single hostile file cannot exhaust
/// the global budget on its own. Configurable via `--max-tasks`. The default
/// is conservative; legitimate workloads stay well under it, while
/// pathological / hostile inputs hit it quickly.
pub const DEFAULT_MAX_TASKS: usize = 10_000;

/// Per-run cap on user-visible diagnostic entries (failed-path list,
/// invalid-timestamp warnings). Beyond this we stop appending so a corrupt
/// or hostile input cannot flood stderr or `ProcessingStats`. The value is
/// shared across categories because the UX rationale is identical
/// ("20 entries is already noisy; the rest can be inferred from totals").
pub const MAX_DIAGNOSTIC_ITEMS: usize = 20;

/// File processing statistics surfaced to stderr after a run.
#[derive(Debug, Default)]
pub struct ProcessingStats {
    /// Files that were read and searched to completion.
    pub files_processed: usize,
    /// Files skipped because they exceed [`MAX_FILE_SIZE`].
    pub files_skipped_size: usize,
    /// Files whose content could not be searched.
    pub files_failed_search: usize,
    /// Files that could not be opened or read.
    pub files_failed_read: usize,
    /// Files read in full and then skipped because their bytes are not valid
    /// UTF-8 — a note saved in a single-byte encoding, typically by a Windows
    /// editor. Counted apart from [`files_failed_read`] because the two need
    /// different answers: this one is fixed by converting the file, not by
    /// retrying or fixing permissions.
    ///
    /// [`files_failed_read`]: ProcessingStats::files_failed_read
    pub files_not_utf8: usize,
    /// Walker-level entries the scanner could not even enumerate
    /// (e.g. `PermissionDenied` on a subdirectory). Counted separately so a
    /// single unreadable subtree does not silently mask the rest of the scan.
    pub walk_errors: usize,
    /// The task limit was hit, so the result is truncated.
    pub max_tasks_reached: bool,
    /// Configured task limit (from `--max-tasks`). Reported in the summary so
    /// users know which limit they hit and can rerun with a higher value.
    pub max_tasks_limit: usize,
    /// Paths of files that could not be read or searched. Capped to avoid unbounded growth.
    pub failed_paths: Vec<String>,
    /// Cumulative count of invalid-timestamp warnings encountered during the
    /// scan, threaded through `extract_tasks_with_counter`. The first
    /// `MAX_DIAGNOSTIC_ITEMS` are emitted verbatim; the next one collapses
    /// into a single "suppressed (showed first N)" notice; further ones are
    /// silent. Owned by `ProcessingStats` so the budget spans every file in
    /// the run without resorting to process-global state.
    pub ts_warnings_emitted: usize,
    /// Cumulative count of `org-properties` diagnostics: a malformed line
    /// (one with no `:`), and a value of an exception key that cannot be used
    /// as written — an unreadable date in `EXDATE`, a `RECURRENCE_ID` that
    /// names no occurrence, half a `SERIES_ID` / `RECURRENCE_ID` pair
    /// (ADR-0020, ADR-0031). Kept apart from `ts_warnings_emitted` because a
    /// mistake in a property is not a mistake in an entry's timestamp: a file
    /// full of unreadable `EXDATE` values must not spend the budget that
    /// warns about the timestamps of every file after it. Gated by
    /// `MAX_DIAGNOSTIC_ITEMS` exactly like `ts_warnings_emitted`, and owned
    /// here so the budget spans every file in the run.
    pub prop_warnings_emitted: usize,
    /// Scan was aborted by SIGINT/SIGTERM before all entries were visited.
    /// Surfaced in the summary so the user knows the output reflects only the
    /// portion processed up to the signal.
    pub interrupted: bool,
    /// Count of processed files whose path is not valid UTF-8 (legal on Linux,
    /// where filenames are arbitrary non-NUL byte sequences; possible on
    /// Windows via unpaired surrogates; not reachable on macOS). Their `file`
    /// field is rendered lossily — `Path::display` substitutes U+FFFD for the
    /// invalid bytes — so the path may not round-trip for a consumer. The
    /// file itself is still read and its tasks emitted. See ADR-0019.
    pub nonutf8_paths: usize,
}

impl ProcessingStats {
    /// Whether the run produced anything worth reporting: a skipped or failed
    /// file, a truncated result, an interruption or a non-UTF-8 path.
    pub fn has_warnings(&self) -> bool {
        self.files_skipped_size > 0
            || self.files_failed_search > 0
            || self.files_failed_read > 0
            || self.files_not_utf8 > 0
            || self.walk_errors > 0
            || self.max_tasks_reached
            || self.interrupted
            || self.nonutf8_paths > 0
    }

    /// Remember a path that could not be processed, up to
    /// [`MAX_DIAGNOSTIC_ITEMS`] entries — beyond that only the counters grow,
    /// so an unreadable tree cannot make the list unbounded.
    pub fn record_failed_path(&mut self, path: &str) {
        if self.failed_paths.len() < MAX_DIAGNOSTIC_ITEMS {
            self.failed_paths.push(path.to_string());
        }
    }

    /// Record a processed file whose path is not valid UTF-8. The first such
    /// path in a run emits one `warn` (with its lossy U+FFFD rendering for
    /// context); later ones only bump the counter, so a directory full of
    /// non-UTF-8 names cannot flood stderr. The aggregate count is also
    /// surfaced in `print_summary`. See ADR-0019.
    pub fn note_nonutf8_path(&mut self, lossy: &str) {
        if self.nonutf8_paths == 0 {
            tracing::warn!(
                file = %lossy,
                "file path is not valid UTF-8; the `file` field is rendered with U+FFFD replacement characters and may not round-trip. Further such paths this run are counted in the summary only."
            );
        }
        self.nonutf8_paths += 1;
    }

    /// Emit the end-of-run summary as one structured `warn` record, or
    /// nothing at all when [`has_warnings`] is false.
    ///
    /// [`has_warnings`]: ProcessingStats::has_warnings
    pub fn print_summary(&self) {
        if !self.has_warnings() {
            return;
        }
        // The 0.5.0 observability review (O5) merged the previous trio
        // (one summary record, one `failed paths (up to first N)`
        // header, and one record per path -- up to 22 warn lines in a
        // row) into a single structured record. The `failed_paths`
        // field carries the whole list (still capped to
        // `MAX_DIAGNOSTIC_ITEMS` at insertion time) so that jq / grep
        // can extract it without stitching together multiple lines,
        // and so it no longer drowns out real per-file warnings on a
        // noisy run.
        tracing::warn!(
            files_processed = self.files_processed,
            files_skipped_size = self.files_skipped_size,
            files_failed_search = self.files_failed_search,
            files_failed_read = self.files_failed_read,
            files_not_utf8 = self.files_not_utf8,
            walk_errors = self.walk_errors,
            max_tasks_reached = self.max_tasks_reached,
            max_tasks_limit = self.max_tasks_limit,
            interrupted = self.interrupted,
            nonutf8_paths = self.nonutf8_paths,
            failed_paths_count = self.failed_paths.len(),
            failed_paths_cap = MAX_DIAGNOSTIC_ITEMS,
            failed_paths = ?self.failed_paths,
            "processing summary"
        );
    }
}

/// Task paired with the number of days from the current date.
/// Used for agenda rendering (overdue / upcoming).
#[derive(Debug, Serialize, Deserialize)]
pub struct TaskWithOffset {
    /// The task itself; flattened on serialization, so its fields appear
    /// alongside `days_offset` rather than nested under a key.
    #[serde(flatten)]
    pub task: Task,
    /// Days between the agenda date and the task's timestamp: negative for
    /// overdue, positive for upcoming. `None` when the task has no date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_offset: Option<i64>,
}

/// Tasks aggregated for a specific date, split into overdue / scheduled / upcoming buckets.
#[derive(Debug, Serialize, Deserialize)]
pub struct DayAgenda {
    /// The day this bucket describes, as `YYYY-MM-DD`.
    pub date: String,
    /// Tasks whose date has already passed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub overdue: Vec<TaskWithOffset>,
    /// Tasks falling on this day at a specific time, ordered by that time.
    pub scheduled_timed: Vec<TaskWithOffset>,
    /// All-day tasks falling on this day.
    pub scheduled_no_time: Vec<TaskWithOffset>,
    /// Tasks dated later than this day but close enough to warn about.
    pub upcoming: Vec<TaskWithOffset>,
}

impl DayAgenda {
    /// An empty agenda for `date`.
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
        // Both cancelled spellings are recognised and preserved (ADR-0021).
        assert_eq!(
            TaskType::from_keyword("CANCELLED"),
            Some(TaskType::Cancelled(CancelledSpelling::DoubleL))
        );
        assert_eq!(
            TaskType::from_keyword("CANCELED"),
            Some(TaskType::Cancelled(CancelledSpelling::SingleL))
        );
        // case-sensitivity: lowercase must not match
        assert_eq!(TaskType::from_keyword("cancelled"), None);
        assert_eq!(TaskType::from_keyword("canceled"), None);
    }

    #[test]
    fn task_type_display() {
        assert_eq!(TaskType::Todo.to_string(), "TODO");
        assert_eq!(TaskType::Done.to_string(), "DONE");
        assert_eq!(
            TaskType::Cancelled(CancelledSpelling::DoubleL).to_string(),
            "CANCELLED"
        );
        assert_eq!(
            TaskType::Cancelled(CancelledSpelling::SingleL).to_string(),
            "CANCELED"
        );
    }

    #[test]
    fn task_type_serializes_as_plain_string_preserving_spelling() {
        // ADR-0021: task_type is a plain JSON string reflecting the original
        // cancelled spelling, not the default externally-tagged object form.
        assert_eq!(serde_json::to_string(&TaskType::Todo).unwrap(), "\"TODO\"");
        assert_eq!(serde_json::to_string(&TaskType::Done).unwrap(), "\"DONE\"");
        assert_eq!(
            serde_json::to_string(&TaskType::Cancelled(CancelledSpelling::DoubleL)).unwrap(),
            "\"CANCELLED\""
        );
        assert_eq!(
            serde_json::to_string(&TaskType::Cancelled(CancelledSpelling::SingleL)).unwrap(),
            "\"CANCELED\""
        );
    }

    #[test]
    fn task_type_round_trips_through_serde() {
        for tt in [
            TaskType::Todo,
            TaskType::Done,
            TaskType::Cancelled(CancelledSpelling::DoubleL),
            TaskType::Cancelled(CancelledSpelling::SingleL),
        ] {
            let json = serde_json::to_string(&tt).unwrap();
            let back: TaskType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, tt, "round trip must preserve the variant: {json}");
        }
    }

    #[test]
    fn task_type_deserialize_rejects_unknown_string() {
        let r: Result<TaskType, _> = serde_json::from_str("\"MAYBE\"");
        assert!(r.is_err(), "unknown task_type string must fail to parse");
    }

    #[test]
    fn interrupted_flag_makes_summary_visible() {
        // A run interrupted by Ctrl-C (SIGINT/SIGTERM) must surface a summary
        // even when no per-file failures accumulated — otherwise a user who
        // aborts a long scan would see no acknowledgement that processing was
        // partial. The `interrupted` flag is therefore part of `has_warnings`.
        let mut stats = ProcessingStats::default();
        assert!(!stats.has_warnings(), "default stats must be quiet");
        stats.interrupted = true;
        assert!(
            stats.has_warnings(),
            "interrupted runs must always show a summary"
        );
    }

    #[test]
    fn note_nonutf8_path_counts_and_makes_summary_visible() {
        // ADR-0019: a non-UTF-8 path is processed but rendered lossily. The
        // count is tracked so the summary surfaces it, and the bucket is part
        // of `has_warnings` so a run that hit only such paths still prints a
        // summary. The warn-once side effect is exercised end-to-end by the
        // `non_utf8_path_is_processed_and_warned` CLI test; here we pin the
        // accounting, which is what gates the single warn.
        let mut stats = ProcessingStats::default();
        assert!(!stats.has_warnings(), "default stats must be quiet");
        stats.note_nonutf8_path("bad\u{FFFD}name.md");
        stats.note_nonutf8_path("other\u{FFFD}.md");
        assert_eq!(
            stats.nonutf8_paths, 2,
            "every non-UTF-8 path must be counted"
        );
        assert!(
            stats.has_warnings(),
            "a run with non-UTF-8 paths must surface a summary"
        );
    }

    #[test]
    fn record_failed_path_caps_list_at_diagnostic_limit() {
        // The summary shouldn't grow without bound — a directory with millions
        // of unreadable files must not consume O(n) memory in `failed_paths`.
        let mut stats = ProcessingStats::default();
        for i in 0..(MAX_DIAGNOSTIC_ITEMS * 3) {
            stats.record_failed_path(&format!("/tmp/file-{i}.md"));
        }
        assert_eq!(
            stats.failed_paths.len(),
            MAX_DIAGNOSTIC_ITEMS,
            "failed_paths must be capped at MAX_DIAGNOSTIC_ITEMS regardless of input size"
        );
        // Order is "first N" — index 0 keeps the very first path.
        assert_eq!(stats.failed_paths[0], "/tmp/file-0.md");
        assert_eq!(
            stats.failed_paths[MAX_DIAGNOSTIC_ITEMS - 1],
            format!("/tmp/file-{}.md", MAX_DIAGNOSTIC_ITEMS - 1)
        );
    }

    #[test]
    fn priority_parse_letters() {
        assert_eq!(Priority::parse("A"), Some(Priority::A));
        assert_eq!(Priority::parse("B"), Some(Priority::B));
        assert_eq!(Priority::parse("C"), Some(Priority::C));
        assert_eq!(Priority::parse("Z"), Some(Priority::Other('Z')));
    }

    #[test]
    fn priority_parse_numeric() {
        assert_eq!(Priority::parse("0"), Some(Priority::Numeric(0)));
        assert_eq!(Priority::parse("1"), Some(Priority::Numeric(1)));
        assert_eq!(Priority::parse("9"), Some(Priority::Numeric(9)));
        assert_eq!(Priority::parse("15"), Some(Priority::Numeric(15)));
        assert_eq!(Priority::parse("64"), Some(Priority::Numeric(64)));
    }

    #[test]
    fn priority_parse_rejects_out_of_range() {
        assert_eq!(Priority::parse("65"), None);
        assert_eq!(Priority::parse("100"), None);
        assert_eq!(Priority::parse("a"), None);
        assert_eq!(Priority::parse("@"), None);
        assert_eq!(Priority::parse("-1"), None);
        assert_eq!(Priority::parse(""), None);
    }

    #[test]
    fn priority_parse_rejects_leading_zero() {
        // Matches org-mode's regex grammar: "01" is not a valid priority value.
        assert_eq!(Priority::parse("01"), None);
        assert_eq!(Priority::parse("00"), None);
    }

    #[test]
    fn priority_order_matches_org_priority_to_value() {
        // Numeric values map to themselves; letters to ASCII code.
        assert_eq!(Priority::Numeric(0).order(), 0);
        assert_eq!(Priority::Numeric(64).order(), 64);
        assert_eq!(Priority::A.order(), 65);
        assert_eq!(Priority::B.order(), 66);
        assert_eq!(Priority::C.order(), 67);
        assert_eq!(Priority::Other('D').order(), 68);
        assert_eq!(Priority::Other('Z').order(), 90);
        // Sorting must reflect priority: numeric below 65 outranks A.
        assert!(Priority::Numeric(64).order() < Priority::A.order());
        assert!(Priority::A.order() < Priority::B.order());
        assert!(Priority::C.order() < Priority::Other('D').order());
    }

    #[test]
    fn priority_serializes_as_string() {
        let json = serde_json::to_string(&Priority::A).unwrap();
        assert_eq!(json, "\"A\"");
        let json = serde_json::to_string(&Priority::Other('D')).unwrap();
        assert_eq!(json, "\"D\"");
        let json = serde_json::to_string(&Priority::Numeric(5)).unwrap();
        assert_eq!(json, "\"5\"");
        let json = serde_json::to_string(&Priority::Numeric(64)).unwrap();
        assert_eq!(json, "\"64\"");
    }

    // ADR-0014: `timestamp_active` is the JSON marker for the bracket form
    // (`true` = `<...>`, `false` = `[...]`). The field is `Option<bool>` so
    // tasks without a timestamp omit it entirely (per ADR-0015's rule that
    // unset Option fields skip serialisation, keeping the addition
    // non-breaking for existing consumers).

    fn empty_task() -> Task {
        Task {
            file: "t.md".into(),
            line: 1,
            ..Task::default()
        }
    }

    #[test]
    fn task_serializes_timestamp_active_true_when_active() {
        let mut t = empty_task();
        t.timestamp = Some("SCHEDULED: <2026-05-25 Mon>".into());
        t.timestamp_type = Some("SCHEDULED".into());
        t.timestamp_active = Some(true);
        let json = serde_json::to_string(&t).unwrap();
        assert!(
            json.contains("\"timestamp_active\":true"),
            "JSON must surface timestamp_active=true: {json}"
        );
    }

    #[test]
    fn task_serializes_timestamp_active_false_when_inactive() {
        let mut t = empty_task();
        t.timestamp = Some("CLOSED: [2026-05-24 Sun 14:30]".into());
        t.timestamp_type = Some("CLOSED".into());
        t.timestamp_active = Some(false);
        let json = serde_json::to_string(&t).unwrap();
        assert!(
            json.contains("\"timestamp_active\":false"),
            "JSON must surface timestamp_active=false: {json}"
        );
    }

    #[test]
    fn task_omits_timestamp_active_when_none() {
        // Per ADR-0015, missing optional fields stay out of JSON entirely
        // so the addition is non-breaking for old consumers.
        let json = serde_json::to_string(&empty_task()).unwrap();
        assert!(
            !json.contains("timestamp_active"),
            "absent timestamp must not emit the field: {json}"
        );
    }

    #[test]
    fn task_round_trips_timestamp_active_through_serde() {
        let mut t = empty_task();
        t.timestamp_active = Some(false);
        let json = serde_json::to_string(&t).unwrap();
        let back: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(back.timestamp_active, Some(false));
    }

    #[test]
    fn task_serializes_properties_when_present() {
        let mut t = empty_task();
        let mut props = std::collections::BTreeMap::new();
        props.insert("GCAL_EVENT_ID".to_string(), "abc123/primary".to_string());
        t.properties = Some(props);
        let json = serde_json::to_string(&t).unwrap();
        assert!(
            json.contains("\"properties\":{\"GCAL_EVENT_ID\":\"abc123/primary\"}"),
            "JSON must surface properties map: {json}"
        );
    }

    #[test]
    fn task_omits_properties_when_none() {
        let json = serde_json::to_string(&empty_task()).unwrap();
        assert!(
            !json.contains("properties"),
            "absent properties must not emit the field: {json}"
        );
    }

    #[test]
    fn task_round_trips_properties_through_serde() {
        let mut t = empty_task();
        let mut props = std::collections::BTreeMap::new();
        props.insert("K".to_string(), "v".to_string());
        t.properties = Some(props.clone());
        let json = serde_json::to_string(&t).unwrap();
        let back: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(back.properties, Some(props));
    }

    #[test]
    fn priority_deserializes_from_string_and_integer() {
        let p: Priority = serde_json::from_str("\"A\"").unwrap();
        assert_eq!(p, Priority::A);
        let p: Priority = serde_json::from_str("\"5\"").unwrap();
        assert_eq!(p, Priority::Numeric(5));
        let p: Priority = serde_json::from_str("5").unwrap();
        assert_eq!(p, Priority::Numeric(5));
        // Out of range fails.
        let r: Result<Priority, _> = serde_json::from_str("65");
        assert!(r.is_err());
    }
}
