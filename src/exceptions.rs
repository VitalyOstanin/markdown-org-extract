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
//! The two reasons are kept apart all the way to the agenda, because they part
//! ways over a debt: nothing is owed for an occurrence that never was, and
//! what is owed for one that moved is owed by the entry it moved to.
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
/// person to read, and both are what people write. The result keeps the order
/// the value was written in and holds one entry per date, whichever way the
/// value spelled it.
///
/// A time right after a date is that date's time, not another field: RFC 5545
/// writes an `EXDATE` of a timed series that way, and so does a calendar
/// export. It is read and left out — occurrences are matched by day here
/// (ADR-0031) — while a time with no date before it is a field like any
/// other and is rejected.
///
/// A field that does not parse is handed to `on_rejected` as it is met rather
/// than collected: the value is only as short as the file makes it, and one
/// written entirely of rubbish would otherwise be held twice over — once in
/// the file, once in a vector — for a caller that reports the first few and
/// drops the rest.
pub fn parse_excluded_dates(raw: &str, mut on_rejected: impl FnMut(&str)) -> Vec<String> {
    let mut dates = Vec::new();
    // A set of what has been seen, rather than a scan of what has been kept:
    // the scan is linear per date and so quadratic over the value, which on a
    // long `EXDATE` is the difference between a pass and a stall.
    let mut seen = HashSet::new();
    let mut after_date = false;
    for field in raw.split([',', ' ', '\t']).filter(|f| !f.is_empty()) {
        match NaiveDate::parse_from_str(field, "%Y-%m-%d") {
            Ok(date) => {
                after_date = true;
                if seen.insert(date) {
                    dates.push(date.format("%Y-%m-%d").to_string());
                }
            }
            Err(_) => {
                let is_time_of_that_date = after_date && parse_clock(field).is_some();
                after_date = false;
                if !is_time_of_that_date {
                    on_rejected(field);
                }
            }
        }
    }
    dates
}

/// The occurrence a `RECURRENCE_ID` value names: a date, optionally followed
/// by a clock time.
///
/// Returns the value normalised (`YYYY-MM-DD` or `YYYY-MM-DD HH:MM`), or
/// `None` when the date does not parse. Anything after the date that is not a
/// time is dropped and the date kept: the date is what the resolver matches
/// on, and losing the exception over a stray word would be the worse failure.
/// What was dropped is handed to `on_dropped` rather than passed over in
/// silence -- it is text the file wrote and this value no longer carries, and
/// an export built from the field will not carry it either.
pub fn parse_recurrence_id(raw: &str, mut on_dropped: impl FnMut(&str)) -> Option<String> {
    let mut fields = raw.split_whitespace();
    let date = NaiveDate::parse_from_str(fields.next()?, "%Y-%m-%d").ok()?;
    let rest: Vec<&str> = fields.collect();
    let time = rest.first().copied().and_then(parse_clock);
    let dropped = if time.is_some() {
        &rest[1..]
    } else {
        &rest[..]
    };
    if !dropped.is_empty() {
        on_dropped(&dropped.join(" "));
    }
    Some(match time {
        Some(t) => format!("{} {}", date.format("%Y-%m-%d"), t.format("%H:%M")),
        None => date.format("%Y-%m-%d").to_string(),
    })
}

/// The clock time of a `RECURRENCE_ID`: written to the minute, or with the
/// seconds a calendar export adds. Occurrences are named to the minute here,
/// so the seconds are read and then left out of the normalised value.
fn parse_clock(field: &str) -> Option<chrono::NaiveTime> {
    chrono::NaiveTime::parse_from_str(field, "%H:%M")
        .or_else(|_| chrono::NaiveTime::parse_from_str(field, "%H:%M:%S"))
        .ok()
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
    unknown_series: Vec<String>,
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
        // A `SERIES_ID` nothing answers to suppresses nothing, and the day
        // ends up holding both the series occurrence and the entry that meant
        // to stand in for it. Both entries are in this run, so the mismatch is
        // decidable right here -- unlike the one exception ADR-0031 allows to
        // pass in silence, where the replacement is in a file the scan never
        // reached.
        let known: HashSet<&str> = tasks.iter().filter_map(task_id).collect();
        let mut unknown_series: Vec<String> = replaced
            .keys()
            .filter(|id| !known.contains(id.as_str()))
            .cloned()
            .collect();
        // Sorted so a run reports them the same way twice over.
        unknown_series.sort();
        Self {
            replaced,
            unknown_series,
        }
    }

    /// The `SERIES_ID` values of this run that name no entry in it.
    ///
    /// Kept rather than reported here: this is built once per pass over the
    /// task list, and the caller that draws the agenda is the one place that
    /// can say it once per run.
    pub fn unknown_series(&self) -> &[String] {
        &self.unknown_series
    }

    /// Every occurrence `task` does not have: what it cancelled itself, and
    /// what other entries of the run replace.
    ///
    /// The one place that answers the question, and it answers it once per
    /// task: the day-by-day walk of a week or a month reads a set instead of
    /// re-reading properties on every cell.
    pub fn dates_for(&self, task: &Task) -> ExcludedOccurrences {
        let cancelled = task
            .excluded_dates
            .as_deref()
            .unwrap_or_default()
            .iter()
            // A date nothing can read is dropped here as it was dropped at
            // the parser: a `Task` can also be built by a library caller,
            // and one bad string must not take the whole list with it.
            .filter_map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .collect();
        let replaced = task_id(task)
            .and_then(|id| self.replaced.get(id))
            .cloned()
            .unwrap_or_default();
        ExcludedOccurrences {
            cancelled,
            replaced,
        }
    }
}

