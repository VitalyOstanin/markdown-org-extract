use chrono::{Datelike, NaiveDate};
use chrono_tz::Tz;

use crate::error::AppError;
use crate::timestamp::{parse_org_timestamp, ParsedTimestamp};
use crate::types::{DayAgenda, Task, TaskType, TaskWithOffset};

const DEADLINE_WARNING_DAYS: i64 = 14;

/// Sort key used in the `--tasks` flat list for tasks with `priority = None`.
/// `u32::MAX` is strictly greater than every value `Priority::order()` can
/// return (numeric `0..=64`, letters `A..Z` = `65..=90`), so no-priority
/// tasks always sort last regardless of how the priority range evolves.
const NO_PRIORITY_ORDER: u32 = u32::MAX;

/// Task with its timestamp pre-parsed once, to avoid re-parsing on every day
/// of a week/month agenda.
struct PreparedTask<'a> {
    task: &'a Task,
    parsed: Option<ParsedTimestamp>,
}

fn prepare_tasks(tasks: &[Task]) -> Vec<PreparedTask<'_>> {
    // ADR-0014 invariant: inactive `[...]` timestamps never feed the
    // agenda. Filtering at the parse step keeps the rest of the agenda
    // logic bracket-form-agnostic — every downstream bucket already
    // skips entries whose `parsed` is `None`. SCHEDULED/DEADLINE are
    // guaranteed active by the extract-layer regex (ADR-0014), CLOSED
    // is guaranteed inactive there and was already excluded from
    // overdue/upcoming by `handle_repeating_task`; the only case this
    // filter actually drops is a PLAIN inline `[YYYY-MM-DD ...]`.
    tasks
        .iter()
        .map(|t| PreparedTask {
            task: t,
            parsed: t
                .timestamp
                .as_deref()
                .and_then(|ts| parse_org_timestamp(ts, None))
                .filter(|p| p.active),
        })
        .collect()
}

/// Result of running [`filter_agenda`]. The variant is determined by the
/// requested [`AgendaScope`]:
///
/// - [`AgendaScope::Day`] / [`AgendaScope::Week`] / [`AgendaScope::Month`]
///   produce [`AgendaOutput::Days`] — one [`DayAgenda`] per day in the
///   window, each carrying overdue / scheduled / upcoming buckets.
/// - [`AgendaScope::Tasks`] produces [`AgendaOutput::Tasks`] — a single
///   flat list filtered to actionable items, with no date bucketing.
///
/// The renderer in [`crate::render`] dispatches on this enum to choose
/// between the per-day agenda layout and the flat list layout.
#[derive(Debug)]
pub enum AgendaOutput {
    /// Per-day agenda for day / week / month scope.
    Days(Vec<DayAgenda>),
    /// Flat task list for `--agenda tasks` / `--tasks` scope.
    Tasks(Vec<Task>),
}

/// Effective agenda scope after resolving CLI flags. `Tasks` is selected via
/// `--tasks` instead of `--agenda`; the other three correspond directly to
/// `--agenda day|week|month`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgendaScope {
    Day,
    Week,
    Month,
    Tasks,
}

/// The CLI date-window arguments for [`filter_agenda`], grouped into one
/// value so the function signature stays within a sane arity. Each field is
/// the raw `Option<&str>` from the corresponding CLI flag; their interplay
/// (priority, edge filling, `Tasks`-scope rejection) is the unified
/// date-window model described in
/// [ADR-0009](../docs/adr/0009-unified-date-window-semantics.md).
#[derive(Debug, Default, Clone, Copy)]
pub struct AgendaDates<'a> {
    /// Value of `--date`. Selects the window's pivot day; in `Day` scope this
    /// is the only day, in `Week` / `Month` scope it picks the containing
    /// week / month. Ignored if `from`/`to` is set. Rejected under `Tasks`
    /// scope.
    pub date: Option<&'a str>,
    /// Value of `--from`. A single edge is filled from `current_date` (or
    /// today). `from > to` returns `AppError::DateRange`.
    pub from: Option<&'a str>,
    /// Value of `--to`. A single edge is filled from `current_date` (or
    /// today).
    pub to: Option<&'a str>,
    /// Value of `--current-date`. Overrides the notion of "today" for
    /// deterministic testing and for rendering the agenda as it would look on
    /// a different day. Also the default for a missing `--from`/`--to` edge.
    pub current_date: Option<&'a str>,
}

fn parse_date_arg(label: &str, value: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|e| AppError::InvalidDate(format!("{label} '{value}': {e}")))
}

/// Convert a UTC instant into the calendar date as seen in `tz`. Factored out
/// from `filter_agenda` so it can be unit-tested with an explicit "now":
/// dropping `.with_timezone(&tz)` would silently produce UTC-relative dates,
/// which only deviates from local dates near midnight — exactly the case a
/// developer would not notice in casual testing. The regression guard for
/// that mistake lives in this module's tests.
fn compute_today_in_tz(now_utc: chrono::DateTime<chrono::Utc>, tz: Tz) -> NaiveDate {
    now_utc.with_timezone(&tz).date_naive()
}

/// Resolve `--from`/`--to` into a `[start, end]` date range, filling a missing
/// edge from `current_date` (today or `--current-date`).
///
/// Returns:
///
/// - `Ok(Some((from, to)))` when at least one of `--from` / `--to` was given.
/// - `Ok(None)` when neither was given, so the caller can fall back to a
///   `--date`-derived window or the current period.
/// - `Err(AppError::DateRange)` when the resulting range is inverted
///   (`from > to`).
///
/// See [ADR-0009](../docs/adr/0009-unified-date-window-semantics.md) for the
/// full model.
fn parse_range(
    from: Option<&str>,
    to: Option<&str>,
    current_date: NaiveDate,
) -> Result<Option<(NaiveDate, NaiveDate)>, AppError> {
    let from_date = from.map(|s| parse_date_arg("from", s)).transpose()?;
    let to_date = to.map(|s| parse_date_arg("to", s)).transpose()?;
    let (start, end) = match (from_date, to_date) {
        (None, None) => return Ok(None),
        (Some(f), Some(t)) => (f, t),
        (Some(f), None) => (f, current_date),
        (None, Some(t)) => (current_date, t),
    };
    if start > end {
        return Err(AppError::DateRange(format!(
            "Start date {start} is after end date {end}"
        )));
    }
    Ok(Some((start, end)))
}

