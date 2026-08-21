//! Exceptions to a repeating entry: an occurrence that is gone, and one that
//! moved.
//!
//! A repeating timestamp describes an endless series and has nowhere to say
//! that one of its occurrences is different. ADR-0031 answers that in the
//! shape iCalendar settled on, written with the `org-properties` keys of
//! ADR-0020:
//!
//! - `EXDATE` on the series lists occurrences the series does not have;
//! - a separate entry carrying `SERIES_ID` and `RECURRENCE_ID` replaces the
//!   one occurrence it names, and needs no `EXDATE` beside it — that is RFC
//!   5545's split between an occurrence that is gone and one that moved.
//!
//! Matching is at day granularity, because the agenda draws at most one
//! occurrence of a series per day; the clock time a `RECURRENCE_ID` may carry
//! is kept for the reader and for export, and is not matched on.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;

use crate::types::Task;

/// Property key listing the occurrences a series does not have.
pub const EXDATE_KEY: &str = "EXDATE";
/// Property key naming the occurrence an entry replaces.
pub const RECURRENCE_ID_KEY: &str = "RECURRENCE_ID";
/// Property key naming the series an entry replaces an occurrence of.
pub const SERIES_ID_KEY: &str = "SERIES_ID";
/// Property key holding a task's own stable identifier (ADR-0020).
pub const ID_KEY: &str = "ID";

/// The dates listed in an `EXDATE` value, normalised to `YYYY-MM-DD`.
///
/// Separators are commas and whitespace, in any mix — a list is written for a
/// person to read, and both are what people write. A field that does not
/// parse as a date is dropped rather than guessed at; the caller reports it.
/// The result keeps the order the value was written in and drops duplicates.
pub fn parse_excluded_dates(raw: &str) -> (Vec<String>, Vec<String>) {
    let mut dates = Vec::new();
    let mut rejected = Vec::new();
    for field in raw.split([',', ' ', '\t']).filter(|f| !f.is_empty()) {
        match NaiveDate::parse_from_str(field, "%Y-%m-%d") {
            Ok(date) => {
                let text = date.format("%Y-%m-%d").to_string();
                if !dates.contains(&text) {
                    dates.push(text);
                }
            }
            Err(_) => rejected.push(field.to_string()),
        }
    }
    (dates, rejected)
}

/// The occurrence a `RECURRENCE_ID` value names: a date, optionally followed
/// by a clock time.
///
/// Returns the value normalised (`YYYY-MM-DD` or `YYYY-MM-DD HH:MM`), or
/// `None` when the date does not parse. A trailing field that is not a time
/// is dropped and the date kept: the date is what the resolver matches on,
/// and losing the exception over a stray word would be the worse failure.
pub fn parse_recurrence_id(raw: &str) -> Option<String> {
    let mut fields = raw.split_whitespace();
    let date = NaiveDate::parse_from_str(fields.next()?, "%Y-%m-%d").ok()?;
    let time = fields
        .next()
        .and_then(|t| chrono::NaiveTime::parse_from_str(t, "%H:%M").ok());
    Some(match time {
        Some(t) => format!("{} {}", date.format("%Y-%m-%d"), t.format("%H:%M")),
        None => date.format("%Y-%m-%d").to_string(),
    })
}

/// The date half of a `RECURRENCE_ID`, which is what occurrences match on.
pub fn recurrence_id_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.split_whitespace().next()?, "%Y-%m-%d").ok()
}

/// Which occurrences of which series are not there, for one run.
///
/// Built from the whole task list because a replacement lives in an entry of
/// its own — possibly in another file of the same scan. An exception
/// therefore reaches only as far as the scan does, which ADR-0031 states as a
/// consequence.
#[derive(Debug, Default, Clone)]
pub struct OccurrenceExceptions {
    replaced: HashMap<String, HashSet<NaiveDate>>,
}

impl OccurrenceExceptions {
    /// Collect every `(SERIES_ID, RECURRENCE_ID)` pair in the run.
    pub fn from_tasks(tasks: &[Task]) -> Self {
        let mut replaced: HashMap<String, HashSet<NaiveDate>> = HashMap::new();
        for task in tasks {
            let (Some(series), Some(recurrence)) =
                (task.series_id.as_deref(), task.recurrence_id.as_deref())
            else {
                continue;
            };
            if let Some(date) = recurrence_id_date(recurrence) {
                replaced.entry(series.to_string()).or_default().insert(date);
            }
        }
        Self { replaced }
    }

    /// Whether `task` skips its occurrence on `date`.
    ///
    /// Two reasons, and the caller needs neither to tell them apart: the
    /// entry lists the date in its own `EXDATE`, or some other entry of the
    /// run replaces that occurrence.
    pub fn excludes(&self, task: &Task, date: NaiveDate) -> bool {
        if let Some(dates) = task.excluded_dates.as_deref() {
            let text = date.format("%Y-%m-%d").to_string();
            if dates.iter().any(|d| d == &text) {
                return true;
            }
        }
        self.task_id(task)
            .and_then(|id| self.replaced.get(id))
            .is_some_and(|dates| dates.contains(&date))
    }

