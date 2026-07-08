use crate::holidays::HolidayCalendar;
use chrono::NaiveDate;

/// Repeater type and interval
#[derive(Debug, Clone, PartialEq)]
pub struct Repeater {
    pub repeater_type: RepeaterType,
    pub value: u32,
    pub unit: RepeaterUnit,
}

/// Type of repeater (org-mode prefix)
#[derive(Debug, Clone, PartialEq)]
pub enum RepeaterType {
    /// `+` — Cumulative: next = base + N*step
    Cumulative,
    /// `++` — Catch-up: next = first base + N*step >= from
    CatchUp,
    /// `.+` — Restart: next = from + step (resets from completion)
    Restart,
}

impl RepeaterType {
    /// Org-mode prefix string (`+`, `++`, `.+`)
    pub fn prefix(&self) -> &'static str {
        match self {
            RepeaterType::Cumulative => "+",
            RepeaterType::CatchUp => "++",
            RepeaterType::Restart => ".+",
        }
    }
}

/// Repeater unit
#[derive(Debug, Clone, PartialEq)]
pub enum RepeaterUnit {
    Day,
    Week,
    Month,
    Year,
    /// `+Nh` — intra-day repeater. For the agenda-by-day view this projects
    /// onto a daily grid: every day is an occurrence regardless of the numeric
    /// value `N`. A hypothetical "+25h" therefore still shows up every day, not
    /// every other day. Documented behaviour, see `closest_date`.
    Hour,
    Workday,
}

impl RepeaterUnit {
    /// Org-mode suffix string (`d`, `w`, `m`, `y`, `h`, `wd`)
    pub fn suffix(&self) -> &'static str {
        match self {
            RepeaterUnit::Day => "d",
            RepeaterUnit::Week => "w",
            RepeaterUnit::Month => "m",
            RepeaterUnit::Year => "y",
            RepeaterUnit::Hour => "h",
            RepeaterUnit::Workday => "wd",
        }
    }
}

impl Repeater {
    /// Canonical org-mode repeater string: prefix + value + unit suffix
    /// (`++7d`, `.+1m`, `+1wd`). Round-trips with `parse_repeater`.
    pub fn canonical(&self) -> String {
        format!(
            "{}{}{}",
            self.repeater_type.prefix(),
            self.value,
            self.unit.suffix()
        )
    }
}

/// Parse repeater string like `+1d`, `++2w`, `.+1m`, `+1wd`
///
/// Returns `None` for malformed input or when the numeric value is zero
/// (zero-step repeaters cause division-by-zero in occurrence math).
///
/// At `trace` level, every rejection is logged with a specific reason so a
/// caller running with `-vvv` can tell `+1` (missing unit), `+1ф` (non-ASCII
/// unit), `+0d` (zero step) and `1d` (missing prefix) apart without rerunning.
pub fn parse_repeater(s: &str) -> Option<Repeater> {
    let s = s.trim();

    let (repeater_type, rest) = if let Some(r) = s.strip_prefix(".+") {
        (RepeaterType::Restart, r)
    } else if let Some(r) = s.strip_prefix("++") {
        (RepeaterType::CatchUp, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (RepeaterType::Cumulative, r)
    } else {
        tracing::trace!(input = %s, reason = "missing prefix", "parse_repeater_rejected");
        return None;
    };

    if rest.is_empty() {
        tracing::trace!(input = %s, reason = "empty after prefix", "parse_repeater_rejected");
        return None;
    }

    // Check for "wd" suffix first
    if let Some(value_str) = rest.strip_suffix("wd") {
        let value: u32 = match value_str.parse() {
            Ok(v) => v,
            Err(_) => {
                tracing::trace!(input = %s, reason = "non-numeric value for wd", "parse_repeater_rejected");
                return None;
            }
        };
        if value == 0 {
            tracing::trace!(input = %s, reason = "zero step for wd", "parse_repeater_rejected");
            return None;
        }
        return Some(Repeater {
            repeater_type,
            value,
            unit: RepeaterUnit::Workday,
        });
    }

    let unit_char = match rest.chars().last() {
        Some(c) => c,
        None => {
            tracing::trace!(input = %s, reason = "empty rest after wd check", "parse_repeater_rejected");
            return None;
        }
    };
    let value_str = &rest[..rest.len() - unit_char.len_utf8()];
    let value: u32 = match value_str.parse() {
        Ok(v) => v,
        Err(_) => {
            tracing::trace!(input = %s, reason = "non-numeric value", "parse_repeater_rejected");
            return None;
        }
    };
    if value == 0 {
        tracing::trace!(input = %s, reason = "zero step", "parse_repeater_rejected");
        return None;
    }

    let unit = match unit_char {
        'd' => RepeaterUnit::Day,
        'w' => RepeaterUnit::Week,
        'm' => RepeaterUnit::Month,
        'y' => RepeaterUnit::Year,
        'h' => RepeaterUnit::Hour,
        _ => {
            tracing::trace!(
                input = %s,
                unit_char = %unit_char,
                reason = "unknown unit",
                "parse_repeater_rejected"
            );
            return None;
        }
    };

    Some(Repeater {
        repeater_type,
        value,
        unit,
    })
}

/// Preference for closest date calculation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DatePreference {
    /// Return latest occurrence <= current, or `None` if no past occurrence exists
    Past,
    /// Return earliest occurrence >= current
    Future,
}