/// Filter and bucket the extracted `tasks` according to the agenda
/// configuration on the command line.
///
/// Inputs:
/// - `tasks` — all tasks produced by `parser::extract_tasks`, across
///   every input file.
/// - `scope` — what shape of output to produce; see [`AgendaScope`] /
///   [`AgendaOutput`].
/// - `dates` — the `--date` / `--from` / `--to` / `--current-date`
///   window arguments, grouped in [`AgendaDates`]. See ADR-0009 for the
///   priorities between them and the `Tasks`-scope rejection rule.
/// - `tz` — IANA time zone name used to compute "today" when
///   `dates.current_date` is `None`.
/// - `include_done` — value of `--tasks-include-done`. Only affects
///   [`AgendaScope::Tasks`]: when `true` the flat list additionally
///   surfaces `DONE` tasks (otherwise it is TODO-only, the documented
///   default). A no-op for day / week / month scope, which keep their
///   Org-faithful `DONE` handling (shown on the occurrence day, hidden
///   from overdue / upcoming).
///
/// Errors:
/// - `AppError::InvalidDate` — any of `date`/`from`/`to`/`current-date`
///   failed `YYYY-MM-DD` parse, or `Tasks` scope was combined with
///   date arguments.
/// - `AppError::InvalidTimezone` — `tz` was not recognised by chrono-tz.
/// - `AppError::DateRange` — `from > to` after edge filling.
pub fn filter_agenda(
    tasks: Vec<Task>,
    scope: AgendaScope,
    dates: AgendaDates<'_>,
    tz: &str,
    include_done: bool,
) -> Result<AgendaOutput, AppError> {
    let AgendaDates {
        date,
        from,
        to,
        current_date: current_date_override,
    } = dates;

    let tz: Tz = tz
        .parse()
        .map_err(|_| AppError::InvalidTimezone(tz.to_string()))?;

    let today = match current_date_override {
        Some(date_str) => parse_date_arg("current-date", date_str)?,
        None => compute_today_in_tz(chrono::Utc::now(), tz),
    };

    tracing::debug!(
        scope = ?scope,
        date,
        from,
        to,
        tz = %tz,
        today = %today,
        input_tasks = tasks.len(),
        "filter_agenda input"
    );

    // Tasks scope is task-based, not date-centric -- reject any date argument
    // up-front so a stray `--date 2026-01-01 --agenda tasks` is loud, not
    // silently ignored. See ADR-0009 for the model.
    if scope == AgendaScope::Tasks
        && (date.is_some() || from.is_some() || to.is_some() || current_date_override.is_some())
    {
        return Err(AppError::DateRange(
            "tasks mode does not accept date arguments (--date, --from, --to, --current-date)"
                .to_string(),
        ));
    }

    match scope {
        AgendaScope::Day => {
            // --from/--to: range of day-agendas. Single edge falls back to
            // `today` (current_date or --current-date).
            if let Some((start_date, end_date)) = parse_range(from, to, today)? {
                Ok(AgendaOutput::Days(build_week_agenda(
                    &tasks, start_date, end_date, today,
                )))
            } else {
                let target_date = match date {
                    Some(date_str) => parse_date_arg("date", date_str)?,
                    None => today,
                };
                Ok(AgendaOutput::Days(vec![build_day_agenda(
                    &tasks,
                    target_date,
                    today,
                )]))
            }
        }
        AgendaScope::Week => {
            let (start_date, end_date) = if let Some(range) = parse_range(from, to, today)? {
                range
            } else if let Some(date_str) = date {
                get_week_for_date(parse_date_arg("date", date_str)?)
            } else {
                get_week_for_date(today)
            };

            Ok(AgendaOutput::Days(build_week_agenda(
                &tasks, start_date, end_date, today,
            )))
        }
        AgendaScope::Month => {
            let (start_date, end_date) = if let Some(range) = parse_range(from, to, today)? {
                range
            } else if let Some(date_str) = date {
                get_month_for_date(parse_date_arg("date", date_str)?)
            } else {
                get_month_for_date(today)
            };

            Ok(AgendaOutput::Days(build_week_agenda(
                &tasks, start_date, end_date, today,
            )))
        }
        AgendaScope::Tasks => {
            // Default: TODO only — the documented contract, pinned by the JSON
            // wire-contract snapshot tests and grepped for by existing
            // pipelines. The opt-in `--tasks-include-done` (`include_done`)
            // additionally surfaces `DONE` tasks so a consumer can act on
            // completion (e.g. a calendar sync deleting the event for a
            // finished task). `DONE` is never auto-included.
            let mut filtered: Vec<Task> = tasks
                .into_iter()
                .filter(|t| {
                    matches!(t.task_type, Some(TaskType::Todo))
                        || (include_done && matches!(t.task_type, Some(TaskType::Done)))
                })
                .collect();
            filtered.sort_by_key(|t| {
                t.priority
                    .as_ref()
                    .map(|p| p.order())
                    .unwrap_or(NO_PRIORITY_ORDER)
            });
            Ok(AgendaOutput::Tasks(filtered))
        }
    }
}

fn build_day_agenda(tasks: &[Task], day_date: NaiveDate, current_date: NaiveDate) -> DayAgenda {
    let prepared = prepare_tasks(tasks);
    build_day_agenda_prepared(&prepared, day_date, current_date)
}

