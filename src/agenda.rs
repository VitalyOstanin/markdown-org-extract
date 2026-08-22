//! Turning a flat task list into a dated agenda.
//!
//! [`filter_agenda`] takes everything a scan found and keeps only what falls
//! inside the requested window, grouping the survivors per day. The window is
//! anchored on [`AgendaDates::current_date`] rather than the host clock so the
//! same input always renders the same output (ADR-0015).

use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use chrono_tz::Tz;

use crate::error::AppError;
use crate::exceptions::{ExcludedOccurrences, OccurrenceExceptions};
use crate::timestamp::{parse_org_timestamp, DatePreference, ParsedTimestamp};
use crate::types::{DayAgenda, Task, TaskType, TaskWithOffset, MAX_DIAGNOSTIC_ITEMS};

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
    /// Occurrences this entry does not have (ADR-0031): the dates of its own
    /// `EXDATE`, plus the ones another entry of the run replaces. Empty for
    /// all but the few entries that carry an exception, which is why it is a
    /// set per task rather than a lookup on every day of every week.
    excluded: ExcludedOccurrences,
}

fn prepare_tasks<'a>(
    tasks: &'a [Task],
    exceptions: &OccurrenceExceptions,
) -> Vec<PreparedTask<'a>> {
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
            excluded: exceptions.dates_for(t),
        })
        .collect()
}

/// Say which replacements name a series no entry of this run carries.
///
/// The parser reports an exception that cannot be read; this is the one an
/// entry alone cannot see, because the two halves live in two entries and
/// possibly in two files. A `SERIES_ID` nothing answers to replaces nothing,
/// and the day it names keeps both the series occurrence and the entry that
/// meant to stand in for it — the failure ADR-0031 calls out by name.
///
/// One record for the lot, listing the first [`MAX_DIAGNOSTIC_ITEMS`] of
/// them, in the shape the run summary already uses: a corpus with hundreds of
/// broken replacements says so once rather than a hundred times.
fn warn_about_unknown_series(exceptions: &OccurrenceExceptions) {
    let unknown = exceptions.unknown_series();
    if unknown.is_empty() {
        return;
    }
    let named: Vec<&str> = unknown
        .iter()
        .take(MAX_DIAGNOSTIC_ITEMS)
        .map(String::as_str)
        .collect();
    tracing::warn!(
        series = ?named,
        series_count = unknown.len(),
        series_shown = named.len(),
        "a replacement names a series no entry of this run has; the occurrence it names is not replaced"
    );
}

/// Result of running [`filter_agenda`]. The variant is determined by the
/// requested [`AgendaScope`]:
///
/// - [`AgendaScope::Day`] / [`AgendaScope::Week`] / [`AgendaScope::Month`] /
///   [`AgendaScope::MonthGrid`] produce [`AgendaOutput::Days`] — one
///   [`DayAgenda`] per day in the window, each carrying overdue / scheduled /
///   upcoming buckets.
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
    /// A single day.
    Day,
    /// Seven days starting from the anchor date.
    Week,
    /// The calendar month containing the anchor date.
    Month,
    /// The whole weeks a calendar month falls in: the month, plus the days its
    /// first and last weeks borrow from the months beside it.
    ///
    /// Upstream has no such window — `org-agenda` draws a month as the list of
    /// its days, and the calendar of Emacs gets its entries through the
    /// `org-diary` sexp instead. It exists here because both clients of this
    /// crate draw the month as a grid, and the rule for which days a grid holds
    /// depends on which weekday a week starts on — a value this crate now
    /// owns. See ADR-0028.
    MonthGrid,
    /// No date window: every task, as a flat list.
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
    /// is the only day, in `Week` / `Month` / `MonthGrid` scope it picks the
    /// containing week / month. Ignored if `from`/`to` is set. Rejected under
    /// `Tasks` scope.
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
    /// Value of `--week-start`: which weekday a week begins on, as a name
    /// (`monday` … `sunday`), or `today` for a week that begins on the anchor
    /// day. Absent means Monday, which is what every window produced before
    /// the argument existed.
    ///
    /// This is upstream's `org-agenda-start-on-weekday`, whose `nil` is spelled
    /// `today` here (org-agenda.el:1181). Like upstream, it reaches the
    /// week-shaped windows only: `Day` has no week to align, `Month` is a
    /// calendar month whatever the week does, and `MonthGrid` draws columns
    /// per weekday and therefore refuses `today`.
    pub week_start: Option<&'a str>,
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