/// Select the occurrence side (`n1` or `n2`) that matches the requested
/// `prefer`ence, given the half-open bracket `n1 <= current < n2`.
///
/// The interval is right-open by construction (every `bracket_*` builder
/// returns `n2` strictly after `current`), matching upstream org-mode where
/// a period boundary belongs to the *next* period. So `current == n1` is the
/// reachable left edge (the occurrence itself), while `current == n2` never
/// occurs here — were it to, `Past` would already treat it as the next
/// period (`current >= n2` → `n2`) and `Future` would skip past `n1`. `Past`
/// returns the latest occurrence `<= current`; `Future` the earliest
/// `>= current` (F7, 2026-05-25 logic review).
fn pick(
    prefer: DatePreference,
    current: NaiveDate,
    n1: NaiveDate,
    n2: NaiveDate,
) -> Option<NaiveDate> {
    Some(match prefer {
        DatePreference::Past => {
            if current >= n2 {
                n2
            } else {
                n1
            }
        }
        DatePreference::Future => {
            if current <= n1 {
                n1
            } else {
                n2
            }
        }
    })
}

/// Bracket containing `current` on the year-repeater grid.
/// Returns `(latest occurrence <= current, earliest occurrence > current)`.
/// Skips truncations for Feb-29 by walking valid `from_ymd_opt` candidates.
fn bracket_year(
    base_date: NaiveDate,
    current: NaiveDate,
    value: u32,
) -> Option<(NaiveDate, NaiveDate)> {
    use chrono::Datelike;

    let value = value as i32;
    let base_month = base_date.month();
    let base_day = base_date.day();
    let base_year = base_date.year();
    let current_year = current.year();

    // Search for the latest valid occurrence on or before `current`.
    // For dates like Feb 29 we skip non-leap years entirely instead of truncating.
    let max_complete = (current_year - base_year) / value;
    let mut n1: Option<NaiveDate> = None;
    let mut k = max_complete;
    while k >= 0 {
        let y = base_year + k * value;
        if let Some(d) = NaiveDate::from_ymd_opt(y, base_month, base_day) {
            if d <= current {
                n1 = Some(d);
                break;
            }
        }
        k -= 1;
    }
    // n1 = None requires `current < base_date`, which `closest_date` rules out
    // before dispatching here. Assert in debug, return None in release as a
    // safe degradation rather than panicking on a malformed call.
    debug_assert!(
        n1.is_some(),
        "bracket_year: n1=None despite current >= base_date"
    );
    let n1 = n1?;

    // Next valid occurrence strictly after `current`.
    let mut k2 = (n1.year() - base_year) / value + 1;
    // Accommodate Feb-29 (gap up to 8 years). `max_complete` is >= 0 whenever
    // `current >= base_date`, which `closest_date` guarantees before
    // dispatching here, so `+ 200` is already a positive ceiling. The
    // `.max(0)` is defense-in-depth (F6, 2026-05-25 logic review): a direct
    // call with `current < base_date` would otherwise yield a negative
    // `max_complete` and a ceiling below `k2`, returning None silently
    // instead of looping with a sane bound.
    let safety_limit = max_complete.max(0) + 200;
    let n2 = loop {
        if k2 > safety_limit {
            return None;
        }
        let y = base_year + k2 * value;
        if let Some(d) = NaiveDate::from_ymd_opt(y, base_month, base_day) {
            if d > current {
                break d;
            }
        }
        k2 += 1;
    };

    Some((n1, n2))
}