fn build_day_agenda_prepared(
    prepared: &[PreparedTask<'_>],
    day_date: NaiveDate,
    current_date: NaiveDate,
) -> DayAgenda {
    let mut agenda = DayAgenda::new(day_date);

    for entry in prepared {
        let task = entry.task;
        if let Some(ref parsed) = entry.parsed {
            if let Some(ref repeater) = parsed.repeater {
                handle_repeating_task(task, parsed, repeater, day_date, current_date, &mut agenda);
            } else {
                handle_non_repeating_task(task, parsed, day_date, current_date, &mut agenda);
            }
        }
    }

    agenda.overdue.sort_by_key(|t| t.days_offset);
    agenda
        .scheduled_timed
        .sort_by(|a, b| a.task.timestamp_time.cmp(&b.task.timestamp_time));
    agenda.upcoming.sort_by_key(|t| t.days_offset);
    // scheduled_no_time has no time-of-day to order by, so it is sorted by
    // priority (high first, mirroring upstream org-agenda's `urgency-down`),
    // then by file path and line as a deterministic tiebreaker. Without this
    // the bucket inherited the walker's filesystem traversal order, which is
    // unspecified and could differ between runs on identical input (m1 in the
    // 2026-05-25 logic review). No-priority tasks sort last, consistent with
    // the `--tasks` flat list.
    agenda.scheduled_no_time.sort_by(|a, b| {
        let pa = a
            .task
            .priority
            .as_ref()
            .map(|p| p.order())
            .unwrap_or(NO_PRIORITY_ORDER);
        let pb = b
            .task
            .priority
            .as_ref()
            .map(|p| p.order())
            .unwrap_or(NO_PRIORITY_ORDER);
        pa.cmp(&pb)
            .then_with(|| a.task.file.cmp(&b.task.file))
            .then_with(|| a.task.line.cmp(&b.task.line))
    });

    agenda
}

fn handle_non_repeating_task(
    task: &Task,
    parsed: &crate::timestamp::ParsedTimestamp,
    day_date: NaiveDate,
    current_date: NaiveDate,
    agenda: &mut DayAgenda,
) {
    let task_date = parsed.date;
    let days_diff = (task_date - day_date).num_days();
    let is_done = matches!(task.task_type, Some(TaskType::Done));
    let is_today = day_date == current_date;

    let days_offset = if days_diff != 0 {
        Some(days_diff)
    } else {
        None
    };

    // Show task on its scheduled date
    if task_date == day_date {
        let task_with_offset = TaskWithOffset {
            task: task.clone(),
            days_offset,
        };
        if task_with_offset.task.timestamp_time.is_some() {
            agenda.scheduled_timed.push(task_with_offset);
        } else {
            agenda.scheduled_no_time.push(task_with_offset);
        }
    } else if days_diff < 0 && is_today && !is_done {
        // Overdue only in today agenda
        agenda
            .overdue
            .push(create_task_without_time(task, days_offset));
    } else if days_diff > 0 && is_today {
        // Upcoming only in today agenda, only for DEADLINE within warning
        // period. A `-N<unit>` cookie on the timestamp overrides the global
        // default (see upstream `org-get-wdays` in lisp/org.el L14937-14943).
        if let Some(ref ts_type) = task.timestamp_type {
            let window = parsed.warning_days.unwrap_or(DEADLINE_WARNING_DAYS);
            if ts_type == "DEADLINE" && days_diff <= window {
                agenda
                    .upcoming
                    .push(create_task_without_time(task, days_offset));
            }
        }
    }
}

fn create_task_without_time(task: &Task, days_offset: Option<i64>) -> TaskWithOffset {
    let mut task_copy = task.clone();
    task_copy.timestamp_time = None;
    task_copy.timestamp_end_time = None;
    TaskWithOffset {
        task: task_copy,
        days_offset,
    }
}

/// Format an org-mode timestamp string with the original repeater type preserved
/// (`+`, `++`, `.+`) and a substituted occurrence date.
fn format_repeating_timestamp(
    ts_type: &str,
    date: NaiveDate,
    time: Option<&str>,
    repeater: &crate::timestamp::Repeater,
) -> String {
    let weekday = date.format("%a");
    let date_str = date.format("%Y-%m-%d");
    let prefix = repeater.repeater_type.prefix();
    let suffix = repeater.unit.suffix();
    match time {
        Some(t) => format!(
            "{ts_type}: <{date_str} {weekday} {t} {prefix}{value}{suffix}>",
            value = repeater.value
        ),
        None => format!(
            "{ts_type}: <{date_str} {weekday} {prefix}{value}{suffix}>",
            value = repeater.value
        ),
    }
}

fn push_scheduled_occurrence(
    task: &Task,
    repeater: &crate::timestamp::Repeater,
    day_date: NaiveDate,
    agenda: &mut DayAgenda,
) {
    let mut task_copy = task.clone();
    task_copy.timestamp_date = Some(day_date.format("%Y-%m-%d").to_string());

    if let Some(ref ts_type) = task.timestamp_type {
        task_copy.timestamp = Some(format_repeating_timestamp(
            ts_type,
            day_date,
            task.timestamp_time.as_deref(),
            repeater,
        ));
    }

    let task_with_offset = TaskWithOffset {
        task: task_copy,
        days_offset: None,
    };

    if task_with_offset.task.timestamp_time.is_some() {
        agenda.scheduled_timed.push(task_with_offset);
    } else {
        agenda.scheduled_no_time.push(task_with_offset);
    }
}

fn push_overdue_occurrence(
    task: &Task,
    repeater: &crate::timestamp::Repeater,
    deadline_date: NaiveDate,
    current_date: NaiveDate,
    agenda: &mut DayAgenda,
) {
    let days_diff = (deadline_date - current_date).num_days();
    let mut task_copy = task.clone();
    task_copy.timestamp_time = None;
    task_copy.timestamp_end_time = None;
    task_copy.timestamp_date = Some(deadline_date.format("%Y-%m-%d").to_string());

    if let Some(ref ts_type) = task.timestamp_type {
        task_copy.timestamp = Some(format_repeating_timestamp(
            ts_type,
            deadline_date,
            None,
            repeater,
        ));
    }

    agenda.overdue.push(TaskWithOffset {
        task: task_copy,
        days_offset: Some(days_diff),
    });
}

fn handle_repeating_task(
    task: &Task,
    parsed: &crate::timestamp::ParsedTimestamp,
    repeater: &crate::timestamp::Repeater,
    day_date: NaiveDate,
    current_date: NaiveDate,
    agenda: &mut DayAgenda,
) {
    use crate::timestamp::{closest_date, DatePreference};

    let base_date = parsed.date;
    let is_today = day_date == current_date;

    let deadline = closest_date(base_date, current_date, DatePreference::Past, repeater);
    // `repeat` is "should this exact day show the recurring task?" — that
    // question is local to `day_date`, not to `current_date`, otherwise past
    // occurrence days in a week/month agenda would be silently empty.
    let repeat = if day_date <= current_date {
        closest_date(base_date, day_date, DatePreference::Past, repeater)
    } else {
        closest_date(base_date, day_date, DatePreference::Future, repeater)
    };

    // Show task on its occurrence day. If base_date is in the future,
    // deadline may be None; in that case use base_date as the first occurrence.
    let mut shown_on_day = false;
    if let Some(repeat_date) = repeat {
        if day_date == repeat_date {
            push_scheduled_occurrence(task, repeater, day_date, agenda);
            shown_on_day = true;
        }
    }
    if !shown_on_day && deadline.is_none() && current_date < base_date && day_date == base_date {
        push_scheduled_occurrence(task, repeater, day_date, agenda);
    }

    // DONE tasks and CLOSED-typed timestamps never appear in overdue or
    // upcoming (mirrors upstream Org-mode org-agenda.el lines 6424-6428 for
    // DONE, and the :closed/:deadline entry-type split at line 5571 for
    // CLOSED). Occurrence-day scheduling above is unaffected; that matches
    // the default of `org-agenda-skip-deadline-if-done` (nil), which still
    // shows the DONE task on its actual deadline date.
    let is_done = matches!(task.task_type, Some(TaskType::Done));
    let is_closed_ts = matches!(task.timestamp_type.as_deref(), Some("CLOSED"));

    if is_today && !is_done && !is_closed_ts {
        // Overdue: requires a past occurrence
        if let Some(deadline_date) = deadline {
            if deadline_date < current_date {
                let should_show_overdue =
                    if repeater.unit == crate::timestamp::RepeaterUnit::Workday {
                        use crate::holidays::HolidayCalendar;
                        HolidayCalendar::global().is_workday(current_date)
                    } else {
                        true
                    };

                if should_show_overdue {
                    push_overdue_occurrence(task, repeater, deadline_date, current_date, agenda);
                }
            }
        }

        // Upcoming: DEADLINE within warning period.
        //
        // `repeat` here is `closest_date(..., DatePreference::Past, ...)` with
        // anchor `day_date == current_date`, so when it is `Some(r)` we know
        // `r <= current_date` — never a future occurrence, never a candidate
        // for the upcoming bucket. The only way a repeating DEADLINE produces
        // an upcoming entry is when there is no past occurrence yet and the
        // base date itself is still ahead of `current_date`.
        if let Some(ref ts_type) = task.timestamp_type {
            if ts_type == "DEADLINE" {
                let next_due = if repeat.is_none() && current_date < base_date {
                    Some(base_date)
                } else {
                    None
                };
                if let Some(next_date) = next_due {
                    let days_diff = (next_date - current_date).num_days();
                    let window = parsed.warning_days.unwrap_or(DEADLINE_WARNING_DAYS);
                    if days_diff > 0 && days_diff <= window {
                        let mut task_copy = task.clone();
                        task_copy.timestamp_time = None;
                        task_copy.timestamp_end_time = None;
                        agenda.upcoming.push(TaskWithOffset {
                            task: task_copy,
                            days_offset: Some(days_diff),
                        });
                    }
                }
            }
        }
    }
}

/// Build agenda for a range of days (week or month). Pre-parses every task's
/// timestamp once and reuses it across all days in the range.
fn build_week_agenda(
    tasks: &[Task],
    start_date: NaiveDate,
    end_date: NaiveDate,
    current_date: NaiveDate,
) -> Vec<DayAgenda> {
    let prepared = prepare_tasks(tasks);
    let mut result = Vec::new();
    let mut current = start_date;

    while current <= end_date {
        result.push(build_day_agenda_prepared(&prepared, current, current_date));
        current += chrono::Duration::days(1);
    }

    result
}

/// Get week boundaries (Monday to Sunday) for a specific date
fn get_week_for_date(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let weekday = date.weekday();
    let days_from_monday = weekday.num_days_from_monday();
    let monday = date - chrono::Duration::days(days_from_monday as i64);
    let sunday = monday + chrono::Duration::days(6);
    (monday, sunday)
}

/// Get month boundaries (first to last day) for a specific date
fn get_month_for_date(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    // `date` is a valid NaiveDate, so its (year, month) is in range and
    // day 1 always exists. Likewise Dec 31 and the 1st of any month <= 12
    // are constructible. The unwraps below cannot panic.
    let first_day = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).expect("y/m valid");
    let last_day = if date.month() == 12 {
        NaiveDate::from_ymd_opt(date.year(), 12, 31).expect("Dec 31 always valid")
    } else {
        NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)
            .expect("next month 1st always valid")
            - chrono::Duration::days(1)
    };
    (first_day, last_day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn compute_today_in_tz_crosses_midnight_eastward() {
        // 2024-12-05 22:30 UTC is already 2024-12-06 01:30 in Europe/Moscow
        // (UTC+3, no DST since 2014). A regression that drops the
        // `.with_timezone(&tz)` call and reads `now_utc.date_naive()` would
        // return 2024-12-05 — i.e. agenda for the day that has just ended
        // locally. This test pins the contract so that regression is caught.
        let now_utc = chrono::Utc
            .with_ymd_and_hms(2024, 12, 5, 22, 30, 0)
            .unwrap();
        let moscow: Tz = "Europe/Moscow".parse().unwrap();
        let today = compute_today_in_tz(now_utc, moscow);
        assert_eq!(
            today,
            NaiveDate::from_ymd_opt(2024, 12, 6).unwrap(),
            "Europe/Moscow at 2024-12-05 22:30 UTC must read as 2024-12-06 local"
        );
    }

    #[test]
    fn compute_today_in_tz_crosses_midnight_westward() {
        // Mirror direction: 2024-12-06 02:00 UTC is 2024-12-05 18:00 in
        // America/Los_Angeles (UTC-8 in winter). Defends against the symmetric
        // bug where `with_timezone` is replaced by raw UTC for "convenience".
        let now_utc = chrono::Utc.with_ymd_and_hms(2024, 12, 6, 2, 0, 0).unwrap();
        let la: Tz = "America/Los_Angeles".parse().unwrap();
        let today = compute_today_in_tz(now_utc, la);
        assert_eq!(
            today,
            NaiveDate::from_ymd_opt(2024, 12, 5).unwrap(),
            "America/Los_Angeles at 2024-12-06 02:00 UTC must read as 2024-12-05 local"
        );
    }

    #[test]
    fn compute_today_in_tz_same_day_midday() {
        // Sanity baseline: a midday UTC instant resolves to the same date in
        // both UTC and a near-UTC timezone, so the assertions above are
        // genuinely about timezone conversion rather than a date arithmetic
        // quirk.
        let now_utc = chrono::Utc.with_ymd_and_hms(2024, 12, 5, 12, 0, 0).unwrap();
        let moscow: Tz = "Europe/Moscow".parse().unwrap();
        assert_eq!(
            compute_today_in_tz(now_utc, moscow),
            NaiveDate::from_ymd_opt(2024, 12, 5).unwrap(),
        );
    }

    fn create_test_task_with_type(
        date_str: &str,
        time: Option<&str>,
        task_type: TaskType,
        ts_type: &str,
    ) -> Task {
        let timestamp = if let Some(t) = time {
            format!("{ts_type}: <{date_str} {t}>")
        } else {
            format!("{ts_type}: <{date_str}>")
        };

        Task {
            file: "test.md".to_string(),
            line: 1,
            heading: "Test task".to_string(),
            content: String::new(),
            task_type: Some(task_type),
            priority: None,
            created: None,
            timestamp: Some(timestamp.clone()),
            timestamp_type: Some(ts_type.to_string()),
            timestamp_active: Some(true),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            timestamp_time: time.map(|t| t.to_string()),
            timestamp_end_time: None,
            clocks: None,
            total_clock_time: None,
            properties: None,
        }
    }

    fn create_test_task(date_str: &str, time: Option<&str>, task_type: TaskType) -> Task {
        create_test_task_with_type(date_str, time, task_type, "SCHEDULED")
    }

    /// Build a Task with a plain inline timestamp (no keyword prefix). The
    /// bracket form (`<...>` vs `[...]`) drives `timestamp_active`, which
    /// agenda re-derives via `parse_org_timestamp` — the field on `Task` is
    /// informational for downstream consumers, not for the agenda filter.
    fn create_test_plain_task(timestamp: &str, date_str: &str) -> Task {
        let active = timestamp.starts_with('<');
        Task {
            file: "test.md".to_string(),
            line: 1,
            heading: "Plain timestamp task".to_string(),
            content: String::new(),
            task_type: Some(TaskType::Todo),
            priority: None,
            created: None,
            timestamp: Some(timestamp.to_string()),
            timestamp_type: Some("PLAIN".to_string()),
            timestamp_active: Some(active),
            timestamp_date: Some(date_str.to_string()),
            timestamp_time: None,
            timestamp_end_time: None,
            clocks: None,
            total_clock_time: None,
            properties: None,
        }
    }

    #[test]
    fn agenda_excludes_plain_inactive_timestamp() {
        // ADR-0014 invariant: inactive `[...]` timestamps never feed agenda.
        // PLAIN inline is the only form that can be inactive and reach the
        // agenda layer (SCHEDULED/DEADLINE only accept `<...>`, CLOSED was
        // already excluded by the timestamp_type guard in handle_repeating_task
        // and is filtered by the same `active` flag here).
        let tasks = vec![create_test_plain_task("[2024-12-05 Thu]", "2024-12-05")];
        let day = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day, day);
        assert!(
            agenda.scheduled_no_time.is_empty(),
            "inactive plain timestamp must not appear in scheduled bucket"
        );
        assert!(agenda.scheduled_timed.is_empty());
        assert!(agenda.overdue.is_empty());
        assert!(agenda.upcoming.is_empty());
    }

    #[test]
    fn agenda_includes_plain_active_timestamp() {
        // Counterpart to the inactive case: an active plain timestamp on
        // its date shows up in the scheduled-no-time bucket. Without this
        // guard the inactive-filter implementation could over-shoot and
        // silently drop active timestamps too.
        let tasks = vec![create_test_plain_task("<2024-12-05 Thu>", "2024-12-05")];
        let day = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day, day);
        assert_eq!(agenda.scheduled_no_time.len(), 1);
    }

    #[test]
    fn scheduled_no_time_sorts_by_priority_then_file_line() {
        use crate::types::Priority;

        // m1 in the 2026-05-25 logic review: scheduled_no_time was the only
        // day-agenda bucket left unsorted, so its order followed the walker's
        // filesystem traversal and could differ between runs on identical
        // input. It is now ordered by priority (high first, mirroring upstream
        // org-agenda's `urgency-down`), then by file path and line as a fully
        // deterministic tiebreaker (approximating `category-keep` / source
        // order). No-priority tasks sort strictly last, like the `--tasks`
        // flat list.
        let day = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let make = |heading: &str, prio: Option<Priority>, file: &str, line: u32| Task {
            file: file.to_string(),
            line,
            heading: heading.to_string(),
            content: String::new(),
            task_type: Some(TaskType::Todo),
            priority: prio,
            created: None,
            timestamp: Some("SCHEDULED: <2024-12-05 Thu>".to_string()),
            timestamp_type: Some("SCHEDULED".to_string()),
            timestamp_active: Some(true),
            timestamp_date: Some("2024-12-05".to_string()),
            timestamp_time: None,
            timestamp_end_time: None,
            clocks: None,
            total_clock_time: None,
            properties: None,
        };

        // Deliberately scrambled input order: highest priority arrives second,
        // the no-priority task arrives first, and the two `[#A]` entries are
        // in reverse file:line order relative to the expected output.
        let tasks = vec![
            make("none-a1", None, "a.md", 1),
            make("A-b5", Some(Priority::A), "b.md", 5),
            make("B-a1", Some(Priority::B), "a.md", 1),
            make("A-a9", Some(Priority::A), "a.md", 9),
        ];

        let agenda = build_day_agenda(&tasks, day, day);
        let order: Vec<&str> = agenda
            .scheduled_no_time
            .iter()
            .map(|t| t.task.heading.as_str())
            .collect();
        assert_eq!(
            order,
            vec!["A-a9", "A-b5", "B-a1", "none-a1"],
            "scheduled_no_time must sort by priority (high first), then file path, then line"
        );
    }

    #[test]
    fn test_scheduled_future_not_shown_as_upcoming() {
        let tasks = vec![
            create_test_task("2024-12-10 Tue", None, TaskType::Todo),
            create_test_task("2024-12-20 Fri", None, TaskType::Todo),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            0,
            "SCHEDULED tasks in future should not appear as upcoming"
        );
        assert_eq!(agenda.scheduled_timed.len(), 0);
        assert_eq!(agenda.scheduled_no_time.len(), 0);
    }

    #[test]
    fn test_deadline_within_14_days_shown_as_upcoming() {
        let tasks = vec![
            create_test_task_with_type("2024-12-10 Tue", None, TaskType::Todo, "DEADLINE"),
            create_test_task_with_type("2024-12-15 Sun", None, TaskType::Todo, "DEADLINE"),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            2,
            "DEADLINE within 14 days should appear as upcoming"
        );
        assert_eq!(agenda.upcoming[0].days_offset, Some(5));
        assert_eq!(agenda.upcoming[1].days_offset, Some(10));
    }

    #[test]
    fn test_deadline_beyond_14_days_not_shown() {
        let tasks = vec![
            create_test_task_with_type("2024-12-20 Fri", None, TaskType::Todo, "DEADLINE"),
            create_test_task_with_type("2025-01-10 Fri", None, TaskType::Todo, "DEADLINE"),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            0,
            "DEADLINE beyond 14 days should not appear"
        );
    }

    #[test]
    fn test_deadline_exactly_14_days_shown() {
        let tasks = vec![create_test_task_with_type(
            "2024-12-19 Thu",
            None,
            TaskType::Todo,
            "DEADLINE",
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            1,
            "DEADLINE exactly 14 days away should appear"
        );
        assert_eq!(agenda.upcoming[0].days_offset, Some(14));
    }

    #[test]
    fn test_deadline_15_days_not_shown() {
        let tasks = vec![create_test_task_with_type(
            "2024-12-20 Fri",
            None,
            TaskType::Todo,
            "DEADLINE",
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            0,
            "DEADLINE 15 days away should not appear"
        );
    }

    #[test]
    fn test_overdue_only_on_current_date() {
        let tasks = vec![
            create_test_task("2024-12-01 Sun", None, TaskType::Todo),
            create_test_task("2024-12-03 Tue", None, TaskType::Todo),
        ];

        // Check on current date - should show overdue
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, current_date, current_date);

        assert_eq!(
            agenda.overdue.len(),
            2,
            "Overdue tasks should appear on current date"
        );
        assert_eq!(agenda.overdue[0].days_offset, Some(-4));
        assert_eq!(agenda.overdue[1].days_offset, Some(-2));

        // Check on past date - should not show overdue
        let past_date = NaiveDate::from_ymd_opt(2024, 12, 2).unwrap();
        let agenda_past = build_day_agenda(&tasks, past_date, current_date);

        assert_eq!(
            agenda_past.overdue.len(),
            0,
            "Overdue should not appear on past dates"
        );
    }

    #[test]
    fn test_week_agenda_past_days_empty() {
        let tasks = vec![
            create_test_task("2024-12-02 Mon", Some("10:00"), TaskType::Todo),
            create_test_task("2024-12-03 Tue", None, TaskType::Todo),
            create_test_task("2024-12-05 Thu", Some("14:00"), TaskType::Todo),
        ];

        let start_date = NaiveDate::from_ymd_opt(2024, 12, 2).unwrap(); // Monday
        let end_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap(); // Sunday
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap(); // Thursday

        let week = build_week_agenda(&tasks, start_date, end_date, current_date);

        assert_eq!(week.len(), 7);

        // Monday (past) - shows scheduled task on its day
        assert_eq!(week[0].date, "2024-12-02");
        assert_eq!(week[0].scheduled_timed.len(), 1);
        assert_eq!(week[0].scheduled_no_time.len(), 0);

        // Tuesday (past) - shows scheduled task on its day
        assert_eq!(week[1].date, "2024-12-03");
        assert_eq!(week[1].scheduled_timed.len(), 0);
        assert_eq!(week[1].scheduled_no_time.len(), 1);

        // Wednesday (past) - no tasks
        assert_eq!(week[2].date, "2024-12-04");
        assert_eq!(week[2].scheduled_timed.len(), 0);

        // Thursday (current) - has tasks
        assert_eq!(week[3].date, "2024-12-05");
        assert_eq!(week[3].scheduled_timed.len(), 1);
        assert_eq!(week[3].overdue.len(), 2); // Monday and Tuesday tasks are overdue

        // Future days should have tasks if scheduled
        assert!(week[4].scheduled_timed.is_empty()); // Friday
    }

    #[test]
    fn test_build_day_agenda_scheduled_timed() {
        let tasks = vec![
            create_test_task("2024-12-05 Wed", Some("10:00"), TaskType::Todo),
            create_test_task("2024-12-05 Wed", Some("14:00"), TaskType::Todo),
            create_test_task("2024-12-05 Wed", None, TaskType::Todo),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.scheduled_timed.len(), 2);
        assert_eq!(agenda.scheduled_no_time.len(), 1);
        assert_eq!(agenda.upcoming.len(), 0);
        assert_eq!(agenda.overdue.len(), 0);

        // Check time sorting
        assert_eq!(
            agenda.scheduled_timed[0].task.timestamp_time,
            Some("10:00".to_string())
        );
        assert_eq!(
            agenda.scheduled_timed[1].task.timestamp_time,
            Some("14:00".to_string())
        );
    }

    #[test]
    fn test_mixed_scheduled_and_deadline() {
        let tasks = vec![
            create_test_task("2024-12-10 Tue", None, TaskType::Todo), // SCHEDULED - not shown
            create_test_task_with_type("2024-12-10 Tue", None, TaskType::Todo, "DEADLINE"), // DEADLINE - shown
            create_test_task_with_type("2024-12-25 Wed", None, TaskType::Todo, "DEADLINE"), // DEADLINE too far - not shown
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            1,
            "Only DEADLINE within 14 days should appear"
        );
        assert_eq!(
            agenda.upcoming[0].task.timestamp_type,
            Some("DEADLINE".to_string())
        );
    }

    fn create_test_task_with_repeater(
        date_str: &str,
        time: Option<&str>,
        repeater: &str,
        task_type: TaskType,
    ) -> Task {
        let timestamp = if let Some(t) = time {
            format!("SCHEDULED: <{date_str} {t} {repeater}>")
        } else {
            format!("SCHEDULED: <{date_str} {repeater}>")
        };

        Task {
            file: "test.md".to_string(),
            line: 1,
            heading: "Test task".to_string(),
            content: String::new(),
            task_type: Some(task_type),
            priority: None,
            created: None,
            timestamp: Some(timestamp.clone()),
            timestamp_type: Some("SCHEDULED".to_string()),
            timestamp_active: Some(true),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            timestamp_time: time.map(|t| t.to_string()),
            timestamp_end_time: None,
            clocks: None,
            total_clock_time: None,
            properties: None,
        }
    }

    fn create_test_task_with_repeater_deadline(
        date_str: &str,
        time: Option<&str>,
        repeater: &str,
        task_type: TaskType,
    ) -> Task {
        let timestamp = if let Some(t) = time {
            format!("DEADLINE: <{date_str} {t} {repeater}>")
        } else {
            format!("DEADLINE: <{date_str} {repeater}>")
        };

        Task {
            file: "test.md".to_string(),
            line: 1,
            heading: "Test task".to_string(),
            content: String::new(),
            task_type: Some(task_type),
            priority: None,
            created: None,
            timestamp: Some(timestamp.clone()),
            timestamp_type: Some("DEADLINE".to_string()),
            timestamp_active: Some(true),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            timestamp_time: time.map(|t| t.to_string()),
            timestamp_end_time: None,
            clocks: None,
            total_clock_time: None,
            properties: None,
        }
    }

    #[test]
    fn test_build_day_agenda_repeating_daily() {
        let tasks = vec![create_test_task_with_repeater(
            "2024-12-01 Sun",
            Some("10:00"),
            "+1d",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.scheduled_timed.len(), 1);
        assert_eq!(
            agenda.scheduled_timed[0].task.timestamp_time,
            Some("10:00".to_string())
        );
    }

    #[test]
    fn test_build_day_agenda_repeating_not_occurrence_day() {
        let tasks = vec![create_test_task_with_repeater(
            "2024-12-01 Sun",
            None,
            "+2d",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 4).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.scheduled_timed.len(), 0);
        assert_eq!(agenda.scheduled_no_time.len(), 0);
    }

    #[test]
    fn test_build_day_agenda_repeating_weekly() {
        let tasks = vec![create_test_task_with_repeater(
            "2024-12-01 Sun",
            None,
            "+1w",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.scheduled_no_time.len(), 1);

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 9).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.scheduled_no_time.len(), 0);
    }

    #[test]
    fn test_build_day_agenda_repeating_every_2_days() {
        let tasks = vec![create_test_task_with_repeater(
            "2024-12-01 Sun",
            None,
            "+2d",
            TaskType::Todo,
        )];

        // +2d from 2024-12-01 → occurrences 12-01, 12-03, 12-05, 12-07, ...
        // Past occurrence days are shown (so week/month agenda surfaces them).
        let test_dates = vec![
            (NaiveDate::from_ymd_opt(2024, 12, 1).unwrap(), true), // base, occurrence
            (NaiveDate::from_ymd_opt(2024, 12, 2).unwrap(), false),
            (NaiveDate::from_ymd_opt(2024, 12, 3).unwrap(), true), // past occurrence
            (NaiveDate::from_ymd_opt(2024, 12, 4).unwrap(), false),
            (NaiveDate::from_ymd_opt(2024, 12, 5).unwrap(), true), // today, occurrence
        ];

        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();

        for (date, should_show) in test_dates {
            let agenda = build_day_agenda(&tasks, date, current_date);
            if should_show {
                assert_eq!(agenda.scheduled_no_time.len(), 1, "Failed for date {date}");
            } else {
                assert_eq!(agenda.scheduled_no_time.len(), 0, "Failed for date {date}");
            }
        }
    }

    #[test]
    fn test_week_agenda_daily_repeater_shows_each_past_occurrence() {
        // Regression: in a week-agenda, a +1d task with base on Monday must
        // appear on every Mon..Sun day, including past days before `today`.
        let tasks = vec![create_test_task_with_repeater(
            "2024-12-02 Mon",
            None,
            "+1d",
            TaskType::Todo,
        )];

        let start_date = NaiveDate::from_ymd_opt(2024, 12, 2).unwrap(); // Monday
        let end_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap(); // Sunday
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap(); // Thursday

        let week = build_week_agenda(&tasks, start_date, end_date, current_date);

        assert_eq!(week.len(), 7);
        for day in &week {
            assert_eq!(
                day.scheduled_no_time.len(),
                1,
                "+1d task must appear on {}",
                day.date
            );
        }
    }

    #[test]
    fn test_overdue_repeating_task_on_non_occurrence_day() {
        let tasks = vec![create_test_task_with_repeater(
            "2024-12-01 Sun",
            Some("10:00"),
            "+2d",
            TaskType::Todo,
        )];

        // 2024-12-06 is NOT an occurrence day (+2d from 2024-12-01: 12-01, 12-03, 12-05)
        // Next occurrence is 12-05, which is in the past, so task is overdue
        let day_date = NaiveDate::from_ymd_opt(2024, 12, 6).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 6).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        // Should appear in overdue (next occurrence 12-05 is in the past)
        assert!(
            !agenda.overdue.is_empty(),
            "expected the +2d task to surface in overdue on a non-occurrence day; \
             got scheduled_timed={} scheduled_no_time={}",
            agenda.scheduled_timed.len(),
            agenda.scheduled_no_time.len()
        );
        assert_eq!(agenda.overdue[0].task.timestamp_time, None);
    }

    #[test]
    fn test_upcoming_repeating_task_has_no_time() {
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2024-12-10 Mon",
            Some("15:00"),
            "+1d",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.upcoming.len(), 1);
        assert_eq!(agenda.upcoming[0].task.timestamp_time, None);
        assert_eq!(agenda.upcoming[0].days_offset, Some(5));
    }

    #[test]
    fn repeating_deadline_past_occurrence_does_not_become_upcoming() {
        // Regression for the dead branch removed in agenda::handle_repeating_task:
        // when `repeat` is `Some(past_occurrence)`, the upcoming bucket must stay
        // empty regardless of how close the next future occurrence is. The
        // previous code had a vestigial `if r > current_date` that could never
        // fire (closest_date(..., Past, ...) returns <= current_date by
        // contract); this test pins the behaviour as the dead branch is removed.
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2024-12-01 Sun",
            None,
            "+1d",
            TaskType::Todo,
        )];

        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, current_date, current_date);

        assert!(
            agenda.upcoming.is_empty(),
            "repeating DEADLINE whose past occurrence is recorded must not surface in upcoming; got {:?}",
            agenda.upcoming
        );
    }

    #[test]
    fn test_repeating_deadline_beyond_warning_not_shown() {
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2026-08-24 Mon",
            None,
            "+1y",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            0,
            "DEADLINE beyond 14 days should not appear in upcoming"
        );
    }

    #[test]
    fn test_build_day_agenda_mixed_repeating_and_regular() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", Some("10:00"), "+1d", TaskType::Todo),
            create_test_task("2024-12-05 Wed", Some("14:00"), TaskType::Todo),
            create_test_task_with_type("2024-12-06 Thu", None, TaskType::Todo, "DEADLINE"),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.scheduled_timed.len(), 2);
        assert_eq!(agenda.upcoming.len(), 1); // Only DEADLINE
    }

    #[test]
    fn test_build_day_agenda_repeating_with_time_sorting() {
        let tasks = vec![
            create_test_task_with_repeater("2024-12-01 Sun", Some("14:00"), "+1d", TaskType::Todo),
            create_test_task_with_repeater("2024-12-01 Sun", Some("09:00"), "+1d", TaskType::Todo),
            create_test_task("2024-12-05 Wed", Some("11:00"), TaskType::Todo),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.scheduled_timed.len(), 3);
        assert_eq!(
            agenda.scheduled_timed[0].task.timestamp_time,
            Some("09:00".to_string())
        );
        assert_eq!(
            agenda.scheduled_timed[1].task.timestamp_time,
            Some("11:00".to_string())
        );
        assert_eq!(
            agenda.scheduled_timed[2].task.timestamp_time,
            Some("14:00".to_string())
        );
    }

    #[test]
    fn test_overdue_tasks_have_no_time() {
        let tasks = vec![
            create_test_task("2024-12-01 Mon", Some("10:00"), TaskType::Todo),
            create_test_task("2024-12-02 Tue", Some("14:00"), TaskType::Todo),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.overdue.len(), 2);
        assert_eq!(agenda.overdue[0].task.timestamp_time, None);
        assert_eq!(agenda.overdue[1].task.timestamp_time, None);
    }

    #[test]
    fn test_upcoming_deadline_tasks_have_no_time() {
        let tasks = vec![
            create_test_task_with_type("2024-12-06 Thu", Some("10:00"), TaskType::Todo, "DEADLINE"),
            create_test_task_with_type("2024-12-07 Fri", Some("14:00"), TaskType::Todo, "DEADLINE"),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.upcoming.len(), 2);
        assert_eq!(agenda.upcoming[0].task.timestamp_time, None);
        assert_eq!(agenda.upcoming[1].task.timestamp_time, None);
    }

    #[test]
    fn test_repeating_task_on_occurrence_day_not_in_overdue() {
        let tasks = vec![create_test_task_with_repeater(
            "2024-12-01 Sun",
            Some("10:00"),
            "+1d",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        // Should appear in scheduled (it's an occurrence day)
        assert_eq!(agenda.scheduled_timed.len(), 1);
        assert_eq!(
            agenda.scheduled_timed[0].task.timestamp_time,
            Some("10:00".to_string())
        );
        assert_eq!(agenda.scheduled_timed[0].days_offset, None);

        // Should NOT appear in overdue (to avoid duplicate)
        assert_eq!(agenda.overdue.len(), 0);
    }

    #[test]
    fn test_repeating_task_no_overdue_if_not_missed() {
        let tasks = vec![create_test_task_with_repeater(
            "2024-12-05 Wed",
            Some("10:00"),
            "+1d",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.scheduled_timed.len(), 1);
        assert_eq!(agenda.overdue.len(), 0);
    }

    #[test]
    fn test_get_current_month_december() {
        // Test December specifically (has 31 days)
        let today = NaiveDate::from_ymd_opt(2024, 12, 15).unwrap();

        // Simulate getting month for December
        let first_day = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
        let last_day = NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap();

        assert_eq!(first_day, NaiveDate::from_ymd_opt(2024, 12, 1).unwrap());
        assert_eq!(last_day, NaiveDate::from_ymd_opt(2024, 12, 31).unwrap());
    }

    #[test]
    fn test_get_current_month_february_leap() {
        // Test February in leap year
        let first_day = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        let last_day = NaiveDate::from_ymd_opt(2024, 3, 1).unwrap() - chrono::Duration::days(1);

        assert_eq!(first_day, NaiveDate::from_ymd_opt(2024, 2, 1).unwrap());
        assert_eq!(last_day, NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());
    }

    #[test]
    fn test_get_current_month_february_non_leap() {
        // Test February in non-leap year
        let first_day = NaiveDate::from_ymd_opt(2025, 2, 1).unwrap();
        let last_day = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap() - chrono::Duration::days(1);

        assert_eq!(first_day, NaiveDate::from_ymd_opt(2025, 2, 1).unwrap());
        assert_eq!(last_day, NaiveDate::from_ymd_opt(2025, 2, 28).unwrap());
    }

    #[test]
    fn test_month_agenda_length() {
        let tasks = vec![create_test_task("2024-12-15 Sun", None, TaskType::Todo)];

        let start_date = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();

        let month = build_week_agenda(&tasks, start_date, end_date, current_date);

        assert_eq!(month.len(), 31, "December should have 31 days");
        assert_eq!(month[0].date, "2024-12-01");
        assert_eq!(month[30].date, "2024-12-31");
    }

    #[test]
    fn test_month_agenda_past_days_empty() {
        let tasks = vec![
            create_test_task("2024-12-02 Mon", Some("10:00"), TaskType::Todo),
            create_test_task("2024-12-03 Tue", None, TaskType::Todo),
            create_test_task("2024-12-10 Tue", Some("14:00"), TaskType::Todo),
        ];

        let start_date = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();

        let month = build_week_agenda(&tasks, start_date, end_date, current_date);

        // Day 1 should be empty
        assert_eq!(month[0].scheduled_timed.len(), 0);
        assert_eq!(month[0].scheduled_no_time.len(), 0);

        // Day 2 should show scheduled task
        assert_eq!(month[1].scheduled_timed.len(), 1);

        // Day 3 should show scheduled task
        assert_eq!(month[2].scheduled_no_time.len(), 1);

        // Day 4 should be empty
        assert_eq!(month[3].scheduled_timed.len(), 0);

        // Current day should have overdue tasks
        assert_eq!(month[4].date, "2024-12-05");
        assert!(
            !month[4].overdue.is_empty(),
            "Current day should have overdue tasks"
        );

        // Future days should have scheduled tasks if applicable
        assert_eq!(
            month[9].scheduled_timed.len(),
            1,
            "Day 10 should have scheduled task"
        );
    }

    #[test]
    fn test_month_agenda_february() {
        let tasks = vec![create_test_task("2024-02-15 Thu", None, TaskType::Todo)];

        let start_date = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(); // Leap year
        let current_date = NaiveDate::from_ymd_opt(2024, 2, 10).unwrap();

        let month = build_week_agenda(&tasks, start_date, end_date, current_date);

        assert_eq!(
            month.len(),
            29,
            "February 2024 (leap year) should have 29 days"
        );
        assert_eq!(month[0].date, "2024-02-01");
        assert_eq!(month[28].date, "2024-02-29");
    }

    #[test]
    fn test_month_agenda_custom_range() {
        let tasks = vec![
            create_test_task("2024-12-10 Tue", None, TaskType::Todo),
            create_test_task("2024-12-15 Sun", None, TaskType::Todo),
        ];

        let start_date = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap();
        let end_date = NaiveDate::from_ymd_opt(2024, 12, 20).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 12).unwrap();

        let range = build_week_agenda(&tasks, start_date, end_date, current_date);

        assert_eq!(
            range.len(),
            11,
            "Range should have 11 days (10-20 inclusive)"
        );
        assert_eq!(range[0].date, "2024-12-10");
        assert_eq!(range[10].date, "2024-12-20");
    }

    #[test]
    fn test_done_tasks_not_in_overdue() {
        let tasks = vec![
            create_test_task("2024-12-01 Sun", None, TaskType::Done),
            create_test_task("2024-12-02 Mon", Some("10:00"), TaskType::Done),
            create_test_task("2024-12-03 Tue", None, TaskType::Todo),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.overdue.len(),
            1,
            "Only TODO tasks should appear in overdue"
        );
        assert_eq!(agenda.overdue[0].task.task_type, Some(TaskType::Todo));
    }

    #[test]
    fn test_done_tasks_shown_on_their_date() {
        let tasks = vec![
            create_test_task("2024-12-05 Wed", None, TaskType::Done),
            create_test_task("2024-12-05 Wed", Some("14:00"), TaskType::Done),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.scheduled_no_time.len(),
            1,
            "DONE task without time should appear on its date"
        );
        assert_eq!(
            agenda.scheduled_timed.len(),
            1,
            "DONE task with time should appear on its date"
        );
        assert_eq!(
            agenda.overdue.len(),
            0,
            "DONE tasks should not appear in overdue"
        );
    }

    #[test]
    fn tasks_scope_sorts_by_priority_with_no_priority_last() {
        // Locks the sort-key invariant: a task without `priority` must sort
        // strictly after every defined Priority, including the lowest one
        // (`Other('Z')` → order 90). Catches a regression where the sentinel
        // for missing priority would fall inside the valid range.
        use crate::types::Priority;

        let mut t_z = create_test_task("2024-12-05 Wed", None, TaskType::Todo);
        t_z.priority = Some(Priority::Other('Z'));
        t_z.heading = "Z-priority".to_string();

        let mut t_a = create_test_task("2024-12-05 Wed", None, TaskType::Todo);
        t_a.priority = Some(Priority::A);
        t_a.heading = "A-priority".to_string();

        let mut t_none = create_test_task("2024-12-05 Wed", None, TaskType::Todo);
        t_none.priority = None;
        t_none.heading = "no-priority".to_string();

        let mut t_num0 = create_test_task("2024-12-05 Wed", None, TaskType::Todo);
        t_num0.priority = Some(Priority::Numeric(0));
        t_num0.heading = "numeric-0".to_string();

        // Mixed input order so the assertion proves the sort is doing the work.
        let input = vec![t_none.clone(), t_z.clone(), t_a.clone(), t_num0.clone()];

        // Tasks scope does not accept date arguments: --current-date is
        // about overdue baseline, which tasks mode does not use (see
        // ADR-0009). The fixed task dates inside the input still make the
        // test deterministic without it.
        let result = filter_agenda(
            input,
            AgendaScope::Tasks,
            AgendaDates::default(),
            "UTC",
            false,
        )
        .expect("filter_agenda");

        let tasks = match result {
            AgendaOutput::Tasks(tasks) => tasks,
            other => panic!("expected AgendaOutput::Tasks, got {other:?}"),
        };
        let headings: Vec<&str> = tasks.iter().map(|t| t.heading.as_str()).collect();
        assert_eq!(
            headings,
            vec!["numeric-0", "A-priority", "Z-priority", "no-priority"],
            "no-priority must sort strictly after every defined priority"
        );
    }

    #[test]
    fn tasks_scope_excludes_done_by_default() {
        // The flat `--tasks` list is TODO-only by default — the documented
        // contract pinned by the JSON wire-contract snapshot tests. A DONE
        // task must never leak in when `include_done` is false.
        let input = vec![
            create_test_task("2024-12-05 Wed", None, TaskType::Todo),
            create_test_task("2024-12-06 Thu", None, TaskType::Done),
        ];

        let result = filter_agenda(
            input,
            AgendaScope::Tasks,
            AgendaDates::default(),
            "UTC",
            false,
        )
        .expect("filter_agenda");

        let tasks = match result {
            AgendaOutput::Tasks(tasks) => tasks,
            other => panic!("expected AgendaOutput::Tasks, got {other:?}"),
        };
        assert_eq!(tasks.len(), 1, "only the TODO task must remain");
        assert_eq!(tasks[0].task_type, Some(TaskType::Todo));
    }

    #[test]
    fn tasks_scope_includes_done_when_requested() {
        // With `include_done` set (the opt-in `--tasks-include-done` flag),
        // the flat `--tasks` list surfaces DONE tasks alongside TODO ones so
        // a consumer can act on completion (e.g. a calendar sync deleting the
        // event for a finished task). The default TODO-only behaviour is left
        // intact; this branch only relaxes the filter.
        let input = vec![
            create_test_task("2024-12-05 Wed", None, TaskType::Todo),
            create_test_task("2024-12-06 Thu", None, TaskType::Done),
        ];

        let result = filter_agenda(
            input,
            AgendaScope::Tasks,
            AgendaDates::default(),
            "UTC",
            true,
        )
        .expect("filter_agenda");

        let tasks = match result {
            AgendaOutput::Tasks(tasks) => tasks,
            other => panic!("expected AgendaOutput::Tasks, got {other:?}"),
        };
        assert_eq!(tasks.len(), 2, "both TODO and DONE must be present");
        assert!(
            tasks
                .iter()
                .any(|t| matches!(t.task_type, Some(TaskType::Todo))),
            "TODO task must be present"
        );
        assert!(
            tasks
                .iter()
                .any(|t| matches!(t.task_type, Some(TaskType::Done))),
            "DONE task must be present when include_done is set"
        );
    }

    #[test]
    fn test_done_deadline_not_in_overdue() {
        let tasks = vec![
            create_test_task_with_type("2024-12-01 Sun", None, TaskType::Done, "DEADLINE"),
            create_test_task_with_type("2024-12-02 Mon", None, TaskType::Todo, "DEADLINE"),
        ];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.overdue.len(),
            1,
            "Only TODO deadline should appear in overdue"
        );
        assert_eq!(agenda.overdue[0].task.task_type, Some(TaskType::Todo));
    }

    #[test]
    fn test_workday_repeater_not_overdue_on_weekend() {
        // Task scheduled for Friday with +1wd repeater
        let tasks = vec![create_test_task_with_repeater(
            "2025-12-05 Fri",
            None,
            "+1wd",
            TaskType::Todo,
        )];

        // Today is Saturday - next workday is Monday
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        // Should NOT appear as overdue because next occurrence is Monday (in the future)
        assert_eq!(
            agenda.overdue.len(),
            0,
            "Task with +1wd should not be overdue on Saturday"
        );
        assert_eq!(agenda.scheduled_timed.len(), 0);
        assert_eq!(agenda.scheduled_no_time.len(), 0);
    }

    #[test]
    fn test_workday_repeater_not_overdue_on_sunday() {
        let tasks = vec![create_test_task_with_repeater(
            "2025-12-05 Fri",
            None,
            "+1wd",
            TaskType::Todo,
        )];

        // Today is Sunday - next workday is Monday
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.overdue.len(),
            0,
            "Task with +1wd should not be overdue on Sunday"
        );
    }

    #[test]
    fn test_year_repeater_shows_on_occurrence_day() {
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2025-12-11 Thu",
            None,
            "+1y",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 12, 11).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 11).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.scheduled_no_time.len(), 1);
        assert_eq!(agenda.overdue.len(), 0);
    }

    #[test]
    fn test_year_repeater_shows_in_upcoming() {
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2025-12-11 Thu",
            None,
            "+1y",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.upcoming.len(), 1);
        assert_eq!(agenda.upcoming[0].days_offset, Some(5));
    }

    #[test]
    fn test_year_repeater_not_in_upcoming_too_far() {
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2025-12-11 Thu",
            None,
            "+1y",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 11, 21).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 11, 21).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.upcoming.len(), 0);
    }

    #[test]
    fn test_month_repeater_shows_on_occurrence_day() {
        let tasks = vec![create_test_task_with_repeater(
            "2024-12-05 Thu",
            None,
            "+1m",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 1, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 1, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.scheduled_no_time.len(), 1);
    }

    #[test]
    fn test_workday_repeater_scheduled_on_monday() {
        let tasks = vec![create_test_task_with_repeater(
            "2025-12-05 Fri",
            None,
            "+1wd",
            TaskType::Todo,
        )];

        // Today is Monday - this is the next occurrence day
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.scheduled_no_time.len(),
            1,
            "Task should be scheduled on Monday"
        );
        assert_eq!(
            agenda.overdue.len(),
            0,
            "Task should not be overdue on its occurrence day"
        );
    }

    #[test]
    fn test_yearly_deadline_shows_on_occurrence_day() {
        // День Рождения Джамика: DEADLINE <2024-12-05 Thu +1y>
        // В 2025 году дедлайн должен быть 2025-12-05 (пятница)
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2024-12-05 Thu",
            None,
            "+1y",
            TaskType::Todo,
        )];

        // Пятница 2025-12-05 - день deadline (последнее вхождение <= today)
        // По логике org-mode показывается, даже если это прошлая дата
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap(); // Сегодня воскресенье
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.scheduled_no_time.len(),
            1,
            "Task should be shown on deadline day (org-mode logic)"
        );
        assert_eq!(agenda.overdue.len(), 0);

        // Проверим будущий occurrence day (2026-12-05)
        let future_day = NaiveDate::from_ymd_opt(2026, 12, 5).unwrap();
        let agenda_future = build_day_agenda(&tasks, future_day, current_date);

        assert_eq!(
            agenda_future.scheduled_no_time.len(),
            1,
            "Future occurrence day should show task"
        );
        assert_eq!(
            agenda_future.scheduled_no_time[0].task.timestamp_date,
            Some("2026-12-05".to_string())
        );
        assert!(agenda_future.scheduled_no_time[0]
            .task
            .timestamp
            .as_ref()
            .unwrap()
            .contains("2026-12-05"));
    }

    #[test]
    fn test_yearly_deadline_shows_as_overdue_after_occurrence() {
        // День Рождения Джамика: DEADLINE <2024-12-05 Thu +1y>
        // В 2025 году дедлайн был 2025-12-05 (пятница)
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2024-12-05 Thu",
            None,
            "+1y",
            TaskType::Todo,
        )];

        // Воскресенье 2025-12-07 - через 2 дня после дедлайна
        let day_date = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.overdue.len(), 1, "Task should be overdue on Sunday");
        assert_eq!(
            agenda.overdue[0].days_offset,
            Some(-2),
            "Task should be 2 days overdue"
        );

        // Check that timestamp shows last occurrence date (2025-12-05)
        assert_eq!(
            agenda.overdue[0].task.timestamp_date,
            Some("2025-12-05".to_string())
        );
        assert!(agenda.overdue[0]
            .task
            .timestamp
            .as_ref()
            .unwrap()
            .contains("2025-12-05"));
    }

    /// Build a repeating task with explicit `timestamp_type` so tests can
    /// cover CLOSED-typed timestamps without piggybacking on the
    /// SCHEDULED / DEADLINE helpers.
    fn create_test_task_with_repeater_and_ts_type(
        date_str: &str,
        repeater: &str,
        task_type: TaskType,
        ts_type: &str,
    ) -> Task {
        let timestamp = format!("{ts_type}: <{date_str} {repeater}>");
        Task {
            file: "test.md".to_string(),
            line: 1,
            heading: "Test task".to_string(),
            content: String::new(),
            task_type: Some(task_type),
            priority: None,
            created: None,
            timestamp: Some(timestamp),
            timestamp_type: Some(ts_type.to_string()),
            timestamp_active: Some(true),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            timestamp_time: None,
            timestamp_end_time: None,
            clocks: None,
            total_clock_time: None,
            properties: None,
        }
    }

    // Upstream Org-mode (org-agenda.el lines 6424-6428) unconditionally
    // suppresses past-due warnings and deadline prewarnings for DONE tasks:
    //
    //     ;; Possibly skip done tasks.
    //     (when (and done?
    //                (or org-agenda-skip-deadline-if-done
    //                    (/= deadline current)))
    //       (throw :skip nil))
    //
    // Only the actual deadline date is left subject to the user's opt-in
    // `org-agenda-skip-deadline-if-done` flag; everything else is silent
    // when the task is DONE. The repeating-task path needs the same guard,
    // which `handle_non_repeating_task` already has for the overdue bucket
    // (`days_diff < 0 && is_today && !is_done`) but not for upcoming.

    #[test]
    fn test_done_repeating_deadline_not_in_overdue() {
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2024-12-01 Sun",
            None,
            "+1w",
            TaskType::Done,
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.overdue.len(),
            0,
            "DONE repeating DEADLINE must not surface as overdue (matches upstream org-agenda.el L6424-6428)"
        );
    }

    #[test]
    fn test_done_repeating_deadline_not_in_upcoming() {
        // base 5 days in the future, within the 14-day warning period.
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2025-12-11 Thu",
            None,
            "+1y",
            TaskType::Done,
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            0,
            "DONE repeating DEADLINE must not surface as prewarning (matches upstream org-agenda.el L6424-6428)"
        );
    }

    #[test]
    fn test_done_repeating_still_shows_on_occurrence_day() {
        // Upstream default: `org-agenda-skip-deadline-if-done` is nil, so a
        // DONE task IS still shown on its actual occurrence date. The fix
        // for overdue/upcoming must not regress this.
        let tasks = vec![create_test_task_with_repeater(
            "2024-12-01 Sun",
            None,
            "+1w",
            TaskType::Done,
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 8).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.scheduled_no_time.len(),
            1,
            "DONE repeating task must still appear on its occurrence day"
        );
        assert_eq!(agenda.overdue.len(), 0);
        assert_eq!(agenda.upcoming.len(), 0);
    }

    // Upstream Org-mode (org-agenda.el L5571) routes CLOSED-typed
    // timestamps to `org-agenda-get-progress`, never to
    // `org-agenda-get-deadlines` or `org-agenda-get-scheduled`. The
    // project does not implement a progress view, but the daily agenda
    // must not mistake a CLOSED timestamp for a deadline candidate. In
    // practice, real-world Org files never emit `CLOSED: [...+1w]`, so
    // this is a defensive guard rather than a bug fix for a common case.
    // (ADR-0014 also rules out CLOSED with active `<...>`.)

    #[test]
    fn test_closed_repeating_not_in_overdue() {
        let tasks = vec![create_test_task_with_repeater_and_ts_type(
            "2024-12-01 Sun",
            "+1w",
            TaskType::Todo,
            "CLOSED",
        )];

        let day_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.overdue.len(),
            0,
            "CLOSED-typed timestamps must not surface as overdue"
        );
    }

    // Warning-period cookie `-N<unit>` overrides the default
    // `DEADLINE_WARNING_DAYS` (14) for one specific DEADLINE, matching
    // upstream `org-get-wdays` (lisp/org.el L14937-L14943). Smaller values
    // shrink the window (silent until N days before), larger values
    // expand it (start warning earlier).

    #[test]
    fn test_deadline_with_minus_3d_not_in_upcoming_at_day_5() {
        // -3d means "warn me 3 days before"; today is 5 days out, so the
        // task must NOT yet appear in upcoming (with the default 14d it
        // would).
        let tasks = vec![create_test_task_with_type(
            "2025-12-10 Wed -3d",
            None,
            TaskType::Todo,
            "DEADLINE",
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            0,
            "DEADLINE with -3d cookie must not appear in upcoming at day 5"
        );
    }

    #[test]
    fn test_deadline_with_minus_3d_in_upcoming_at_day_2() {
        // Same task, but today is 2 days out — inside the 3-day window.
        let tasks = vec![create_test_task_with_type(
            "2025-12-10 Wed -3d",
            None,
            TaskType::Todo,
            "DEADLINE",
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.upcoming.len(), 1);
        assert_eq!(agenda.upcoming[0].days_offset, Some(2));
    }

    #[test]
    fn test_deadline_with_minus_30d_in_upcoming_beyond_default_14() {
        // -30d expands the window beyond the 14-day default; today is 20
        // days out, so the task must appear in upcoming (default would
        // skip).
        let tasks = vec![create_test_task_with_type(
            "2025-12-25 Thu -30d",
            None,
            TaskType::Todo,
            "DEADLINE",
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            1,
            "DEADLINE with -30d must appear in upcoming at day 20 (default 14 would skip)"
        );
        assert_eq!(agenda.upcoming[0].days_offset, Some(20));
    }

    #[test]
    fn test_repeating_deadline_with_minus_3d_not_in_upcoming_at_day_5() {
        // Same semantics for the repeating-task path: cookie overrides
        // the global default.
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2025-12-10 Wed -3d",
            None,
            "+1y",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            0,
            "repeating DEADLINE with -3d cookie must not appear in upcoming at day 5"
        );
    }

    #[test]
    fn test_repeating_deadline_with_minus_3d_in_upcoming_at_day_2() {
        let tasks = vec![create_test_task_with_repeater_deadline(
            "2025-12-10 Wed -3d",
            None,
            "+1y",
            TaskType::Todo,
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(agenda.upcoming.len(), 1);
        assert_eq!(agenda.upcoming[0].days_offset, Some(2));
    }

    #[test]
    fn test_closed_repeating_not_in_upcoming() {
        // CLOSED can never enter the existing upcoming branch because that
        // branch already gates on `ts_type == \"DEADLINE\"`; this test pins
        // that gate so a future refactor cannot quietly relax it.
        let tasks = vec![create_test_task_with_repeater_and_ts_type(
            "2025-12-11 Thu",
            "+1y",
            TaskType::Todo,
            "CLOSED",
        )];

        let day_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let current_date = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let agenda = build_day_agenda(&tasks, day_date, current_date);

        assert_eq!(
            agenda.upcoming.len(),
            0,
            "CLOSED-typed timestamps must never enter the upcoming bucket"
        );
    }
}