/// A task's own identifier, the one a `SERIES_ID` names (ADR-0020).
fn task_id(task: &Task) -> Option<&str> {
    task.properties.as_ref()?.get(ID_KEY).map(String::as_str)
}

/// The occurrences one entry does not have, kept apart by reason.
///
/// Both reasons take the occurrence out of the day it would have fallen on.
/// They part ways over the arrears: a cancelled occurrence never was, so the
/// debt is whichever earlier one still stands, while a replaced occurrence did
/// take place — elsewhere — and its debt travels with the entry that replaced
/// it (ADR-0031).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExcludedOccurrences {
    cancelled: HashSet<NaiveDate>,
    replaced: HashSet<NaiveDate>,
}

impl ExcludedOccurrences {
    /// Whether the series skips `date`, for either reason.
    pub fn contains(&self, date: &NaiveDate) -> bool {
        self.cancelled.contains(date) || self.replaced.contains(date)
    }

    /// Whether another entry of the run stands in for the occurrence on
    /// `date`.
    ///
    /// Asked where the two reasons differ, which is the arrears bucket. A
    /// date named by both is treated as replaced: the occurrence is somewhere,
    /// and an `EXDATE` beside a replacement is redundant rather than
    /// contradictory.
    pub fn is_replaced(&self, date: &NaiveDate) -> bool {
        self.replaced.contains(date)
    }

    /// Whether this entry misses no occurrence at all — the fast path for the
    /// overwhelmingly common case of an entry without an exception.
    pub fn is_empty(&self) -> bool {
        self.cancelled.is_empty() && self.replaced.is_empty()
    }