/// Resolve the window of a scope whose output is a range of days, in the one
/// order ADR-0009 gives: an explicit `--from`/`--to` first, then the period
/// containing `--date`, then the period containing today.
///
/// `period_of` names the period a scope draws around an anchor day — its week,
/// its month, the grid its month falls in. `adjust_range` gets the last word on
/// an explicit window, so a scope that needs whole weeks can grow it; scopes
/// that take the window as given return it unchanged.
fn resolve_period_window(
    date: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    today: NaiveDate,
    period_of: impl Fn(NaiveDate) -> (NaiveDate, NaiveDate),
    adjust_range: impl Fn(NaiveDate, NaiveDate) -> (NaiveDate, NaiveDate),
) -> Result<(NaiveDate, NaiveDate), AppError> {
    if let Some((start, end)) = parse_range(from, to, today)? {
        return Ok(adjust_range(start, end));
    }
    let anchor = match date {
        Some(date_str) => parse_date_arg("date", date_str)?,
        None => today,
    };
    Ok(period_of(anchor))
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
/// - `include_cancelled` — value of `--tasks-include-cancelled`. Only
///   affects [`AgendaScope::Tasks`]: when `true` the flat list additionally
///   surfaces `CANCELLED` tasks. Independent of `include_done` (neither
///   implies the other). A no-op for day / week / month scope.
/// - `annotate_next` — whether to fill `timestamp_next` and, on the cells of
///   a dated scope, `timestamp_next_after` (see `annotate_next_occurrences`
///   and ADR-0029). Only the JSON output carries either field, so the
///   Markdown / HTML renderers pass `false` and skip the work. Always a no-op
///   for [`AgendaScope::Tasks`], which stays date-less per ADR-0023.
///
/// Errors:
/// - `AppError::InvalidDate` — any of `date`/`from`/`to`/`current-date`
///   failed `YYYY-MM-DD` parse.
/// - `AppError::InvalidTimezone` — `tz` was not recognised by chrono-tz.
/// - `AppError::DateRange` — the window cannot be built as asked: `from > to`
///   after edge filling; a window argument (`date`, `from`, `to`,
///   `current_date`, `week_start`) under [`AgendaScope::Tasks`], which has no
///   window; `week_start` naming an anchored week under
///   [`AgendaScope::MonthGrid`], whose columns need a fixed weekday.
pub fn filter_agenda(
    tasks: Vec<Task>,
    scope: AgendaScope,
    dates: AgendaDates<'_>,
    tz: &str,
    include_done: bool,
    include_cancelled: bool,
    annotate_next: bool,
) -> Result<AgendaOutput, AppError> {
    let AgendaDates {
        date,
        from,
        to,
        current_date: current_date_override,
        week_start,
    } = dates;

    let tz: Tz = tz
        .parse()
        .map_err(|_| AppError::InvalidTimezone(tz.to_string()))?;

    // One instant drives both the reference date and the reference moment
    // below: reading the clock twice can straddle midnight and leave the
    // agenda's "today" a day apart from the moment `timestamp_next` uses.
    let now_utc = chrono::Utc::now();

    let today = match current_date_override {
        Some(date_str) => parse_date_arg("current-date", date_str)?,
        None => compute_today_in_tz(now_utc, tz),
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
    // silently ignored. See ADR-0009 for the model. A week start is a window
    // argument like the rest: the flat list has no window to begin.
    if scope == AgendaScope::Tasks
        && (date.is_some()
            || from.is_some()
            || to.is_some()
            || current_date_override.is_some()
            || week_start.is_some())
    {
        return Err(AppError::DateRange(
            "tasks mode does not accept date arguments (--date, --from, --to, --current-date, --week-start)"
                .to_string(),
        ));
    }

    let week_start_given = week_start.is_some();
    let week_start = parse_week_start(week_start)?;

    // Refused up-front, before the tasks are annotated and bucketed: the run
    // cannot produce a grid, so there is nothing to spend that work on.
    if scope == AgendaScope::MonthGrid && week_start.is_none() {
        return Err(month_grid_needs_a_first_day());
    }

    // Accepted but inert in the scopes with no week to align: a single day has
    // none, and a calendar month is the same month whatever the week does.
    // Logged rather than refused — a client that passes its user's first day
    // of the week on every call should not have to strip it per scope — but
    // logged, so "my week start did nothing" is answerable from `-vv`.
    if week_start_given && matches!(scope, AgendaScope::Day | AgendaScope::Month) {
        tracing::debug!(
            scope = ?scope,
            "week-start does not reach this scope and is ignored"
        );
    }

    // Reference instant for `timestamp_next`: the local wall-clock now, so a
    // timed occurrence earlier today is recognised as past. Under a
    // `--current-date` override (tests / pinning) the time is unknown, so use
    // midnight for a deterministic result.
    let now_dt: NaiveDateTime = match current_date_override {
        Some(_) => today.and_time(NaiveTime::MIN),
        None => now_utc.with_timezone(&tz).naive_local(),
    };

    // Annotate before building: the agenda rewrites `timestamp_date` on the
    // copies it renders, and the field must stay anchored on the task's own
    // date. See `annotate_next_occurrences`. Tasks scope stays date-less.
    let mut tasks = tasks;
    // One index per run, built from the list as it stands: a replacement names
    // the series it replaces an occurrence of, so the answer for one task
    // depends on the others (ADR-0031). Both passes that need it — the
    // now-relative one below and the day-by-day walk after it — read this one,
    // and the mismatched `SERIES_ID` values are reported once rather than once
    // per pass.
    let exceptions = OccurrenceExceptions::from_tasks(&tasks);
    warn_about_unknown_series(&exceptions);
    if annotate_next && scope != AgendaScope::Tasks {
        annotate_next_occurrences(&mut tasks, now_dt, &exceptions);
    }

    match scope {
        AgendaScope::Day => {
            // --from/--to: range of day-agendas. Single edge falls back to
            // `today` (current_date or --current-date).
            if let Some((start_date, end_date)) = parse_range(from, to, today)? {
                Ok(AgendaOutput::Days(build_week_agenda(
                    &tasks,
                    start_date,
                    end_date,
                    today,
                    &exceptions,
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
                    &exceptions,
                )]))
            }
        }
        AgendaScope::Week => {
            let (start_date, end_date) = resolve_period_window(
                date,
                from,
                to,
                today,
                |anchor| get_week_for_date(anchor, week_start),
                |start, end| {
                    // An explicit window is the window; unlike the grid, a week
                    // agenda is a list of days and needs no alignment to stay
                    // readable. See ADR-0009 for the priority.
                    if week_start_given {
                        tracing::debug!("an explicit --from/--to window overrides week-start");
                    }
                    (start, end)
                },
            )?;

            Ok(AgendaOutput::Days(build_week_agenda(
                &tasks,
                start_date,
                end_date,
                today,
                &exceptions,
            )))
        }
        AgendaScope::Month => {
            let (start_date, end_date) =
                resolve_period_window(date, from, to, today, get_month_for_date, |start, end| {
                    (start, end)
                })?;

            Ok(AgendaOutput::Days(build_week_agenda(
                &tasks,
                start_date,
                end_date,
                today,
                &exceptions,
            )))
        }
        AgendaScope::MonthGrid => {
            // Refused above; repeated rather than assumed so a future edit that
            // moves the early check cannot turn `today` into a silent Monday.
            let Some(first_day) = week_start else {
                return Err(month_grid_needs_a_first_day());
            };
            let (start_date, end_date) = resolve_period_window(
                date,
                from,
                to,
                today,
                |anchor| get_month_grid_for_date(anchor, first_day),
                // An explicit window is grown to the weeks it touches: the
                // scope draws rows of seven, and a window ending mid-week would
                // leave the last row short of the columns it declares. A window
                // already on the edges of a week is left alone.
                |start, end| grow_to_whole_weeks(start, end, first_day),
            )?;

            Ok(AgendaOutput::Days(build_week_agenda(
                &tasks,
                start_date,
                end_date,
                today,
                &exceptions,
            )))
        }
        AgendaScope::Tasks => {
            // Default: TODO only — the documented contract, pinned by the JSON
            // wire-contract snapshot tests and grepped for by existing
            // pipelines. The opt-in `--tasks-include-done` (`include_done`)
            // additionally surfaces `DONE` tasks so a consumer can act on
            // completion (e.g. a calendar sync deleting the event for a
            // finished task). The independent opt-in `--tasks-include-cancelled`
            // (`include_cancelled`) additionally surfaces `CANCELLED` tasks for
            // the same reason. Neither flag implies the other.
            let mut filtered: Vec<Task> = tasks
                .into_iter()
                .filter(|t| {
                    matches!(t.task_type, Some(TaskType::Todo))
                        || (include_done && matches!(t.task_type, Some(TaskType::Done)))
                        || (include_cancelled
                            && matches!(t.task_type, Some(TaskType::Cancelled(_))))
                })
                .collect();
            // Priority first, then the date and the time — the only axis a
            // date-less list can be read along — with the file and the line
            // left as the tiebreaker. Sorting by priority alone left the rest
            // to the walk over the tree, which is unspecified: two runs over
            // the same notes could hand a consumer the same tasks in another
            // order, and a reader saw 09:30 above 08:00 with nothing to
            // explain it.
            //
            // What has no time to sort by goes last, at both levels: a task
            // with no date after every dated one, and within a day the
            // whole-day task after the timed ones. That is org-agenda's own
            // answer — `org-agenda-sort-notime-is-late` defaults to t, which
            // reads an entry without a clock time as 99:01, later than any
            // entry that has one — and it matches the day agenda here, where
            // `scheduled_no_time` renders below the hours.
            filtered.sort_by(|a, b| {
                let pa = a
                    .priority
                    .as_ref()
                    .map(|p| p.order())
                    .unwrap_or(NO_PRIORITY_ORDER);
                let pb = b
                    .priority
                    .as_ref()
                    .map(|p| p.order())
                    .unwrap_or(NO_PRIORITY_ORDER);
                pa.cmp(&pb)
                    .then_with(|| a.timestamp_date.is_none().cmp(&b.timestamp_date.is_none()))
                    .then_with(|| a.timestamp_date.cmp(&b.timestamp_date))
                    .then_with(|| a.timestamp_time.is_none().cmp(&b.timestamp_time.is_none()))
                    .then_with(|| a.timestamp_time.cmp(&b.timestamp_time))
                    .then_with(|| a.file.cmp(&b.file))
                    .then_with(|| a.line.cmp(&b.line))
            });
            Ok(AgendaOutput::Tasks(filtered))
        }
    }
}

/// Closest still-upcoming occurrence of `repeater` (anchored at `base`)
/// relative to `now`, at day granularity:
/// - a date before today rolls forward to the first occurrence today-or-later;
/// - an occurrence that lands on today stays today when the task has no clock
///   time, or when its time is still ahead of `now`;
/// - an occurrence on today whose clock time has already passed rolls to the
///   following occurrence.
///
/// Returns `None` only when the repeater cannot bracket a future date.
fn next_occurrence(
    base: NaiveDate,
    repeater: &crate::timestamp::Repeater,
    time: Option<NaiveTime>,
    now: NaiveDateTime,
    excluded: &ExcludedOccurrences,
) -> Option<NaiveDate> {
    use crate::timestamp::closest_date;

    let today = now.date();
    let next = closest_date(base, today, DatePreference::Future, repeater);
    let slot_passed_today = next == Some(today) && time.is_some_and(|t| t < now.time());
    let next = if slot_passed_today {
        today
            .succ_opt()
            .and_then(|tomorrow| closest_date(base, tomorrow, DatePreference::Future, repeater))
    } else {
        next
    };
    skip_excluded(base, repeater, next, excluded, Walk::Upcoming)
}

/// Which way a walk over a series runs, and what an occurrence that moved
/// means to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Walk {
    /// Forward, to the occurrence that is coming up. Both reasons take an
    /// occurrence out of the day it would have fallen on, so the walk steps
    /// past either and answers with the next one that still stands.
    Upcoming,
    /// Backward, to the occurrence the entry is in arrears for. A cancelled
    /// occurrence never was, so the walk carries on past it; a replaced one
    /// did take place — in the entry that replaced it, which owes the debt
    /// itself — so the walk ends there rather than reaching further back and
    /// making the series look older than it was before the move (ADR-0032).
    Arrears,
}

impl Walk {
    /// Which side of a day this walk looks for an occurrence on.
    fn prefer(self) -> DatePreference {
        match self {
            Walk::Upcoming => DatePreference::Future,
            Walk::Arrears => DatePreference::Past,
        }
    }

    /// The day past `date` the next candidate is bracketed from.
    ///
    /// A step of one day rather than none, because `closest_date` answers with
    /// the day itself when the series falls on it: asked again from `date`,
    /// the walk would stand still on the occurrence it has just rejected.
    fn beyond(self, date: NaiveDate) -> Option<NaiveDate> {
        match self {
            Walk::Upcoming => date.succ_opt(),
            Walk::Arrears => date.pred_opt(),
        }
    }

    /// Whether meeting a replaced occurrence ends this walk rather than being
    /// stepped over.
    fn ends_at_a_replacement(self) -> bool {
        matches!(self, Walk::Arrears)
    }
}

/// Walk past the occurrences an entry does not have, the way `walk` asks for
/// (ADR-0031, ADR-0032).
///
/// One body for both directions: the bound below is the part worth keeping in
/// one place, and a mirrored copy of it could drift from this one without the
/// compiler noticing.
///
/// Bounded by the number of exceptions, and the bound cannot be reached: every
/// step lands strictly past the candidate before it — that is the contract of
/// `closest_date` — so `excluded.len() + 1` candidates cannot all be
/// exceptions of a set holding `excluded.len()` of them. The bound is what
/// makes that argument safe to rely on rather than a fallback: without it, a
/// repeater that failed to advance would be walked in circles. The `None`
/// after the loop is therefore unreachable, and the answers of `None` that do
/// happen are the two `?` inside it — the end of the calendar, and a series
/// with no occurrence on that side of the day at all.
fn skip_excluded(
    base: NaiveDate,
    repeater: &crate::timestamp::Repeater,
    from: Option<NaiveDate>,
    excluded: &ExcludedOccurrences,
    walk: Walk,
) -> Option<NaiveDate> {
    use crate::timestamp::closest_date;

    let mut candidate = from?;
    for _ in 0..=excluded.len() {
        if walk.ends_at_a_replacement() && excluded.is_replaced(&candidate) {
            return None;
        }
        if !excluded.contains(&candidate) {
            return Some(candidate);
        }
        candidate = closest_date(base, walk.beyond(candidate)?, walk.prefer(), repeater)?;
    }
    // Unreachable by the bound above. Left as an answer rather than a panic:
    // an invariant that holds by construction is not worth ending a run over.
    None
}

/// Set `timestamp_next` on every repeating task to its next still-upcoming
/// occurrence relative to `now`.
///
/// Runs on the *input* tasks, before the agenda is built. That ordering is
/// load-bearing: `push_scheduled_occurrence` / `push_overdue_occurrence`
/// rewrite `timestamp_date` to the occurrence they render, and a monthly
/// repeater anchored on the 31st would lose its day-of-month if the field
/// were computed from that rewritten date (`bracket_month` truncates 31.01 to
/// 28.02 and, restarted from there, never climbs back). Annotating once per
/// task also means the value is identical in every cell the task appears in --
/// the field names the next occurrence relative to now, not to the cell.
///
/// Callers skip this for `tasks` scope: that mode is deliberately date-less
/// (ADR-0009), so a now-relative field would make its output non-deterministic.
///
/// Non-repeating tasks and tasks whose date or repeater cannot be parsed are
/// left untouched, so the field is absent from their JSON.
fn annotate_next_occurrences(
    tasks: &mut [Task],
    now: NaiveDateTime,
    exceptions: &OccurrenceExceptions,
) {
    for task in tasks {
        let excluded = exceptions.dates_for(task);
        set_next_occurrence(task, now, &excluded);
    }
}