/// Bracket on the month-repeater grid, truncating the day to fit the
/// destination month while always starting from `base_date` so that
/// `base_day` is preserved across truncations.
///
/// The returned pair satisfies the `pick` / `closest_date` invariant
/// `n1 <= current < n2`. The month-number difference alone does not
/// guarantee that: when `base_day` falls later in the month than
/// `current`'s day (or `base_day` is truncated to a short month),
/// `add_months(base, complete_months)` can land *after* `current`
/// inside `current`'s own month. In that case the occurrence in
/// `current`'s month is actually `n2`, so we step `complete_months`
/// back one full period. `add_months` already truncates the day, so we
/// reuse it directly instead of recomputing the truncation by hand.
fn bracket_month(
    base_date: NaiveDate,
    current: NaiveDate,
    value: u32,
) -> Option<(NaiveDate, NaiveDate)> {
    use chrono::Datelike;

    let months_to_add = value as i32;

    let months_diff = (current.year() - base_date.year()) * 12
        + (current.month() as i32 - base_date.month() as i32);
    let mut complete_months = (months_diff / months_to_add) * months_to_add;

    let mut n1 = add_months(base_date, complete_months)?;
    if n1 > current {
        // The occurrence in `current`'s month has not been reached yet;
        // the previous period is the latest occurrence on or before
        // `current`. Stepping back by one full period lands in a month
        // strictly earlier than `current`'s, so the invariant holds and
        // the date we just rejected becomes `n2`.
        complete_months -= months_to_add;
        n1 = add_months(base_date, complete_months)?;
    }
    debug_assert!(
        n1 <= current,
        "bracket_month: n1={n1} still after current={current} after step-back"
    );

    let n2 = add_months(base_date, complete_months + months_to_add)?;

    Some((n1, n2))
}

/// Bracket on a uniform daily grid (Day/Week/Hour repeaters).
/// `days` is the period length expressed in days.
fn bracket_uniform_days(
    base_date: NaiveDate,
    current: NaiveDate,
    days: i64,
) -> (NaiveDate, NaiveDate) {
    let days_diff = (current - base_date).num_days();
    let complete_periods = days_diff / days;

    let n1 = base_date + chrono::Duration::days(complete_periods * days);
    let n2 = n1 + chrono::Duration::days(days);
    (n1, n2)
}

/// Bracket on the workday-repeater grid using the calendar's O(log n)
/// workday-counting primitive instead of walking day-by-day.
fn bracket_workday(base_date: NaiveDate, current: NaiveDate, value: u32) -> (NaiveDate, NaiveDate) {
    let calendar = HolidayCalendar::global();
    let step = value as i64;

    let m = calendar.workdays_between_exclusive(base_date, current);
    let k = m / step;

    let n1 = if k == 0 {
        base_date
    } else {
        calendar.nth_workday_after(base_date, (k * step) as u64)
    };
    let n2 = calendar.nth_workday_after(n1, step as u64);
    (n1, n2)
}