    /// Every occurrence `task` does not have: what it excluded itself, and
    /// what other entries of the run replace.
    ///
    /// Collected per task once, so the day-by-day walk of a week or a month
    /// answers from a set instead of re-reading properties on every cell.
    pub fn dates_for(&self, task: &Task) -> HashSet<NaiveDate> {
        let mut dates: HashSet<NaiveDate> = task
            .excluded_dates
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .collect();
        if let Some(replaced) = self.task_id(task).and_then(|id| self.replaced.get(id)) {
            dates.extend(replaced.iter().copied());
        }
        dates
    }

    /// Whether the run holds any exception at all — the fast path for the
    /// overwhelmingly common case of a corpus without one.
    pub fn is_empty(&self) -> bool {
        self.replaced.is_empty()
    }

    fn task_id<'a>(&self, task: &'a Task) -> Option<&'a str> {
        task.properties.as_ref()?.get(ID_KEY).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn series(id: &str) -> Task {
        let mut props = BTreeMap::new();
        props.insert(ID_KEY.to_string(), id.to_string());
        Task {
            properties: Some(props),
            ..Task::default()
        }
    }

    fn replacement(series_id: &str, recurrence: &str) -> Task {
        Task {
            series_id: Some(series_id.to_string()),
            recurrence_id: Some(recurrence.to_string()),
            ..Task::default()
        }
    }

    #[test]
    fn excluded_dates_take_commas_and_spaces_alike() {
        let (dates, rejected) = parse_excluded_dates("2026-08-20, 2026-08-27 2026-09-03");
        assert_eq!(dates, ["2026-08-20", "2026-08-27", "2026-09-03"]);
        assert!(rejected.is_empty(), "nothing to reject: {rejected:?}");
    }

    #[test]
    fn excluded_dates_drop_what_is_not_a_date_and_say_so() {
        let (dates, rejected) = parse_excluded_dates("2026-08-20, next thursday");
        assert_eq!(dates, ["2026-08-20"]);
        assert_eq!(rejected, ["next", "thursday"]);
    }

    #[test]
    fn excluded_dates_keep_one_copy_of_a_repeated_date() {
        let (dates, _) = parse_excluded_dates("2026-08-20 2026-08-20");
        assert_eq!(dates, ["2026-08-20"]);
    }

    #[test]
    fn a_recurrence_id_keeps_the_time_when_it_carries_one() {
        assert_eq!(
            parse_recurrence_id("2026-08-20 15:00").as_deref(),
            Some("2026-08-20 15:00")
        );
        assert_eq!(
            parse_recurrence_id("2026-08-20").as_deref(),
            Some("2026-08-20")
        );
    }

    #[test]
    fn a_recurrence_id_without_a_date_is_no_recurrence_id() {
        assert_eq!(parse_recurrence_id("thursday 15:00"), None);
    }

    #[test]
    fn a_trailing_field_that_is_not_a_time_leaves_the_date_standing() {
        assert_eq!(
            parse_recurrence_id("2026-08-20 afternoon").as_deref(),
            Some("2026-08-20")
        );
    }

    #[test]
    fn an_entry_skips_the_date_it_lists_itself() {
        let task = Task {
            excluded_dates: Some(vec!["2026-08-20".to_string()]),
            ..Task::default()
        };
        let exceptions = OccurrenceExceptions::from_tasks(std::slice::from_ref(&task));

        assert!(exceptions.excludes(&task, ymd(2026, 8, 20)));
        assert!(!exceptions.excludes(&task, ymd(2026, 8, 27)));
    }

    #[test]
    fn a_replacement_suppresses_the_occurrence_it_names() {
        let english = series("series-1");
        let moved = replacement("series-1", "2026-08-20 15:00");
        let exceptions = OccurrenceExceptions::from_tasks(&[english.clone(), moved]);

        assert!(exceptions.excludes(&english, ymd(2026, 8, 20)));
        assert!(!exceptions.excludes(&english, ymd(2026, 8, 27)));
    }

    #[test]
    fn a_replacement_of_another_series_leaves_this_one_alone() {
        let english = series("series-1");
        let moved = replacement("series-2", "2026-08-20");
        let exceptions = OccurrenceExceptions::from_tasks(&[english.clone(), moved]);

        assert!(!exceptions.excludes(&english, ymd(2026, 8, 20)));
    }

    #[test]
    fn a_series_without_an_id_cannot_be_replaced() {
        let anonymous = Task::default();
        let moved = replacement("series-1", "2026-08-20");
        let exceptions = OccurrenceExceptions::from_tasks(&[anonymous.clone(), moved]);

        assert!(!exceptions.excludes(&anonymous, ymd(2026, 8, 20)));
    }
}