/// Fill one task's `timestamp_next`; see [`annotate_next_occurrences`].
fn set_next_occurrence(task: &mut Task, now: NaiveDateTime, excluded: &ExcludedOccurrences) {
    use crate::timestamp::parse_repeater;

    let (Some(date_str), Some(rep_str)) = (
        task.timestamp_date.as_deref(),
        task.timestamp_repeater.as_deref(),
    ) else {
        return;
    };
    let (Some(base), Some(rep)) = (
        NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok(),
        parse_repeater(rep_str),
    ) else {
        // Both are already-validated shapes coming out of the parser, so a
        // failure here means the wire format drifted -- worth a trace, not a
        // silent skip.
        tracing::debug!(
            file = %task.file,
            line = task.line,
            date = date_str,
            repeater = rep_str,
            "timestamp_next skipped: date or repeater did not parse"
        );
        return;
    };
    let time = task
        .timestamp_time
        .as_deref()
        .and_then(|t| NaiveTime::parse_from_str(t, "%H:%M").ok());
    if time.is_none() && task.timestamp_time.is_some() {
        // An unparsable clock time silently disables the "the slot already
        // passed today" rule, so the field would name a past slot as next.
        tracing::debug!(
            file = %task.file,
            line = task.line,
            time = task.timestamp_time.as_deref().unwrap_or_default(),
            "timestamp_next: clock time did not parse, treating the occurrence as all-day"
        );
    }
    task.timestamp_next =
        next_occurrence(base, &rep, time, now, excluded).map(|d| d.format("%Y-%m-%d").to_string());
    if task.timestamp_next.is_none() {
        tracing::debug!(
            file = %task.file,
            line = task.line,
            date = date_str,
            repeater = rep_str,
            "timestamp_next: repeater could not bracket an upcoming date"
        );
    }
}