/// Calculate closest occurrence date relative to `current` for the given repeater.
///
/// Contract:
/// - If `current == base_date`, returns `Some(base_date)`.
/// - If `current < base_date`:
///   - `Past` returns `None` (no past occurrence exists yet);
///   - `Future` returns `Some(base_date)` (first occurrence).
/// - Otherwise, returns the closest occurrence on or before / on or after `current`
///   according to `prefer`.
pub fn closest_date(
    base_date: NaiveDate,
    current: NaiveDate,
    prefer: DatePreference,
    repeater: &Repeater,
) -> Option<NaiveDate> {
    if current == base_date {
        return Some(base_date);
    }
    if current < base_date {
        return match prefer {
            DatePreference::Past => None,
            DatePreference::Future => Some(base_date),
        };
    }

    let (n1, n2) = match repeater.unit {
        RepeaterUnit::Year => bracket_year(base_date, current, repeater.value)?,
        RepeaterUnit::Month => bracket_month(base_date, current, repeater.value)?,
        RepeaterUnit::Day => bracket_uniform_days(base_date, current, repeater.value as i64),
        RepeaterUnit::Week => bracket_uniform_days(base_date, current, (repeater.value * 7) as i64),
        // Hour repeaters always project onto a daily grid: any +Nh repeater is
        // intra-day so for an agenda-by-day view every day is an occurrence.
        // The numeric value is intentionally ignored — see docstring on
        // `RepeaterUnit::Hour` and the README "Repeaters" section. Documented
        // explicitly so a future contributor does not "fix" this by using
        // `repeater.value`, which would silently turn +5h into "every 5 days".
        RepeaterUnit::Hour => bracket_uniform_days(base_date, current, 1),
        RepeaterUnit::Workday => bracket_workday(base_date, current, repeater.value),
    };

    pick(prefer, current, n1, n2)
}