    /// How many occurrences are missing, counting a date named by both
    /// reasons twice. An upper bound is all the walks over a series need, and
    /// an exact count would cost a pass over the smaller set.
    pub fn len(&self) -> usize {
        self.cancelled.len() + self.replaced.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// The dates of an `EXDATE` value, for a test that expects all of them to
    /// read.
    fn dates_of(raw: &str) -> Vec<String> {
        parse_excluded_dates(raw, |field| panic!("unexpected reject: {field:?}"))
    }

    /// The occurrence a `RECURRENCE_ID` names, for a test that expects the
    /// whole value to read.
    fn occurrence_of(raw: &str) -> Option<String> {
        parse_recurrence_id(raw, |dropped| panic!("unexpected drop: {dropped:?}"))
    }

    fn series(id: &str) -> Task {
        let mut props = BTreeMap::new();
        props.insert(ID_KEY.to_string(), id.to_string());
        Task {
            properties: Some(props),
            ..Task::default()
        }
    }

    fn cancelling(dates: &[&str]) -> Task {
        Task {
            excluded_dates: Some(dates.iter().map(|d| (*d).to_string()).collect()),
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
        assert_eq!(
            dates_of("2026-08-20, 2026-08-27 2026-09-03"),
            ["2026-08-20", "2026-08-27", "2026-09-03"]
        );
    }

    #[test]
    fn excluded_dates_drop_what_is_not_a_date_and_say_so() {
        let mut rejected = Vec::new();
        let dates = parse_excluded_dates("2026-08-20, next thursday", |field| {
            rejected.push(field.to_string());
        });

        assert_eq!(dates, ["2026-08-20"]);
        assert_eq!(
            rejected,
            ["next", "thursday"],
            "each field is reported as it is met"
        );
    }

    #[test]
    fn a_time_after_a_date_belongs_to_that_date() {
        // The form RFC 5545 uses for a timed series, and the one a calendar
        // export writes. The day is what an occurrence is matched on, so the
        // time is read and left out -- and not reported as a field nothing
        // can read, which is what a correct value would otherwise be called.
        assert_eq!(
            dates_of("2026-08-20 15:00, 2026-08-27 15:00:00"),
            ["2026-08-20", "2026-08-27"]
        );
    }

    #[test]
    fn a_time_with_no_date_before_it_is_a_field_like_any_other() {
        let mut rejected = Vec::new();
        let dates = parse_excluded_dates("15:00, 2026-08-20 15:00 16:00", |field| {
            rejected.push(field.to_string());
        });

        assert_eq!(dates, ["2026-08-20"]);
        assert_eq!(
            rejected,
            ["15:00", "16:00"],
            "one time belongs to the date before it; a second one belongs to nothing"
        );
    }

    #[test]
    fn excluded_dates_keep_one_copy_of_a_repeated_date() {
        assert_eq!(dates_of("2026-08-20 2026-08-20"), ["2026-08-20"]);
    }

    #[test]
    fn excluded_dates_keep_one_copy_however_the_date_was_spelled() {
        assert_eq!(dates_of("2026-8-20, 2026-08-20"), ["2026-08-20"]);
    }

    #[test]
    fn a_long_exdate_costs_one_pass_and_not_one_per_date_already_seen() {
        // A value is only as short as the file makes it, and a linear scan of
        // what is already collected turns that length into its square: 20 000
        // dates are 2*10^8 string comparisons, seconds of a test run, and on a
        // file of the size the scanner accepts, an entry nothing finishes
        // reading.
        const DATES: i64 = 20_000;
        let first = ymd(2000, 1, 1);
        let raw = (0..DATES)
            .map(|i| {
                (first + chrono::Duration::days(i))
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(", ");

        let dates = dates_of(&raw);

        assert_eq!(dates.len(), DATES as usize, "every date is kept, once");
        assert_eq!(dates[0], "2000-01-01", "in the order it was written");
    }

    #[test]
    fn a_recurrence_id_keeps_the_time_when_it_carries_one() {
        assert_eq!(
            occurrence_of("2026-08-20 15:00").as_deref(),
            Some("2026-08-20 15:00")
        );
        assert_eq!(occurrence_of("2026-08-20").as_deref(), Some("2026-08-20"));
    }

    #[test]
    fn a_recurrence_id_without_a_date_is_no_recurrence_id() {
        assert_eq!(parse_recurrence_id("thursday 15:00", |_| {}), None);
    }

    #[test]
    fn a_trailing_field_that_is_not_a_time_leaves_the_date_standing_and_is_told() {
        let mut dropped = Vec::new();
        let occurrence = parse_recurrence_id("2026-08-20 afternoon", |text| {
            dropped.push(text.to_string());
        });

        assert_eq!(occurrence.as_deref(), Some("2026-08-20"));
        assert_eq!(
            dropped,
            ["afternoon"],
            "the text the value no longer carries is named"
        );
    }

    #[test]
    fn a_recurrence_id_written_with_seconds_keeps_the_time_it_names() {
        // The form a calendar export writes. Occurrences are named to the
        // minute here, so the seconds go and the time stays.
        assert_eq!(
            occurrence_of("2026-08-20 15:00:00").as_deref(),
            Some("2026-08-20 15:00")
        );
    }

    #[test]
    fn whatever_follows_the_time_is_dropped_and_named() {
        let mut dropped = Vec::new();
        let occurrence = parse_recurrence_id("2026-08-20 15:00 sharp", |text| {
            dropped.push(text.to_string());
        });

        assert_eq!(occurrence.as_deref(), Some("2026-08-20 15:00"));
        assert_eq!(dropped, ["sharp"]);
    }

    #[test]
    fn a_replacement_that_names_no_series_of_the_run_is_reported() {
        // A typo in the identifier leaves both entries standing on the day,
        // which is exactly what an exception is written to avoid. Both are in
        // this run, so the mismatch is decidable and is not one of the silences
        // ADR-0031 allows.
        let english = series("series-1");
        let moved = replacement("seires-1", "2026-08-20");

        let exceptions = OccurrenceExceptions::from_tasks(&[english.clone(), moved]);

        assert_eq!(exceptions.unknown_series(), ["seires-1".to_string()]);
        assert!(
            exceptions.dates_for(&english).is_empty(),
            "and nothing is suppressed, which is what the report is about"
        );
    }

    #[test]
    fn a_replacement_naming_a_series_of_the_run_is_not_reported() {
        let english = series("series-1");
        let moved = replacement("series-1", "2026-08-20");

        let exceptions = OccurrenceExceptions::from_tasks(&[english, moved]);

        assert!(exceptions.unknown_series().is_empty());
    }

    #[test]
    fn an_entry_skips_the_date_it_lists_itself() {
        let task = cancelling(&["2026-08-20"]);
        let missing =
            OccurrenceExceptions::from_tasks(std::slice::from_ref(&task)).dates_for(&task);

        assert!(missing.contains(&ymd(2026, 8, 20)));
        assert!(!missing.contains(&ymd(2026, 8, 27)));
        assert!(
            !missing.is_replaced(&ymd(2026, 8, 20)),
            "an EXDATE cancels an occurrence, it does not move it"
        );
    }

    #[test]
    fn a_date_in_an_exdate_that_cannot_be_read_is_dropped_and_the_rest_kept() {
        // Reachable through the library, where a `Task` is built by hand and
        // not by the parser that normalises what it writes.
        let task = cancelling(&["last thursday", "2026-08-27"]);
        let missing =
            OccurrenceExceptions::from_tasks(std::slice::from_ref(&task)).dates_for(&task);

        assert!(missing.contains(&ymd(2026, 8, 27)));
        assert_eq!(missing.len(), 1);
    }

    #[test]
    fn a_replacement_suppresses_the_occurrence_it_names() {
        let english = series("series-1");
        let moved = replacement("series-1", "2026-08-20 15:00");
        let missing =
            OccurrenceExceptions::from_tasks(&[english.clone(), moved]).dates_for(&english);

        assert!(missing.contains(&ymd(2026, 8, 20)));
        assert!(!missing.contains(&ymd(2026, 8, 27)));
        assert!(
            missing.is_replaced(&ymd(2026, 8, 20)),
            "the occurrence moved: its debt is the replacement's"
        );
    }

    #[test]
    fn both_reasons_meet_in_one_answer_and_stay_apart_in_it() {
        let mut english = series("series-1");
        english.excluded_dates = Some(vec!["2026-08-13".to_string()]);
        let moved = replacement("series-1", "2026-08-20 15:00");
        let missing =
            OccurrenceExceptions::from_tasks(&[english.clone(), moved]).dates_for(&english);

        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&ymd(2026, 8, 13)) && missing.contains(&ymd(2026, 8, 20)));
        assert!(!missing.is_replaced(&ymd(2026, 8, 13)), "the 13th is gone");
        assert!(missing.is_replaced(&ymd(2026, 8, 20)), "the 20th moved");
    }

    #[test]
    fn a_replacement_of_another_series_leaves_this_one_alone() {
        let english = series("series-1");
        let moved = replacement("series-2", "2026-08-20");
        let missing =
            OccurrenceExceptions::from_tasks(&[english.clone(), moved]).dates_for(&english);

        assert!(missing.is_empty());
    }

    #[test]
    fn a_series_without_an_id_cannot_be_replaced() {
        let anonymous = Task::default();
        let moved = replacement("series-1", "2026-08-20");
        let missing =
            OccurrenceExceptions::from_tasks(&[anonymous.clone(), moved]).dates_for(&anonymous);

        assert!(missing.is_empty());
    }

    #[test]
    fn an_entry_whose_only_exception_is_an_exdate_is_not_an_entry_without_any() {
        let task = cancelling(&["2026-08-20"]);
        let missing =
            OccurrenceExceptions::from_tasks(std::slice::from_ref(&task)).dates_for(&task);

        assert!(
            !missing.is_empty(),
            "an EXDATE is an exception: an entry holding one is not an entry without any"
        );
    }

    #[test]
    fn one_definition_answers_whatever_the_date_is_written_like() {
        // `2026-8-20` is what a person writes and what chrono reads. One
        // definition of "is this occurrence missing" means one answer,
        // whichever way the value spelled the day.
        let task = cancelling(&["2026-8-20"]);
        let missing =
            OccurrenceExceptions::from_tasks(std::slice::from_ref(&task)).dates_for(&task);

        assert!(missing.contains(&ymd(2026, 8, 20)));
    }
}