fn build_day_agenda(
    tasks: &[Task],
    day_date: NaiveDate,
    current_date: NaiveDate,
    exceptions: &OccurrenceExceptions,
) -> DayAgenda {
    let prepared = prepare_tasks(tasks, exceptions);
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
                handle_repeating_task(
                    task,
                    parsed,
                    repeater,
                    day_date,
                    current_date,
                    &entry.excluded,
                    &mut agenda,
                );
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
    } else if days_diff < 0 && is_today && !is_done && keeps_a_missed_date(task) {
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

/// Whether a date that has passed leaves anything behind.
///
/// Only planning keywords do, and upstream splits the collection three ways to
/// say so (ADR-0012; read against org-agenda.el at 6916affed). A plain
/// timestamp is gathered by `org-agenda-get-timestamps`, whose regexp is built
/// to "match timestamps set to current date, timestamps with a repeater, and
/// S-exp timestamps" and which skips "date ranges, scheduled and deadlines,
/// which are handled specially" -- so it produces an entry on the day being
/// drawn and on no other. The reminder about a date gone by belongs to
/// `org-agenda-get-scheduled`, which formats it with the `Sched.%2dx:` leader
/// "when the item is scheduled on the current day ... due to automatic
/// rescheduling of unfinished items for the following day", and to
/// `org-agenda-get-deadlines` beside it.
///
/// Hence: a lesson held last Monday is not owed to anybody on Tuesday, and a
/// weekly series set up a year ago is not a year of arrears.
///
/// CLOSED is inactive and never reaches the agenda at all (ADR-0014), so the
/// two keywords below are the whole of it.
fn keeps_a_missed_date(task: &Task) -> bool {
    matches!(
        task.timestamp_type.as_deref(),
        Some("SCHEDULED") | Some("DEADLINE")
    )
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

/// The occurrence after the one being drawn: what `timestamp_next_after`
/// carries.
///
/// Read from the task's own date rather than from `day_date`, for the reason
/// [`annotate_next_occurrences`] gives — a monthly repeater anchored on the
/// 31st is truncated to 28.02 by `bracket_month` and never climbs back once
/// restarted from there. The clock time plays no part: the question is which
/// day comes next, and the day being drawn is excluded by asking from the day
/// after it.
fn next_after_day(
    base: NaiveDate,
    repeater: &crate::timestamp::Repeater,
    day_date: NaiveDate,
    excluded: &ExcludedOccurrences,
) -> Option<String> {
    let after = day_date.succ_opt()?;

    next_occurrence(
        base,
        repeater,
        None,
        after.and_time(NaiveTime::MIN),
        excluded,
    )
    .map(|date| date.format("%Y-%m-%d").to_string())
}

/// Put a repeating task into the cell of the day it recurs on.
///
/// The cell holds a *copy* whose `timestamp_date` and `timestamp` are rewritten
/// to the occurrence being drawn, not the task's own date. Anything that has to
/// be read from that own date — `next_after_day` here, `timestamp_next` in
/// `annotate_next_occurrences` — must therefore be computed from `parsed`
/// or before this runs; reading it back off the copy gives the cell's date and,
/// for a monthly repeater anchored past the 28th, silently walks the series
/// down the month.
fn push_scheduled_occurrence(
    task: &Task,
    parsed: &crate::timestamp::ParsedTimestamp,
    repeater: &crate::timestamp::Repeater,
    day_date: NaiveDate,
    excluded: &ExcludedOccurrences,
    agenda: &mut DayAgenda,
) {
    let mut task_copy = task.clone();
    // Filled in only where `annotate_next_occurrences` ran: both fields serve
    // the same consumer and only the JSON renderer prints either, so the
    // Markdown and HTML paths -- which pass `annotate_next = false` and leave
    // `timestamp_next` empty -- never reach the computation below.
    if task.timestamp_next.is_some() {
        task_copy.timestamp_next_after = next_after_day(parsed.date, repeater, day_date, excluded);
    }
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
    excluded: &ExcludedOccurrences,
    agenda: &mut DayAgenda,
) {
    use crate::timestamp::closest_date;

    let base_date = parsed.date;
    let is_today = day_date == current_date;

    // An occurrence that was cancelled or moved is not owed. Which one is
    // owed instead depends on why it is missing, and that is what
    // `Walk::Arrears` reads apart (ADR-0032).
    let deadline = skip_excluded(
        base_date,
        repeater,
        closest_date(base_date, current_date, DatePreference::Past, repeater),
        excluded,
        Walk::Arrears,
    );
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
    let day_is_excluded = excluded.contains(&day_date);
    if let Some(repeat_date) = repeat {
        if day_date == repeat_date && !day_is_excluded {
            push_scheduled_occurrence(task, parsed, repeater, day_date, excluded, agenda);
            shown_on_day = true;
        }
    }
    if !shown_on_day
        && !day_is_excluded
        && deadline.is_none()
        && current_date < base_date
        && day_date == base_date
    {
        push_scheduled_occurrence(task, parsed, repeater, day_date, excluded, agenda);
    }

    // DONE tasks and CLOSED-typed timestamps never appear in overdue or
    // upcoming (mirrors upstream Org-mode org-agenda.el lines 6424-6428 for
    // DONE, and the :closed/:deadline entry-type split at line 5571 for
    // CLOSED). Occurrence-day scheduling above is unaffected; that matches
    // the default of `org-agenda-skip-deadline-if-done` (nil), which still
    // shows the DONE task on its actual deadline date.
    //
    // Neither does a plain timestamp, repeater or not -- see
    // [`keeps_a_missed_date`]. A weekly class recurs on its day and leaves
    // nothing behind on the six others.
    let is_done = matches!(task.task_type, Some(TaskType::Done));

    if is_today && !is_done && keeps_a_missed_date(task) {
        let day = RepeatingDay {
            task,
            parsed,
            repeater,
            base_date,
            current_date,
            excluded,
        };
        push_overdue_if_owed(&day, deadline, agenda);
        push_upcoming_deadline(&day, repeat, agenda);
    }
}

/// One repeating entry as the day pass has worked it out, handed to the two
/// buckets that borrow it into today. Kept together rather than passed as
/// eight arguments: every field below is read by both.
struct RepeatingDay<'a> {
    task: &'a Task,
    parsed: &'a crate::timestamp::ParsedTimestamp,
    repeater: &'a crate::timestamp::Repeater,
    /// The date the entry's own timestamp carries, which anchors the series.
    base_date: NaiveDate,
    current_date: NaiveDate,
    excluded: &'a ExcludedOccurrences,
}

/// The arrears of a repeating entry, drawn into today.
///
/// `deadline` is the occurrence the arrears walk stopped at (`Walk::Arrears`),
/// so what is decided here is only whether it is behind and whether today is a
/// day this repeater speaks about at all.
fn push_overdue_if_owed(
    day: &RepeatingDay<'_>,
    deadline: Option<NaiveDate>,
    agenda: &mut DayAgenda,
) {
    let Some(deadline_date) = deadline else {
        return;
    };
    if deadline_date >= day.current_date {
        return;
    }
    // A workday repeater says nothing on a day that is not a workday: the
    // arrears of "every working day" are not owed on a Sunday.
    if day.repeater.unit == crate::timestamp::RepeaterUnit::Workday {
        use crate::holidays::HolidayCalendar;
        if !HolidayCalendar::global().is_workday(day.current_date) {
            return;
        }
    }
    push_overdue_occurrence(
        day.task,
        day.repeater,
        deadline_date,
        day.current_date,
        agenda,
    );
}

/// A repeating DEADLINE coming due, drawn into today ahead of its date.
///
/// `repeat` is `closest_date(..., DatePreference::Past, ...)` anchored on
/// today, so when it is `Some` the series already has an occurrence behind it
/// — never a candidate for this bucket. The only way a repeating DEADLINE
/// produces an upcoming entry is when there is no past occurrence yet and the
/// base date itself is still ahead.
fn push_upcoming_deadline(
    day: &RepeatingDay<'_>,
    repeat: Option<NaiveDate>,
    agenda: &mut DayAgenda,
) {
    if day.task.timestamp_type.as_deref() != Some("DEADLINE") {
        return;
    }
    if repeat.is_some() || day.current_date >= day.base_date {
        return;
    }
    // The step every other pass takes, rather than a filter: a cancelled first
    // occurrence leaves the series standing, and what is coming up is the next
    // one that does. Dropping it here would contradict `timestamp_next`, which
    // names that occurrence in the same payload.
    let Some(next_date) = skip_excluded(
        day.base_date,
        day.repeater,
        Some(day.base_date),
        day.excluded,
        Walk::Upcoming,
    ) else {
        return;
    };

    let days_diff = (next_date - day.current_date).num_days();
    let window = day.parsed.warning_days.unwrap_or(DEADLINE_WARNING_DAYS);
    if days_diff <= 0 || days_diff > window {
        return;
    }

    let mut task_copy = day.task.clone();
    task_copy.timestamp_time = None;
    task_copy.timestamp_end_time = None;
    agenda.upcoming.push(TaskWithOffset {
        task: task_copy,
        days_offset: Some(days_diff),
    });
}

/// Build agenda for a range of days (week or month). Pre-parses every task's
/// timestamp once and reuses it across all days in the range.
fn build_week_agenda(
    tasks: &[Task],
    start_date: NaiveDate,
    end_date: NaiveDate,
    current_date: NaiveDate,
    exceptions: &OccurrenceExceptions,
) -> Vec<DayAgenda> {
    let prepared = prepare_tasks(tasks, exceptions);
    let mut result = Vec::new();
    let mut current = start_date;

    while current <= end_date {
        result.push(build_day_agenda_prepared(&prepared, current, current_date));
        // `succ_opt` rather than `+ 1 day`: the last day chrono can represent
        // is a legitimate end of a window, and stepping past it must end the
        // walk instead of panicking.
        let Some(next) = current.succ_opt() else {
            break;
        };
        current = next;
    }

    result
}

/// Read `--week-start` into the weekday a week begins on.
///
/// `None` in the answer is upstream's `nil`: the week has no fixed first day
/// and begins wherever the anchor stands. `None` in the argument — the flag
/// left out — is Monday, the week every window produced before the flag
/// existed.
fn parse_week_start(value: Option<&str>) -> Result<Option<Weekday>, AppError> {
    let Some(raw) = value else {
        return Ok(Some(Weekday::Mon));
    };
    let name = raw.trim();
    if name.eq_ignore_ascii_case("today") {
        return Ok(None);
    }
    // `Weekday::from_str` takes the three-letter abbreviations as well as the
    // full names and ignores case, so `mon`, `Monday` and `MONDAY` all land
    // here.
    name.parse::<Weekday>().map(Some).map_err(|_| {
        AppError::DateRange(format!(
            "week-start '{raw}' is not a weekday name or 'today'"
        ))
    })
}

/// A grid draws one column per weekday, so its columns need a weekday to begin
/// on; a week anchored on the rendered day leaves them undefined. Refused
/// rather than quietly read as Monday: a caller that asked for an anchored week
/// would silently get a different grid than the one it asked for.
fn month_grid_needs_a_first_day() -> AppError {
    AppError::DateRange(
        "month-grid needs a fixed first day of the week: --week-start today has none".to_string(),
    )
}

/// Get week boundaries for a specific date: seven days beginning on
/// `week_start`, or on `date` itself when the week has no fixed first day.
///
/// A week that would run off either end of the calendar is clamped to it
/// rather than taken down with it: `+`/`-` on `NaiveDate` panic on overflow,
/// and the library takes its window arguments from embedders that have no
/// year bounds of their own (the CLI's `--date` validator refuses anything
/// outside 1900..=2100, but nothing stops a UniFFI caller from passing the
/// last day chrono can represent).
fn get_week_for_date(date: NaiveDate, week_start: Option<Weekday>) -> (NaiveDate, NaiveDate) {
    let start = match week_start {
        Some(first) => {
            let offset = date.weekday().days_since(first) as i64;
            chrono::TimeDelta::try_days(offset)
                .and_then(|delta| date.checked_sub_signed(delta))
                .unwrap_or(NaiveDate::MIN)
        }
        None => date,
    };
    let end = chrono::TimeDelta::try_days(6)
        .and_then(|delta| start.checked_add_signed(delta))
        .unwrap_or(NaiveDate::MAX);
    (start, end)
}

/// Widen `[start, end]` to the whole weeks it touches, beginning on
/// `week_start`. A range already aligned on both edges comes back unchanged,
/// so the answer is always a whole number of weeks.
fn grow_to_whole_weeks(
    start: NaiveDate,
    end: NaiveDate,
    week_start: Weekday,
) -> (NaiveDate, NaiveDate) {
    let (grown_start, _) = get_week_for_date(start, Some(week_start));
    let (_, grown_end) = get_week_for_date(end, Some(week_start));
    (grown_start, grown_end)
}

/// Get the boundaries of the grid a calendar month is drawn on: the whole
/// weeks that the month falls in, beginning on `week_start`.
///
/// A month whose edges already land on the edges of a week borrows nothing —
/// February 2027 from a Monday is four rows, not six — so the answer is the
/// weeks the month touches rather than a fixed count of them.
fn get_month_grid_for_date(date: NaiveDate, week_start: Weekday) -> (NaiveDate, NaiveDate) {
    let (first_day, last_day) = get_month_for_date(date);
    grow_to_whole_weeks(first_day, last_day, week_start)
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
    use crate::timestamp::parse_repeater;
    use crate::types::CancelledSpelling;
    use chrono::TimeZone;

    // Helpers for the `next_occurrence` tests below.
    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }
    fn dt(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> NaiveDateTime {
        ymd(y, m, d).and_hms_opt(hh, mm, 0).unwrap()
    }
    fn hm(hh: u32, mm: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hh, mm, 0).unwrap()
    }

    /// A run with no exception in it, which is what almost every test wants.
    fn no_exceptions() -> ExcludedOccurrences {
        ExcludedOccurrences::default()
    }

    /// The day and the week builders with the run's exception index supplied
    /// from the same task list, which is what `filter_agenda` does. Both
    /// shadow the functions of the module above so a test reads the way it did
    /// before the index became a parameter.
    fn build_day_agenda(tasks: &[Task], day_date: NaiveDate, current_date: NaiveDate) -> DayAgenda {
        super::build_day_agenda(
            tasks,
            day_date,
            current_date,
            &OccurrenceExceptions::from_tasks(tasks),
        )
    }

    fn build_week_agenda(
        tasks: &[Task],
        start_date: NaiveDate,
        end_date: NaiveDate,
        current_date: NaiveDate,
    ) -> Vec<DayAgenda> {
        super::build_week_agenda(
            tasks,
            start_date,
            end_date,
            current_date,
            &OccurrenceExceptions::from_tasks(tasks),
        )
    }

    #[test]
    fn next_occurrence_rolls_a_past_date_forward() {
        // Weekly ++7d anchored on Tue 21.07; "now" is Fri 24.07 -> next is 28.07.
        let rep = parse_repeater("++7d").unwrap();
        let next = next_occurrence(
            ymd(2026, 7, 21),
            &rep,
            None,
            dt(2026, 7, 24, 10, 0),
            &no_exceptions(),
        );
        assert_eq!(next, Some(ymd(2026, 7, 28)));
    }

    #[test]
    fn next_occurrence_monthly_rolls_month_by_month() {
        let rep = parse_repeater("++1m").unwrap();
        let next = next_occurrence(
            ymd(2026, 1, 15),
            &rep,
            None,
            dt(2026, 7, 24, 10, 0),
            &no_exceptions(),
        );
        assert_eq!(next, Some(ymd(2026, 8, 15)));
    }

    #[test]
    fn next_occurrence_all_day_today_stays_today_even_late() {
        // An untimed occurrence on today is still "next" late in the evening.
        let rep = parse_repeater("++1w").unwrap();
        let next = next_occurrence(
            ymd(2026, 7, 24),
            &rep,
            None,
            dt(2026, 7, 24, 22, 3),
            &no_exceptions(),
        );
        assert_eq!(next, Some(ymd(2026, 7, 24)));
    }

    #[test]
    fn next_occurrence_timed_today_before_its_time_stays_today() {
        let rep = parse_repeater("++7d").unwrap();
        let next = next_occurrence(
            ymd(2026, 7, 24),
            &rep,
            Some(hm(14, 0)),
            dt(2026, 7, 24, 9, 0),
            &no_exceptions(),
        );
        assert_eq!(next, Some(ymd(2026, 7, 24)));
    }

    #[test]
    fn next_occurrence_timed_today_after_its_time_rolls_forward() {
        // 24.07 14:00 viewed at 22:03 -> the slot passed today, next is 31.07.
        let rep = parse_repeater("++7d").unwrap();
        let next = next_occurrence(
            ymd(2026, 7, 24),
            &rep,
            Some(hm(14, 0)),
            dt(2026, 7, 24, 22, 3),
            &no_exceptions(),
        );
        assert_eq!(next, Some(ymd(2026, 7, 31)));
    }

    #[test]
    fn next_occurrence_future_anchor_is_returned_as_is() {
        let rep = parse_repeater("++7d").unwrap();
        let next = next_occurrence(
            ymd(2026, 8, 12),
            &rep,
            None,
            dt(2026, 7, 24, 22, 3),
            &no_exceptions(),
        );
        assert_eq!(next, Some(ymd(2026, 8, 12)));
    }

    #[test]
    fn next_occurrence_treats_all_three_repeater_kinds_alike() {
        // `+`, `++` and `.+` differ in how Org advances the *stored* stamp on
        // completion; `timestamp_next` only answers "when does it next come
        // round", which is the same catch-up date for all three. Pinned so a
        // future change to that rule is a deliberate one.
        for kind in ["+7d", "++7d", ".+7d"] {
            let rep = parse_repeater(kind).expect(kind);
            let next = next_occurrence(
                ymd(2026, 7, 21),
                &rep,
                None,
                dt(2026, 7, 24, 10, 0),
                &no_exceptions(),
            );
            assert_eq!(next, Some(ymd(2026, 7, 28)), "repeater kind {kind}");
        }
    }

    #[test]
    fn next_occurrence_workday_unit_skips_the_weekend() {
        // `wd` counts working days, so from Fri 24.07 the next one-workday
        // occurrence is Mon 27.07, not Sat 25.07.
        let rep = parse_repeater("++1wd").unwrap();
        let next = next_occurrence(
            ymd(2026, 7, 24),
            &rep,
            None,
            dt(2026, 7, 25, 10, 0),
            &no_exceptions(),
        );
        assert_eq!(next, Some(ymd(2026, 7, 27)));
    }

    #[test]
    fn next_occurrence_yearly_unit_rolls_to_the_next_anniversary() {
        let rep = parse_repeater("++1y").unwrap();
        let next = next_occurrence(
            ymd(2020, 3, 9),
            &rep,
            None,
            dt(2026, 7, 24, 10, 0),
            &no_exceptions(),
        );
        assert_eq!(next, Some(ymd(2027, 3, 9)));
    }

    #[test]
    fn next_occurrence_hourly_unit_projects_onto_the_day_grid() {
        // Hour repeaters are projected onto the daily grid (the value is
        // ignored, see the repeater docs), so a past slot today rolls to
        // tomorrow rather than to the next slot within today.
        let rep = parse_repeater("+2h").unwrap();
        let next = next_occurrence(
            ymd(2026, 7, 24),
            &rep,
            Some(hm(9, 0)),
            dt(2026, 7, 24, 10, 0),
            &no_exceptions(),
        );
        assert_eq!(next, Some(ymd(2026, 7, 25)));
    }

    /// Build a repeating SCHEDULED task straight from its anchor and repeater.
    fn repeating_task(date_str: &str, repeater: &str, time: Option<&str>) -> Task {
        let mut task = create_test_task(date_str, time, TaskType::Todo);
        task.timestamp_repeater = Some(repeater.to_string());
        task.timestamp = Some(match time {
            Some(t) => format!("SCHEDULED: <{date_str} {t} {repeater}>"),
            None => format!("SCHEDULED: <{date_str} {repeater}>"),
        });
        task
    }

    /// Every `timestamp_next` in a day/week/month payload, across all buckets.
    fn collect_next(output: &AgendaOutput) -> Vec<Option<String>> {
        match output {
            AgendaOutput::Days(days) => days
                .iter()
                .flat_map(|day| {
                    day.overdue
                        .iter()
                        .chain(&day.scheduled_timed)
                        .chain(&day.scheduled_no_time)
                        .chain(&day.upcoming)
                })
                .map(|item| item.task.timestamp_next.clone())
                .collect(),
            AgendaOutput::Tasks(tasks) => tasks.iter().map(|t| t.timestamp_next.clone()).collect(),
        }
    }

    /// A series with an `ID`, so a replacement has something to name.
    fn series_task(date_str: &str, repeater: &str, time: Option<&str>, id: &str) -> Task {
        let mut task = repeating_task(date_str, repeater, time);
        let mut props = std::collections::BTreeMap::new();
        props.insert(crate::exceptions::ID_KEY.to_string(), id.to_string());
        task.properties = Some(props);
        task
    }

    /// The entry that stands in for one occurrence of `series_id`.
    fn replacement_task(date_str: &str, time: &str, series_id: &str, recurrence: &str) -> Task {
        let mut task = create_test_task(date_str, Some(time), TaskType::Todo);
        task.heading = "English, moved".to_string();
        task.file = "moved.md".to_string();
        task.series_id = Some(series_id.to_string());
        task.recurrence_id = Some(recurrence.to_string());
        task
    }

    /// The headings drawn on the days they are dated to, in order.
    ///
    /// The arrears and the deadlines coming up are left out on purpose: they
    /// are copies borrowed into today, and a test about which occurrences a
    /// series has should read the cells the series is drawn in.
    fn collect_scheduled_headings(output: &AgendaOutput) -> Vec<String> {
        match output {
            AgendaOutput::Days(days) => days
                .iter()
                .flat_map(|day| day.scheduled_timed.iter().chain(&day.scheduled_no_time))
                .map(|item| item.task.heading.clone())
                .collect(),
            AgendaOutput::Tasks(tasks) => tasks.iter().map(|t| t.heading.clone()).collect(),
        }
    }

    fn week_from(tasks: Vec<Task>, current: &str) -> AgendaOutput {
        filter_agenda(
            tasks,
            AgendaScope::Week,
            AgendaDates {
                current_date: Some(current),
                date: Some(current),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda")
    }

    #[test]
    fn an_excluded_date_drops_that_occurrence_and_keeps_the_rest() {
        // Weekly class on Thursdays; the 20th is cancelled (ADR-0031).
        let mut task = repeating_task("2026-08-13 Thu", "+1w", Some("15:00"));
        task.heading = "English".to_string();
        task.excluded_dates = Some(vec!["2026-08-20".to_string()]);

        let output = week_from(vec![task], "2026-08-17");

        let AgendaOutput::Days(days) = &output else {
            panic!("week scope must produce days");
        };
        let drawn: Vec<String> = days
            .iter()
            .filter(|day| !day.scheduled_timed.is_empty())
            .map(|day| day.date.clone())
            .collect();
        assert!(
            drawn.is_empty(),
            "the week of the 17th holds only the excluded occurrence, so nothing is drawn: {drawn:?}"
        );
    }

    #[test]
    fn a_replacement_takes_the_place_of_the_occurrence_it_names() {
        let series = series_task("2026-08-13 Thu", "+1w", Some("15:00"), "series-1");
        let moved = replacement_task("2026-08-20 Thu", "18:00", "series-1", "2026-08-20 15:00");

        let headings = collect_scheduled_headings(&week_from(vec![series, moved], "2026-08-17"));

        assert_eq!(
            headings,
            vec!["English, moved".to_string()],
            "the day must hold the replacement alone, not both it and the series"
        );
    }

    #[test]
    fn a_replacement_of_another_series_does_not_disturb_this_one() {
        let series = series_task("2026-08-13 Thu", "+1w", Some("15:00"), "series-1");
        let mut moved = replacement_task("2026-08-20 Thu", "18:00", "series-2", "2026-08-20 15:00");
        moved.heading = "Someone else's class".to_string();

        let headings = collect_scheduled_headings(&week_from(vec![series, moved], "2026-08-17"));

        assert_eq!(
            headings,
            vec!["Test task".to_string(), "Someone else's class".to_string()],
            "an unrelated replacement leaves the series occurrence standing"
        );
    }

    #[test]
    fn the_next_occurrence_steps_over_an_excluded_one() {
        let mut task = repeating_task("2026-08-13 Thu", "+1w", Some("15:00"));
        task.excluded_dates = Some(vec!["2026-08-20".to_string()]);

        let output = filter_agenda(
            vec![task],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-08-17"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        assert_eq!(
            collect_next(&output),
            vec![Some("2026-08-27".to_string())],
            "the 20th is not an occurrence, so the next one is the 27th"
        );
    }

    #[test]
    fn arrears_are_owed_from_the_last_occurrence_that_still_stands() {
        // Scheduled weekly from the 6th; the 20th was cancelled, so on the
        // 24th what is owed is the 13th, not the 20th.
        let mut task = repeating_task("2026-08-06 Thu", "+1w", None);
        task.heading = "Water the flowers".to_string();
        task.excluded_dates = Some(vec!["2026-08-20".to_string()]);

        let output = filter_agenda(
            vec![task],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-08-24"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = &output else {
            panic!("day scope must produce days");
        };
        let owed: Vec<Option<i64>> = days[0].overdue.iter().map(|o| o.days_offset).collect();
        assert_eq!(
            owed,
            vec![Some(-11)],
            "the debt is the 13th (eleven days back), the 20th having been cancelled"
        );
    }

    #[test]
    fn a_cancelled_deadline_warns_about_the_occurrence_that_follows_it() {
        // A repeating DEADLINE whose first occurrence is cancelled still has a
        // series: what comes up is the next occurrence that stands, and the
        // warning is about that one. Falling silent would contradict
        // `timestamp_next`, which names it in the same payload.
        let mut task =
            create_test_task_with_type("2026-08-25 Tue", None, TaskType::Todo, "DEADLINE");
        task.heading = "Pay the rent".to_string();
        task.timestamp_repeater = Some("+1w".to_string());
        task.timestamp = Some("DEADLINE: <2026-08-25 Tue +1w>".to_string());
        task.excluded_dates = Some(vec!["2026-08-25".to_string()]);

        let output = filter_agenda(
            vec![task],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-08-22"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = &output else {
            panic!("day scope must produce days");
        };
        let coming: Vec<Option<i64>> = days[0].upcoming.iter().map(|u| u.days_offset).collect();
        assert_eq!(
            coming,
            vec![Some(10)],
            "the 25th is cancelled, so what is coming up is the 1st of September, ten days out"
        );
    }

    #[test]
    fn the_debt_of_a_moved_occurrence_travels_with_the_entry_that_replaced_it() {
        // Weekly from the 6th; the 20th was moved to the 22nd. On the 24th the
        // series owes nothing — the occurrence took place, two days ago, in
        // the entry that replaced it — and the arrears must not roll back past
        // it to the 13th, which would make the series look older than before
        // the move (ADR-0031: a replacement is not a cancellation).
        let series = series_task("2026-08-06 Thu", "+1w", None, "series-1");
        let moved = replacement_task("2026-08-22 Sat", "18:00", "series-1", "2026-08-20");

        let output = filter_agenda(
            vec![series, moved],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-08-24"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = &output else {
            panic!("day scope must produce days");
        };
        let owed: Vec<(String, Option<i64>)> = days[0]
            .overdue
            .iter()
            .map(|o| (o.task.heading.clone(), o.days_offset))
            .collect();
        assert_eq!(
            owed,
            vec![("English, moved".to_string(), Some(-2))],
            "only the replacement is owed; the series must not be owed for an occurrence that moved"
        );
    }

    /// The occurrences one entry misses, as a run works them out: the dates it
    /// cancels itself, and the ones entries of their own stand in for.
    fn exceptions(cancelled: &[&str], replaced: &[&str]) -> ExcludedOccurrences {
        let mut series = series_task("2026-08-13 Thu", "+1w", None, "series-1");
        series.excluded_dates = Some(cancelled.iter().map(|d| (*d).to_string()).collect());
        let run: Vec<Task> = replaced
            .iter()
            .map(|date| replacement_task(date, "18:00", "series-1", date))
            .collect();
        OccurrenceExceptions::from_tasks(&run).dates_for(&series)
    }

    #[test]
    fn a_run_of_cancelled_occurrences_is_walked_past_in_one_answer() {
        // Two Thursdays in a row are cancelled. One answer is all an entry
        // has -- `timestamp_next` is a single field -- so the walk has to step
        // over both of them in it; stopping at the first exception would name
        // a date the series does not have.
        let rep = parse_repeater("+1w").unwrap();
        let next = next_occurrence(
            ymd(2026, 8, 13),
            &rep,
            None,
            dt(2026, 8, 17, 10, 0),
            &exceptions(&["2026-08-20", "2026-08-27"], &[]),
        );
        assert_eq!(next, Some(ymd(2026, 9, 3)));
    }

    #[test]
    fn a_cancelled_occurrence_and_a_moved_one_are_both_walked_past() {
        // The two reasons part ways over a debt, not over whether the day
        // holds the series (ADR-0032). Forward, each of them takes its
        // occurrence out, and the answer is the first one that still stands --
        // however the run happens to mix them.
        let rep = parse_repeater("+1w").unwrap();
        let next = next_occurrence(
            ymd(2026, 8, 13),
            &rep,
            None,
            dt(2026, 8, 17, 10, 0),
            &exceptions(&["2026-08-20"], &["2026-08-27"]),
        );
        assert_eq!(next, Some(ymd(2026, 9, 3)));
    }

    #[test]
    fn a_cancelled_date_the_series_never_falls_on_changes_nothing() {
        // `EXDATE` is written by hand and can name a Saturday of a series that
        // runs on Thursdays. Such a date widens the walk and must do nothing
        // else: the next Thursday is still the next Thursday.
        let rep = parse_repeater("+1w").unwrap();
        let next = next_occurrence(
            ymd(2026, 8, 13),
            &rep,
            None,
            dt(2026, 8, 17, 10, 0),
            &exceptions(&["2026-08-22"], &[]),
        );
        assert_eq!(next, Some(ymd(2026, 8, 20)));
    }

    #[test]
    fn a_long_run_of_cancelled_occurrences_still_names_the_one_that_follows() {
        // The walk is bounded by the number of exceptions, and the bound has
        // to be one step wider than that number: ten cancelled Thursdays take
        // ten steps, and the eleventh candidate is the answer. A bound short
        // by one would answer with nothing here, and the entry would lose its
        // next occurrence instead of skipping the ones it does not have.
        let dates: Vec<String> = (0..10)
            .map(|week| {
                (ymd(2026, 8, 20) + chrono::Duration::weeks(week))
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .collect();
        let cancelled: Vec<&str> = dates.iter().map(String::as_str).collect();
        let rep = parse_repeater("+1w").unwrap();
        let next = next_occurrence(
            ymd(2026, 8, 13),
            &rep,
            None,
            dt(2026, 8, 17, 10, 0),
            &exceptions(&cancelled, &[]),
        );
        assert_eq!(next, Some(ymd(2026, 10, 29)));
    }

    #[test]
    fn arrears_are_walked_back_over_every_occurrence_that_was_cancelled() {
        // Two cancelled Thursdays in a row: what is owed on the 24th is the
        // 6th, the last occurrence that still stands, and not the later of the
        // two dates the series does not have.
        let mut task = repeating_task("2026-08-06 Thu", "+1w", None);
        task.heading = "Water the flowers".to_string();
        task.excluded_dates = Some(vec!["2026-08-13".to_string(), "2026-08-20".to_string()]);

        let output = filter_agenda(
            vec![task],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-08-24"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = &output else {
            panic!("day scope must produce days");
        };
        let owed: Vec<Option<i64>> = days[0].overdue.iter().map(|o| o.days_offset).collect();
        assert_eq!(
            owed,
            vec![Some(-18)],
            "the debt is the 6th, both Thursdays after it having been cancelled"
        );
    }

    #[test]
    fn timestamp_next_anchors_on_the_task_date_not_the_rendered_occurrence() {
        // The agenda rewrites `timestamp_date` to the occurrence it renders
        // (push_scheduled_occurrence / push_overdue_occurrence). Anchoring
        // `timestamp_next` on that rewritten value loses the anchor's
        // day-of-month: bracket_month truncates 31.01 -> 28.02 and, restarted
        // from there, never climbs back to the 31st. The field must be
        // computed from the task's own anchor, so a monthly repeater keeps
        // naming month-end.
        let task = repeating_task("2026-01-31 Sat", "++1m", None);

        let output = filter_agenda(
            vec![task],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-07-25"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        assert_eq!(
            collect_next(&output),
            vec![Some("2026-07-31".to_string())],
            "monthly repeater anchored on the 31st must keep naming month-end"
        );
    }

    #[test]
    fn timestamp_next_is_identical_across_every_day_of_a_week_payload() {
        // The field is "next occurrence relative to now", not "this cell's
        // date": one task must carry the same value in every cell it appears
        // in, whatever bucket the cell puts it in.
        let task = repeating_task("2026-07-21 Tue", "++7d", None);

        let output = filter_agenda(
            vec![task],
            AgendaScope::Week,
            AgendaDates {
                current_date: Some("2026-07-22"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let values = collect_next(&output);
        assert!(!values.is_empty(), "week payload must contain the task");
        assert!(
            values.iter().all(|v| v.as_deref() == Some("2026-07-28")),
            "expected every cell to carry 2026-07-28, got {values:?}"
        );
    }

    #[test]
    fn timestamp_next_is_absent_without_a_repeater() {
        let output = filter_agenda(
            vec![create_test_task("2026-07-25 Sat", None, TaskType::Todo)],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-07-25"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        assert_eq!(collect_next(&output), vec![None]);
    }

    #[test]
    fn timestamp_next_is_absent_when_the_repeater_cannot_be_parsed() {
        let output = filter_agenda(
            vec![repeating_task("2026-07-25 Sat", "++0d", None)],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-07-25"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        assert!(
            collect_next(&output).iter().all(Option::is_none),
            "a rejected repeater must leave the field absent, not guess a date"
        );
    }

    #[test]
    fn tasks_scope_is_never_annotated() {
        // ADR-0023: the date-less flat list stays deterministic, so the
        // now-relative field is not injected there.
        let output = filter_agenda(
            vec![repeating_task("2026-07-21 Tue", "++7d", None)],
            AgendaScope::Tasks,
            AgendaDates::default(),
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        assert_eq!(collect_next(&output), vec![None]);
    }

    #[test]
    fn annotate_next_false_skips_the_field_entirely() {
        // Markdown / HTML never print `timestamp_next`, so main passes false
        // and the per-task repeater arithmetic is skipped. The rest of the
        // payload must be unaffected.
        let output = filter_agenda(
            vec![repeating_task("2026-07-21 Tue", "++7d", None)],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-07-22"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            false,
        )
        .expect("filter_agenda");

        let values = collect_next(&output);
        assert!(!values.is_empty(), "the task must still be rendered");
        assert!(
            values.iter().all(Option::is_none),
            "no cell may carry the field when annotation is off; got {values:?}"
        );
    }

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
            task_type: Some(task_type),
            timestamp: Some(timestamp.clone()),
            timestamp_type: Some(ts_type.to_string()),
            timestamp_active: Some(true),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            timestamp_time: time.map(|t| t.to_string()),
            ..Task::default()
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
            task_type: Some(TaskType::Todo),
            timestamp: Some(timestamp.to_string()),
            timestamp_type: Some("PLAIN".to_string()),
            timestamp_active: Some(active),
            timestamp_date: Some(date_str.to_string()),
            ..Task::default()
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
    fn a_plain_timestamp_that_has_passed_is_not_overdue() {
        // Upstream shows a plain timestamp "exactly on that date": an event
        // that took place is not owed to anybody afterwards. Only SCHEDULED
        // and DEADLINE carry a missed date forward.
        let tasks = vec![create_test_plain_task("<2026-08-10 Mon>", "2026-08-10")];
        let today = NaiveDate::from_ymd_opt(2026, 8, 16).unwrap();
        let agenda = build_day_agenda(&tasks, today, today);
        assert!(
            agenda.overdue.is_empty(),
            "a past event must not be reported as overdue"
        );
    }

    #[test]
    fn a_weekly_class_is_not_a_year_of_arrears() {
        // The case this rule was written for: a series set up long ago, kept
        // as a plain repeating timestamp. On a day between occurrences it
        // says nothing; on its own weekday it is on the agenda as usual.
        let tasks = vec![create_test_plain_task(
            "<2025-09-01 Mon 19:00 +1w>",
            "2025-09-01",
        )];
        let between = NaiveDate::from_ymd_opt(2026, 8, 16).unwrap();
        let on_the_day = NaiveDate::from_ymd_opt(2026, 8, 17).unwrap();

        // Which of the two scheduled buckets the occurrence lands in is the
        // hour's business, and the fixture carries no `timestamp_time` of its
        // own; what is asserted here is that the day has it and the others
        // do not.
        let shown = |a: &DayAgenda| a.scheduled_timed.len() + a.scheduled_no_time.len();

        let quiet = build_day_agenda(&tasks, between, between);
        assert!(quiet.overdue.is_empty(), "a class held weekly owes nothing");
        assert_eq!(shown(&quiet), 0, "no class on a Sunday");

        let held = build_day_agenda(&tasks, on_the_day, on_the_day);
        assert_eq!(shown(&held), 1, "the class is on on Mondays");
        assert!(held.overdue.is_empty());
    }

    #[test]
    fn a_scheduled_date_that_has_passed_is_still_overdue() {
        // The counterpart guard: narrowing overdue to the planning keywords
        // must not empty it. Upstream forwards a scheduled entry day after
        // day until it is marked done.
        let tasks = vec![create_test_task("2026-08-10", None, TaskType::Todo)];
        let today = NaiveDate::from_ymd_opt(2026, 8, 16).unwrap();
        let agenda = build_day_agenda(&tasks, today, today);
        assert_eq!(agenda.overdue.len(), 1);
        assert_eq!(agenda.overdue[0].days_offset, Some(-6));
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
            task_type: Some(TaskType::Todo),
            priority: prio,
            timestamp: Some("SCHEDULED: <2024-12-05 Thu>".to_string()),
            timestamp_type: Some("SCHEDULED".to_string()),
            timestamp_active: Some(true),
            timestamp_date: Some("2024-12-05".to_string()),
            ..Task::default()
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
            task_type: Some(task_type),
            timestamp: Some(timestamp.clone()),
            timestamp_type: Some("SCHEDULED".to_string()),
            timestamp_active: Some(true),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            timestamp_time: time.map(|t| t.to_string()),
            ..Task::default()
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
            task_type: Some(task_type),
            timestamp: Some(timestamp.clone()),
            timestamp_type: Some("DEADLINE".to_string()),
            timestamp_active: Some(true),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            timestamp_time: time.map(|t| t.to_string()),
            ..Task::default()
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
            false,
            true,
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
    fn tasks_scope_orders_one_priority_by_date_then_time() {
        // The flat list has no date axis, so within a priority the order used
        // to be whatever the walk over the files produced -- unspecified, and
        // read by a consumer as "these are unordered". A date and a time are
        // the only things a reader can order this list by, so they come first,
        // with the file and the line left as the tiebreaker that keeps two
        // runs over the same tree identical. A whole-day task sorts after the
        // timed ones of its day, as org-agenda reads a timeless entry (99:01,
        // `org-agenda-sort-notime-is-late`).
        let mut later_day = create_test_task("2024-12-06 Fri", Some("08:00"), TaskType::Todo);
        later_day.heading = "second day, early".to_string();

        let mut same_day_late = create_test_task("2024-12-05 Thu", Some("14:00"), TaskType::Todo);
        same_day_late.heading = "first day, afternoon".to_string();

        let mut same_day_early = create_test_task("2024-12-05 Thu", Some("09:30"), TaskType::Todo);
        same_day_early.heading = "first day, morning".to_string();

        let mut same_day_no_time = create_test_task("2024-12-05 Thu", None, TaskType::Todo);
        same_day_no_time.heading = "first day, all day".to_string();

        // Mixed input order so the assertion proves the sort is doing the work.
        let input = vec![
            same_day_late.clone(),
            later_day.clone(),
            same_day_no_time.clone(),
            same_day_early.clone(),
        ];

        let result = filter_agenda(
            input,
            AgendaScope::Tasks,
            AgendaDates::default(),
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let tasks = match result {
            AgendaOutput::Tasks(tasks) => tasks,
            other => panic!("expected AgendaOutput::Tasks, got {other:?}"),
        };
        let headings: Vec<&str> = tasks.iter().map(|t| t.heading.as_str()).collect();
        assert_eq!(
            headings,
            vec![
                "first day, morning",
                "first day, afternoon",
                "first day, all day",
                "second day, early",
            ],
            "one day comes before the next, and inside a day the hours come before the whole day"
        );
    }

    #[test]
    fn tasks_scope_puts_a_dateless_task_after_every_dated_one() {
        // A task with no timestamp at all cannot take a place on the date
        // order, and putting it first would push the work a reader can act on
        // down the list. It goes last, where the priority group ends.
        let mut dated = create_test_task("2024-12-05 Thu", None, TaskType::Todo);
        dated.heading = "dated".to_string();

        let mut dateless = create_test_task("2024-12-05 Thu", None, TaskType::Todo);
        dateless.heading = "dateless".to_string();
        dateless.timestamp = None;
        dateless.timestamp_type = None;
        dateless.timestamp_active = None;
        dateless.timestamp_date = None;

        let result = filter_agenda(
            vec![dateless.clone(), dated.clone()],
            AgendaScope::Tasks,
            AgendaDates::default(),
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let tasks = match result {
            AgendaOutput::Tasks(tasks) => tasks,
            other => panic!("expected AgendaOutput::Tasks, got {other:?}"),
        };
        let headings: Vec<&str> = tasks.iter().map(|t| t.heading.as_str()).collect();
        assert_eq!(headings, vec!["dated", "dateless"]);
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
            false,
            true,
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
            false,
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
    fn tasks_scope_excludes_cancelled_by_default() {
        // CANCELLED is excluded from the flat list unless explicitly opted in,
        // mirroring the DONE default. Neither include flag set here.
        let input = vec![
            create_test_task("2024-12-05 Wed", None, TaskType::Todo),
            create_test_task(
                "2024-12-06 Thu",
                None,
                TaskType::Cancelled(CancelledSpelling::DoubleL),
            ),
        ];
        let result = filter_agenda(
            input,
            AgendaScope::Tasks,
            AgendaDates::default(),
            "UTC",
            false,
            false,
            true,
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
    fn tasks_scope_includes_cancelled_when_requested() {
        // With include_cancelled set, CANCELLED appears alongside TODO so a
        // consumer can delete the calendar event for a cancelled task. DONE is
        // NOT pulled in by this flag (the two opt-ins are independent).
        let input = vec![
            create_test_task("2024-12-05 Wed", None, TaskType::Todo),
            create_test_task("2024-12-06 Thu", None, TaskType::Done),
            create_test_task(
                "2024-12-07 Fri",
                None,
                TaskType::Cancelled(CancelledSpelling::DoubleL),
            ),
        ];
        let result = filter_agenda(
            input,
            AgendaScope::Tasks,
            AgendaDates::default(),
            "UTC",
            false, // include_done off — DONE must stay out
            true,  // include_cancelled on
            true,
        )
        .expect("filter_agenda");
        let tasks = match result {
            AgendaOutput::Tasks(tasks) => tasks,
            other => panic!("expected AgendaOutput::Tasks, got {other:?}"),
        };
        assert_eq!(tasks.len(), 2, "TODO and CANCELLED present, DONE excluded");
        assert!(
            tasks
                .iter()
                .any(|t| matches!(t.task_type, Some(TaskType::Todo))),
            "TODO must be present"
        );
        assert!(
            tasks
                .iter()
                .any(|t| matches!(t.task_type, Some(TaskType::Cancelled(_)))),
            "CANCELLED must be present when include_cancelled is set"
        );
        assert!(
            !tasks
                .iter()
                .any(|t| matches!(t.task_type, Some(TaskType::Done))),
            "DONE must stay excluded: include_cancelled is independent of include_done"
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
            task_type: Some(task_type),
            timestamp: Some(timestamp),
            timestamp_type: Some(ts_type.to_string()),
            timestamp_active: Some(true),
            timestamp_date: Some(date_str.split_whitespace().next().unwrap().to_string()),
            ..Task::default()
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

    // ---- week start and the month grid ----

    #[test]
    fn week_start_defaults_to_monday() {
        // No `--week-start` is the behaviour every existing caller has, so it
        // has to stay the Monday-to-Sunday week the scope always produced.
        let (start, end) = get_week_for_date(ymd(2026, 8, 19), Some(Weekday::Mon));
        assert_eq!((start, end), (ymd(2026, 8, 17), ymd(2026, 8, 23)));
    }

    #[test]
    fn week_start_sunday_moves_both_edges_back_a_day() {
        let (start, end) = get_week_for_date(ymd(2026, 8, 19), Some(Weekday::Sun));
        assert_eq!((start, end), (ymd(2026, 8, 16), ymd(2026, 8, 22)));
    }

    #[test]
    fn week_start_today_begins_the_window_on_the_anchor() {
        // Upstream's `org-agenda-start-on-weekday` set to nil: the seven days
        // start where the reader is rather than on a fixed weekday.
        let (start, end) = get_week_for_date(ymd(2026, 8, 19), None);
        assert_eq!((start, end), (ymd(2026, 8, 19), ymd(2026, 8, 25)));
    }

    #[test]
    fn month_grid_fills_out_the_weeks_the_month_touches() {
        // August 2026 opens on a Saturday and closes on a Monday, so a grid of
        // whole Monday weeks borrows five days from July and six from
        // September: 27.07 through 06.09, six rows of seven.
        let (start, end) = get_month_grid_for_date(ymd(2026, 8, 12), Weekday::Mon);
        assert_eq!((start, end), (ymd(2026, 7, 27), ymd(2026, 9, 6)));
        assert_eq!((end - start).num_days() + 1, 42);
    }

    #[test]
    fn month_grid_follows_the_first_day_of_the_week() {
        let (start, end) = get_month_grid_for_date(ymd(2026, 8, 12), Weekday::Sun);
        assert_eq!((start, end), (ymd(2026, 7, 26), ymd(2026, 9, 5)));
    }

    #[test]
    fn month_grid_borrows_nothing_from_a_month_that_fills_its_weeks() {
        // February 2027 runs Monday to Sunday: four rows and no borrowed days,
        // which is the case a grid hard-coded to six rows would pad wrongly.
        let (start, end) = get_month_grid_for_date(ymd(2027, 2, 10), Weekday::Mon);
        assert_eq!((start, end), (ymd(2027, 2, 1), ymd(2027, 2, 28)));
        assert_eq!((end - start).num_days() + 1, 28);
    }

    #[test]
    fn parse_week_start_reads_a_weekday_or_the_anchor() {
        assert_eq!(parse_week_start(None).unwrap(), Some(Weekday::Mon));
        assert_eq!(
            parse_week_start(Some("sunday")).unwrap(),
            Some(Weekday::Sun)
        );
        assert_eq!(
            parse_week_start(Some("MONDAY")).unwrap(),
            Some(Weekday::Mon)
        );
        assert_eq!(parse_week_start(Some("today")).unwrap(), None);
        assert!(parse_week_start(Some("payday")).is_err());
    }

    #[test]
    fn month_grid_scope_returns_every_day_of_the_grid() {
        let output = filter_agenda(
            vec![create_test_task("2026-08-12 Wed", None, TaskType::Todo)],
            AgendaScope::MonthGrid,
            AgendaDates {
                current_date: Some("2026-08-12"),
                date: Some("2026-08-12"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = output else {
            panic!("month-grid scope must produce days");
        };
        assert_eq!(days.len(), 42);
        assert_eq!(days.first().map(|d| d.date.as_str()), Some("2026-07-27"));
        assert_eq!(days.last().map(|d| d.date.as_str()), Some("2026-09-06"));
    }

    #[test]
    fn month_grid_scope_refuses_an_anchor_week_start() {
        // Columns of a calendar are a fixed weekday each; a week that starts
        // wherever the reader stands has no columns to draw, so the
        // combination is refused rather than quietly read as Monday.
        let err = filter_agenda(
            vec![],
            AgendaScope::MonthGrid,
            AgendaDates {
                current_date: Some("2026-08-12"),
                week_start: Some("today"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect_err("month-grid must refuse an anchored week start");
        assert!(
            err.to_string().contains("week-start"),
            "the message must name the argument at fault, got: {err}"
        );
    }

    #[test]
    fn a_window_at_the_end_of_the_calendar_is_clamped_not_panicked() {
        // The CLI refuses a year outside 1900..=2100, but an embedder calling
        // the library directly has no such guard: a week anchored on the last
        // day chrono can represent must come back clamped rather than take the
        // process down on the `+ 6 days` that closes it.
        let output = filter_agenda(
            vec![],
            AgendaScope::Week,
            AgendaDates {
                date: Some("+262142-12-31"),
                current_date: Some("+262142-12-31"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = output else {
            panic!("week scope must produce days");
        };
        assert_eq!(
            days.last().map(|d| d.date.as_str()),
            Some("+262142-12-31"),
            "the window stops at the last representable day"
        );
    }

    #[test]
    fn month_grid_scope_grows_an_explicit_range_to_whole_weeks() {
        // A grid is laid out in rows of seven, so an explicit window is grown
        // to the weeks it touches rather than drawn as a ragged row: Wed 5 Aug
        // through Tue 11 Aug covers two weeks, Mon 3 Aug through Sun 16 Aug.
        let output = filter_agenda(
            vec![],
            AgendaScope::MonthGrid,
            AgendaDates {
                current_date: Some("2026-08-12"),
                from: Some("2026-08-05"),
                to: Some("2026-08-11"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = output else {
            panic!("month-grid scope must produce days");
        };
        assert_eq!(days.len(), 14, "two whole weeks");
        assert_eq!(days.first().map(|d| d.date.as_str()), Some("2026-08-03"));
        assert_eq!(days.last().map(|d| d.date.as_str()), Some("2026-08-16"));
    }

    #[test]
    fn month_grid_scope_grows_an_explicit_range_from_its_own_week_start() {
        // The same window read from Sunday lands a day earlier at both edges,
        // and still covers whole weeks.
        let output = filter_agenda(
            vec![],
            AgendaScope::MonthGrid,
            AgendaDates {
                current_date: Some("2026-08-12"),
                from: Some("2026-08-05"),
                to: Some("2026-08-11"),
                week_start: Some("sunday"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = output else {
            panic!("month-grid scope must produce days");
        };
        assert_eq!(days.len(), 14, "two whole weeks");
        assert_eq!(days.first().map(|d| d.date.as_str()), Some("2026-08-02"));
        assert_eq!(days.last().map(|d| d.date.as_str()), Some("2026-08-15"));
    }

    #[test]
    fn month_grid_scope_leaves_an_aligned_range_alone() {
        // A window that already begins and ends on the edges of a week is the
        // grid it asked for: growing it would add a row nobody requested.
        let output = filter_agenda(
            vec![],
            AgendaScope::MonthGrid,
            AgendaDates {
                current_date: Some("2026-08-12"),
                from: Some("2026-08-03"),
                to: Some("2026-08-09"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = output else {
            panic!("month-grid scope must produce days");
        };
        assert_eq!(days.len(), 7);
        assert_eq!(days.first().map(|d| d.date.as_str()), Some("2026-08-03"));
        assert_eq!(days.last().map(|d| d.date.as_str()), Some("2026-08-09"));
    }

    /// The date each scheduled cell carries, paired with the occurrence it
    /// says comes after it.
    fn collect_dated_next_after(output: &AgendaOutput) -> Vec<(String, Option<String>)> {
        let AgendaOutput::Days(days) = output else {
            panic!("expected a dated payload");
        };
        days.iter()
            .flat_map(|day| day.scheduled_timed.iter().chain(&day.scheduled_no_time))
            .map(|item| {
                (
                    item.task.timestamp_date.clone().unwrap_or_default(),
                    item.task.timestamp_next_after.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn next_after_names_the_occurrence_following_the_cell() {
        // The tooltip question is "when is the next one after the day I am
        // reading", so each cell has to answer for itself rather than repeat
        // the now-relative `timestamp_next`.
        let output = filter_agenda(
            vec![repeating_task("2026-08-17 Mon", "+1d", Some("12:00"))],
            AgendaScope::Week,
            AgendaDates {
                current_date: Some("2026-08-17"),
                date: Some("2026-08-17"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let pairs = collect_dated_next_after(&output);
        assert_eq!(
            pairs
                .iter()
                .find(|(date, _)| date == "2026-08-18")
                .map(|(_, next)| next.as_deref()),
            Some(Some("2026-08-19")),
            "the cell of the 18th must name the 19th, not today; got {pairs:?}"
        );
        assert_eq!(
            pairs
                .iter()
                .find(|(date, _)| date == "2026-08-17")
                .map(|(_, next)| next.as_deref()),
            Some(Some("2026-08-18"))
        );
    }

    #[test]
    fn next_after_steps_over_an_occurrence_the_series_does_not_have() {
        // ADR-0031 names three passes that have to agree on which occurrences
        // a series has: the day cells, `timestamp_next`, and this one. It is
        // the only one of the three read per cell rather than per entry, and
        // the only one whose exception set is threaded down through
        // `push_scheduled_occurrence` -- so it is also the one where a
        // regression is invisible outside the JSON payload.
        //
        // Both reasons an occurrence can be missing are here: the 19th is
        // cancelled by the series itself, the 20th is taken by an entry of its
        // own. Neither is an occurrence, so the cell of the 18th names the
        // 21st.
        let mut series = series_task("2026-08-17 Mon", "+1d", Some("12:00"), "series-1");
        series.excluded_dates = Some(vec!["2026-08-19".to_string()]);
        let output = filter_agenda(
            vec![
                series,
                replacement_task("2026-08-20 Thu", "18:00", "series-1", "2026-08-20"),
            ],
            AgendaScope::Week,
            AgendaDates {
                current_date: Some("2026-08-17"),
                date: Some("2026-08-17"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let pairs = collect_dated_next_after(&output);
        assert_eq!(
            pairs
                .iter()
                .find(|(date, _)| date == "2026-08-18")
                .map(|(_, next)| next.as_deref()),
            Some(Some("2026-08-21")),
            "the cell of the 18th must step over both the 19th and the 20th; got {pairs:?}"
        );
    }

    #[test]
    fn next_after_keeps_month_end_for_a_monthly_repeater() {
        // Anchored on the 31st: computed from the rewritten date, February
        // would truncate the anchor to the 28th and never climb back.
        let output = filter_agenda(
            vec![repeating_task("2026-01-31 Sat", "++1m", None)],
            AgendaScope::Month,
            AgendaDates {
                current_date: Some("2026-03-01"),
                date: Some("2026-03-01"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let pairs = collect_dated_next_after(&output);
        assert_eq!(
            pairs
                .iter()
                .find(|(date, _)| date == "2026-03-31")
                .map(|(_, next)| next.as_deref()),
            Some(Some("2026-04-30")),
            "a monthly repeater must keep naming month-end; got {pairs:?}"
        );
    }

    #[test]
    fn next_after_is_absent_from_the_borrowed_buckets() {
        // Overdue and upcoming are copies borrowed into today's agenda; there
        // the question is still "next from now", which `timestamp_next`
        // answers, so the per-cell field stays out of them.
        let output = filter_agenda(
            vec![repeating_task("2026-08-10 Mon", "+1d", None)],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-08-17"),
                date: Some("2026-08-17"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = &output else {
            panic!("expected a dated payload");
        };
        assert!(
            days.iter()
                .flat_map(|day| day.overdue.iter().chain(&day.upcoming))
                .all(|item| item.task.timestamp_next_after.is_none()),
            "borrowed copies must not carry the per-cell field"
        );
    }

    #[test]
    fn next_after_is_absent_when_the_now_relative_pass_was_skipped() {
        // The Markdown and HTML renderers pass `annotate_next = false`: neither
        // prints either field, so neither is computed. A cell that carried the
        // occurrence after it here would mean the work was done for nothing.
        let output = filter_agenda(
            vec![create_test_task("2026-08-03 Mon +1w", None, TaskType::Todo)],
            AgendaScope::Week,
            AgendaDates {
                current_date: Some("2026-08-05"),
                date: Some("2026-08-05"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            false,
        )
        .expect("filter_agenda");

        let pairs = collect_dated_next_after(&output);
        assert!(
            !pairs.is_empty(),
            "the repeating task must still be drawn on its occurrence day"
        );
        assert!(
            pairs.iter().all(|(_, after)| after.is_none()),
            "no cell may carry the following occurrence, got: {pairs:?}"
        );
    }

    #[test]
    fn next_after_is_absent_without_a_repeater() {
        let output = filter_agenda(
            vec![create_test_task("2026-08-18 Tue", None, TaskType::Todo)],
            AgendaScope::Day,
            AgendaDates {
                current_date: Some("2026-08-18"),
                date: Some("2026-08-18"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        assert!(
            collect_dated_next_after(&output)
                .iter()
                .all(|(_, next)| next.is_none()),
            "a task with no repeater has no occurrence after this one"
        );
    }

    #[test]
    fn week_scope_honours_the_first_day_of_the_week() {
        let output = filter_agenda(
            vec![],
            AgendaScope::Week,
            AgendaDates {
                current_date: Some("2026-08-19"),
                week_start: Some("sunday"),
                ..AgendaDates::default()
            },
            "UTC",
            false,
            false,
            true,
        )
        .expect("filter_agenda");

        let AgendaOutput::Days(days) = output else {
            panic!("week scope must produce days");
        };
        assert_eq!(days.first().map(|d| d.date.as_str()), Some("2026-08-16"));
        assert_eq!(days.last().map(|d| d.date.as_str()), Some("2026-08-22"));
    }
}