/// Add `months` to a date, truncating the day to fit the destination month.
/// Constant-time (no per-month loops), correct for negative `months`.
pub fn add_months(date: NaiveDate, months: i32) -> Option<NaiveDate> {
    use chrono::Datelike;

    // Convert (year, 1..=12) into a 0-based "total months since year 0".
    let total = (date.year() as i64) * 12 + (date.month() as i64 - 1) + months as i64;
    let year = total.div_euclid(12);
    let month = (total.rem_euclid(12) + 1) as u32;
    let year: i32 = year.try_into().ok()?;

    let day = date.day().min(days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, day)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => unreachable!("invalid month: {month}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_workday_repeater() {
        let r = parse_repeater("+1wd").unwrap();
        assert_eq!(r.repeater_type, RepeaterType::Cumulative);
        assert_eq!(r.value, 1);
        assert_eq!(r.unit, RepeaterUnit::Workday);
    }

    #[test]
    fn test_parse_workday_repeater_multiple() {
        let r = parse_repeater("+2wd").unwrap();
        assert_eq!(r.value, 2);
        assert_eq!(r.unit, RepeaterUnit::Workday);
    }

    #[test]
    fn test_parse_workday_catchup() {
        let r = parse_repeater("++1wd").unwrap();
        assert_eq!(r.repeater_type, RepeaterType::CatchUp);
        assert_eq!(r.unit, RepeaterUnit::Workday);
    }

    #[test]
    fn prefix_check_order_distinguishes_catchup_from_cumulative() {
        // Regression guard for the prefix-stripping order in `parse_repeater`:
        // `++` must be matched before `+`. If the order were swapped, the
        // `+` arm would consume the first character of `++1d` and the
        // parser would silently classify the remainder as
        // `RepeaterType::Cumulative` with value 1 — same arithmetic, wrong
        // semantics (org-mode's CatchUp resets occurrences on completion).
        // The cheap explicit assertion below is what catches a refactor
        // that re-orders the strip_prefix arms.
        let cat = parse_repeater("++1d").expect("++1d must parse");
        assert_eq!(
            cat.repeater_type,
            RepeaterType::CatchUp,
            "++ must be matched before +; got Cumulative instead"
        );
        let cum = parse_repeater("+1d").expect("+1d must parse");
        assert_eq!(cum.repeater_type, RepeaterType::Cumulative);
        let res = parse_repeater(".+1d").expect(".+1d must parse");
        assert_eq!(res.repeater_type, RepeaterType::Restart);
    }

    #[test]
    fn test_parse_workday_restart() {
        let r = parse_repeater(".+1wd").unwrap();
        assert_eq!(r.repeater_type, RepeaterType::Restart);
        assert_eq!(r.unit, RepeaterUnit::Workday);
    }

    #[test]
    fn test_parse_regular_day() {
        let r = parse_repeater("+1d").unwrap();
        assert_eq!(r.unit, RepeaterUnit::Day);
    }

    #[test]
    fn test_parse_repeater_zero_rejected() {
        assert!(parse_repeater("+0d").is_none());
        assert!(parse_repeater("+0wd").is_none());
        assert!(parse_repeater("++0w").is_none());
        assert!(parse_repeater(".+0m").is_none());
    }

    #[test]
    fn test_parse_repeater_multibyte_last_char_no_panic() {
        // Last char is multibyte (Cyrillic / emoji). Must return None, not panic
        // on a byte-index-not-char-boundary slice.
        assert!(parse_repeater("+1й").is_none());
        assert!(parse_repeater("++2д").is_none());
        assert!(parse_repeater(".+3Й").is_none());
        assert!(parse_repeater("+1\u{1F600}").is_none());
    }

    #[test]
    fn test_parse_repeater_multibyte_in_value_no_panic() {
        // F4 (2026-05-25 logic review): the slice-safety guard at
        // `rest[..rest.len() - unit_char.len_utf8()]` only strips the *last*
        // char, so a multibyte char left in the *middle* of the value
        // (`+1ф5d`: ascii unit `d`, Cyrillic `ф` inside the digits) survives
        // into `value_str`. `u32::parse` then rejects it, so the function
        // returns None without panicking — but that path was not pinned. The
        // multibyte byte boundary must also not be mistaken for a slice index.
        assert!(parse_repeater("+1ф5d").is_none());
        assert!(parse_repeater("++2д3w").is_none());
        assert!(parse_repeater(".+1喜2y").is_none());
        // Cyrillic inside a workday value, ascii `wd` suffix.
        assert!(parse_repeater("+1ф2wd").is_none());
    }

    #[test]
    fn parse_repeater_rejects_each_failure_mode() {
        // Each rejection branch logs a distinct reason at trace-level; we cannot
        // observe the trace output here without a test subscriber, but at least
        // pin that every branch still returns None so a future refactor cannot
        // silently turn one of them into Some(...).
        // missing prefix
        assert!(parse_repeater("1d").is_none(), "no prefix");
        // empty after prefix
        assert!(parse_repeater("+").is_none(), "prefix only");
        // non-numeric wd value
        assert!(parse_repeater("+abwd").is_none(), "non-numeric wd");
        // unknown ASCII unit
        assert!(parse_repeater("+1q").is_none(), "unknown unit");
        // non-numeric value with valid unit
        assert!(parse_repeater("+abd").is_none(), "non-numeric value");
    }

    #[test]
    fn test_parse_year_repeater() {
        let r = parse_repeater("+1y").unwrap();
        assert_eq!(r.repeater_type, RepeaterType::Cumulative);
        assert_eq!(r.value, 1);
        assert_eq!(r.unit, RepeaterUnit::Year);
    }

    #[test]
    fn test_parse_hour_repeater() {
        let r = parse_repeater("+1h").unwrap();
        assert_eq!(r.repeater_type, RepeaterType::Cumulative);
        assert_eq!(r.value, 1);
        assert_eq!(r.unit, RepeaterUnit::Hour);
    }

    // --- Regression tests for fixed bugs ---

    #[test]
    fn test_closest_date_workday_value_2() {
        // base = Mon 2025-12-08, +2wd → 12-08, 12-10, 12-12, 12-16, 12-18, ...
        let base = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 2,
            unit: RepeaterUnit::Workday,
        };

        // current = Wed 12-10 should be on the grid
        let c1 = NaiveDate::from_ymd_opt(2025, 12, 10).unwrap();
        let past = closest_date(base, c1, DatePreference::Past, &repeater).unwrap();
        assert_eq!(past, c1, "+2wd: 12-10 must be an occurrence");

        // current = Thu 12-11 → past should be 12-10, future should be 12-12
        let c2 = NaiveDate::from_ymd_opt(2025, 12, 11).unwrap();
        let past = closest_date(base, c2, DatePreference::Past, &repeater).unwrap();
        let fut = closest_date(base, c2, DatePreference::Future, &repeater).unwrap();
        assert_eq!(past, NaiveDate::from_ymd_opt(2025, 12, 10).unwrap());
        assert_eq!(fut, NaiveDate::from_ymd_opt(2025, 12, 12).unwrap());
    }

    #[test]
    fn test_closest_date_hour_repeater_advances_daily() {
        // Hour repeater is projected onto daily grid; for +1h:
        // base = 2025-12-05, current = 2025-12-08 → past must be 2025-12-08 (every day is an occurrence)
        let base = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Hour,
        };
        let current = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let past = closest_date(base, current, DatePreference::Past, &repeater).unwrap();
        let fut = closest_date(base, current, DatePreference::Future, &repeater).unwrap();
        assert_eq!(past, current);
        assert_eq!(fut, current);
    }

    #[test]
    fn test_closest_date_hour_repeater_ignores_value() {
        // Documented behaviour: hour-repeaters project onto a daily grid; the
        // numeric value is irrelevant for agenda-by-day. +1h, +12h, and even
        // +25h all yield "every day is an occurrence" — this test locks the
        // semantics so a refactor that uses repeater.value can't slip past CI.
        let base = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let current = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        for value in [1u32, 5, 12, 25] {
            let repeater = Repeater {
                repeater_type: RepeaterType::Cumulative,
                value,
                unit: RepeaterUnit::Hour,
            };
            assert_eq!(
                closest_date(base, current, DatePreference::Past, &repeater),
                Some(current),
                "+{value}h Past must be current day"
            );
            assert_eq!(
                closest_date(base, current, DatePreference::Future, &repeater),
                Some(current),
                "+{value}h Future must be current day"
            );
        }
    }

    #[test]
    fn test_closest_date_year_value_greater_than_diff() {
        // base = 2025-01-01, +10y. current = 2025-12-05 → max_complete = 0,
        // n1 must be base (k=0 candidate), n2 must be 2035-01-01.
        let base = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 10,
            unit: RepeaterUnit::Year,
        };
        let current = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let past = closest_date(base, current, DatePreference::Past, &repeater).unwrap();
        let fut = closest_date(base, current, DatePreference::Future, &repeater).unwrap();
        assert_eq!(past, base, "+10y past from year-0 must stay on base");
        assert_eq!(fut, NaiveDate::from_ymd_opt(2035, 1, 1).unwrap());
    }

    #[test]
    fn test_closest_date_year_feb_29_skips_non_leap() {
        // base = 2024-02-29 (leap), +1y. For current = 2025-03-01,
        // last valid occurrence must be 2024-02-29 (NOT truncated 2025-02-28).
        let base = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Year,
        };
        let current = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();
        let past = closest_date(base, current, DatePreference::Past, &repeater).unwrap();
        assert_eq!(past, base, "Feb-29 must not be truncated to Feb-28");

        // Next occurrence after 2025 must be 2028-02-29
        let fut = closest_date(base, current, DatePreference::Future, &repeater).unwrap();
        assert_eq!(fut, NaiveDate::from_ymd_opt(2028, 2, 29).unwrap());
    }

    #[test]
    fn test_closest_date_month_n2_preserves_base_day() {
        // base = 2024-01-31, +1m. current = 2024-04-15.
        // complete_months = 3, n1 = 2024-04-30 (truncated). n2 must come from base + 4m = 2024-05-31.
        let base = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Month,
        };
        let current = NaiveDate::from_ymd_opt(2024, 4, 15).unwrap();
        let fut = closest_date(base, current, DatePreference::Future, &repeater).unwrap();
        // n1 = 2024-04-30 (truncated). 2024-04-15 < n1, so Future returns n1.
        assert_eq!(fut, NaiveDate::from_ymd_opt(2024, 4, 30).unwrap());

        // current = 2024-05-01 → n1 = 2024-04-30, n2 = 2024-05-31 (preserves base_day)
        let c2 = NaiveDate::from_ymd_opt(2024, 5, 1).unwrap();
        let fut2 = closest_date(base, c2, DatePreference::Future, &repeater).unwrap();
        assert_eq!(fut2, NaiveDate::from_ymd_opt(2024, 5, 31).unwrap());
    }

    #[test]
    fn test_closest_date_month_past_respects_invariant() {
        // Regression for F1 (2026-05-25 logic review): the `pick` /
        // `closest_date` contract guarantees `n1 <= current < n2`, so for
        // `Past` the answer must be the latest occurrence on or before
        // `current` — never a date strictly after it.
        //
        // base = 2024-01-31, +1m. Occurrences (with the project's
        // day-truncation semantics) are 2024-01-31, 02-29, 03-31, 04-30, …
        // For current = 2024-04-15 the latest occurrence on or before is
        // 2024-03-31, NOT the truncated April occurrence 2024-04-30 (which
        // is after current and was returned before the fix).
        let base = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Month,
        };
        let current = NaiveDate::from_ymd_opt(2024, 4, 15).unwrap();
        let past = closest_date(base, current, DatePreference::Past, &repeater).unwrap();
        assert_eq!(
            past,
            NaiveDate::from_ymd_opt(2024, 3, 31).unwrap(),
            "Past must be the last occurrence on or before current, not after it"
        );
        assert!(past <= current, "invariant n1 <= current violated");

        // Future from the same point is unchanged: the earliest occurrence on
        // or after 2024-04-15 is the truncated April date 2024-04-30.
        let fut = closest_date(base, current, DatePreference::Future, &repeater).unwrap();
        assert_eq!(fut, NaiveDate::from_ymd_opt(2024, 4, 30).unwrap());

        // A multi-month period (+3m) must also keep the invariant. base + 3m
        // grid lands on 2024-04-30; for current = 2024-04-15 the last
        // occurrence on or before is the base date 2024-01-31.
        let r3 = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 3,
            unit: RepeaterUnit::Month,
        };
        let past3 = closest_date(base, current, DatePreference::Past, &r3).unwrap();
        assert_eq!(
            past3, base,
            "+3m Past from 2024-04-15 must be the base 2024-01-31"
        );
        assert!(past3 <= current);
    }

    #[test]
    fn test_closest_date_current_before_base_past_returns_none() {
        let base = NaiveDate::from_ymd_opt(2025, 12, 10).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Day,
        };
        let current = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        assert!(closest_date(base, current, DatePreference::Past, &repeater).is_none());
        assert_eq!(
            closest_date(base, current, DatePreference::Future, &repeater),
            Some(base),
        );
    }

    /// Reference implementation of the workday `closest_date` using the
    /// original O(N) day-by-day walk. Used only as a test oracle to verify
    /// the optimized O(log N) version produces identical results.
    fn closest_date_workday_oracle(
        base_date: NaiveDate,
        current: NaiveDate,
        prefer: DatePreference,
        step: u32,
    ) -> Option<NaiveDate> {
        // F3 (2026-05-25 logic review): a zero step makes the inner
        // `for _ in 0..step` walk no day, so `next` never advances past
        // `current` and the outer `loop` spins forever. `parse_repeater`
        // already rejects `+0wd`, so production never reaches here with 0;
        // this guard stops a future test that passes 0 from hanging the
        // suite, failing loudly in debug instead.
        debug_assert!(step > 0, "workday oracle requires step > 0 to terminate");
        let calendar = crate::holidays::HolidayCalendar::global();
        if current == base_date {
            return Some(base_date);
        }
        if current < base_date {
            return match prefer {
                DatePreference::Past => None,
                DatePreference::Future => Some(base_date),
            };
        }
        let mut last_occurrence = base_date;
        loop {
            let mut next = last_occurrence;
            for _ in 0..step {
                next = calendar.next_workday(next);
            }
            if next > current {
                break;
            }
            last_occurrence = next;
        }
        let n1 = last_occurrence;
        let mut n2 = n1;
        for _ in 0..step {
            n2 = calendar.next_workday(n2);
        }
        match prefer {
            DatePreference::Past => {
                if current >= n2 {
                    Some(n2)
                } else {
                    Some(n1)
                }
            }
            DatePreference::Future => {
                if current <= n1 {
                    Some(n1)
                } else {
                    Some(n2)
                }
            }
        }
    }

    #[test]
    fn test_closest_date_workday_matches_oracle_across_2026() {
        // Sweep every day of 2026 against the slow oracle to make sure the
        // optimized O(log N) path produces identical results to the original
        // day-by-day walk. Covers all the holiday-cluster and weekend boundary
        // cases that exist in the bundled calendar.
        let base = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for step in [1u32, 2, 3, 5] {
            let repeater = Repeater {
                repeater_type: RepeaterType::Cumulative,
                value: step,
                unit: RepeaterUnit::Workday,
            };
            let mut day = base;
            let end = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
            while day <= end {
                for &prefer in &[DatePreference::Past, DatePreference::Future] {
                    let got = closest_date(base, day, prefer, &repeater);
                    let want = closest_date_workday_oracle(base, day, prefer, step);
                    assert_eq!(
                        got, want,
                        "mismatch at base={base} current={day} step={step} prefer={prefer:?}"
                    );
                }
                day += chrono::Duration::days(1);
            }
        }
    }

    #[test]
    fn test_closest_date_workday_handles_year_old_base() {
        // Regression for the O(N) workday loop: with a year-old base date the
        // optimized path must still land on the right grid point and return
        // it in well under the previous "hundreds of next_workday calls".
        let base = NaiveDate::from_ymd_opt(2025, 1, 13).unwrap(); // Mon, first workday of Jan 2025
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Workday,
        };
        let current = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(); // Mon
        let got = closest_date(base, current, DatePreference::Past, &repeater).unwrap();
        let want = closest_date_workday_oracle(base, current, DatePreference::Past, 1).unwrap();
        assert_eq!(got, want);
        assert!(got <= current);
    }

    #[test]
    fn test_workdays_between_exclusive_basic() {
        use crate::holidays::HolidayCalendar;
        let cal = HolidayCalendar::global();
        // Mon-Fri 2025-12-08..2025-12-12 → 5 workdays in (12-07, 12-12].
        let a = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap(); // Sun
        let b = NaiveDate::from_ymd_opt(2025, 12, 12).unwrap(); // Fri
        assert_eq!(cal.workdays_between_exclusive(a, b), 5);

        // Across Jan 2026 holidays: (2025-12-29, 2026-01-13]. Jan 1-9 are
        // holidays, Jan 10-11 weekend, Jan 12-13 workdays. Dec 29 Mon was
        // start; Dec 30 Tue, Dec 31 Wed are holidays per JSON (2025-12-31 in
        // data). Wait: only 2025-12-31 is a holiday, so Dec 30 Tue is a workday.
        let a = NaiveDate::from_ymd_opt(2025, 12, 29).unwrap();
        let b = NaiveDate::from_ymd_opt(2026, 1, 13).unwrap();
        // Manually: workdays in (Dec 29, Jan 13]:
        //   Dec 30 (Tue, workday), Dec 31 (Wed, holiday), Jan 1-9 (holidays),
        //   Jan 10-11 (weekend), Jan 12 (Mon, workday), Jan 13 (Tue, workday) = 3.
        assert_eq!(cal.workdays_between_exclusive(a, b), 3);
    }

    #[test]
    fn test_nth_workday_after_basic() {
        use crate::holidays::HolidayCalendar;
        let cal = HolidayCalendar::global();
        // From Sun 2025-12-07, the 1st workday after is Mon 2025-12-08.
        let base = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        assert_eq!(
            cal.nth_workday_after(base, 1),
            NaiveDate::from_ymd_opt(2025, 12, 8).unwrap()
        );
        // The 5th workday after Sun 2025-12-07 is Fri 2025-12-12.
        assert_eq!(
            cal.nth_workday_after(base, 5),
            NaiveDate::from_ymd_opt(2025, 12, 12).unwrap()
        );
        // The 6th workday after Sun 2025-12-07 skips the weekend → Mon 2025-12-15.
        assert_eq!(
            cal.nth_workday_after(base, 6),
            NaiveDate::from_ymd_opt(2025, 12, 15).unwrap()
        );
    }
}
