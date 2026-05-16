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

/// Parse repeater string like `+1d`, `++2w`, `.+1m`, `+1wd`
///
/// Returns `None` for malformed input or when the numeric value is zero
/// (zero-step repeaters cause division-by-zero in occurrence math).
pub fn parse_repeater(s: &str) -> Option<Repeater> {
    let s = s.trim();

    let (repeater_type, rest) = if let Some(r) = s.strip_prefix(".+") {
        (RepeaterType::Restart, r)
    } else if let Some(r) = s.strip_prefix("++") {
        (RepeaterType::CatchUp, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (RepeaterType::Cumulative, r)
    } else {
        return None;
    };

    if rest.is_empty() {
        return None;
    }

    // Check for "wd" suffix first
    if let Some(value_str) = rest.strip_suffix("wd") {
        let value: u32 = value_str.parse().ok()?;
        if value == 0 {
            return None;
        }
        return Some(Repeater {
            repeater_type,
            value,
            unit: RepeaterUnit::Workday,
        });
    }

    let unit_char = rest.chars().last()?;
    let value_str = &rest[..rest.len() - 1];
    let value: u32 = value_str.parse().ok()?;
    if value == 0 {
        return None;
    }

    let unit = match unit_char {
        'd' => RepeaterUnit::Day,
        'w' => RepeaterUnit::Week,
        'm' => RepeaterUnit::Month,
        'y' => RepeaterUnit::Year,
        'h' => RepeaterUnit::Hour,
        _ => return None,
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
    use chrono::Datelike;

    if current == base_date {
        return Some(base_date);
    }
    if current < base_date {
        return match prefer {
            DatePreference::Past => None,
            DatePreference::Future => Some(base_date),
        };
    }

    match repeater.unit {
        RepeaterUnit::Year => {
            let value = repeater.value as i32;
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
            let n1 = n1?;

            // Next valid occurrence strictly after `current`.
            let mut k2 = (n1.year() - base_year) / value + 1;
            let safety_limit = max_complete + 200; // accommodate Feb-29 (gap up to 8 years)
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
        RepeaterUnit::Month => {
            let months_to_add = repeater.value as i32;
            let base_day = base_date.day();

            let months_diff = (current.year() - base_date.year()) * 12
                + (current.month() as i32 - base_date.month() as i32);
            let complete_months = (months_diff / months_to_add) * months_to_add;

            // Compute n1 and n2 from base_date so that base_day is preserved across truncations
            let n1_raw = add_months(base_date, complete_months)?;
            let n1 = NaiveDate::from_ymd_opt(
                n1_raw.year(),
                n1_raw.month(),
                base_day.min(days_in_month(n1_raw.year(), n1_raw.month())),
            )?;

            let n2_raw = add_months(base_date, complete_months + months_to_add)?;
            let n2 = NaiveDate::from_ymd_opt(
                n2_raw.year(),
                n2_raw.month(),
                base_day.min(days_in_month(n2_raw.year(), n2_raw.month())),
            )?;

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
        RepeaterUnit::Day | RepeaterUnit::Week | RepeaterUnit::Hour => {
            // Hour repeaters are projected onto a daily grid for agenda purposes
            let days = match repeater.unit {
                RepeaterUnit::Day => repeater.value as i64,
                RepeaterUnit::Week => (repeater.value * 7) as i64,
                RepeaterUnit::Hour => 1,
                _ => unreachable!(),
            };

            let days_diff = (current - base_date).num_days();
            let complete_periods = days_diff / days;

            let n1 = base_date + chrono::Duration::days(complete_periods * days);
            let n2 = n1 + chrono::Duration::days(days);

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
        RepeaterUnit::Workday => {
            let calendar = HolidayCalendar::global();
            let step = repeater.value as i64;

            // Grid: base_date (k=0), then k>=1 means the (k*step)-th workday
            // strictly after base_date. Find the largest k with grid[k] <= current
            // by counting workdays in (base_date, current] once (O(log n)), instead
            // of walking the grid step-by-step (O(N) calendar days).
            let m = calendar.workdays_between_exclusive(base_date, current);
            let k = m / step;

            let n1 = if k == 0 {
                base_date
            } else {
                calendar.nth_workday_after(base_date, (k * step) as u64)
            };
            let n2 = calendar.nth_workday_after(n1, step as u64);

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
    }
}

/// Calculate next occurrence date for a repeater (CatchUp/Restart semantics)
#[allow(dead_code)]
pub fn next_occurrence(
    base_date: NaiveDate,
    repeater: &Repeater,
    from_date: NaiveDate,
) -> Option<NaiveDate> {
    if repeater.unit == RepeaterUnit::Workday {
        let calendar = HolidayCalendar::global();
        let step = repeater.value;

        match repeater.repeater_type {
            RepeaterType::Cumulative => {
                if from_date < base_date {
                    return Some(base_date);
                }
                // Walk on the grid until we strictly pass from_date
                let mut current = base_date;
                while current <= from_date {
                    for _ in 0..step {
                        current = calendar.next_workday(current);
                    }
                }
                Some(current)
            }
            RepeaterType::CatchUp | RepeaterType::Restart => {
                let mut current = from_date;
                for _ in 0..step {
                    current = calendar.next_workday(current);
                }
                Some(current)
            }
        }
    } else {
        match repeater.repeater_type {
            RepeaterType::Cumulative => match repeater.unit {
                RepeaterUnit::Month | RepeaterUnit::Year => {
                    let months_to_add = if repeater.unit == RepeaterUnit::Year {
                        (repeater.value * 12) as i32
                    } else {
                        repeater.value as i32
                    };

                    if from_date < base_date {
                        return Some(base_date);
                    }

                    let mut current = base_date;
                    while current <= from_date {
                        current = add_months(current, months_to_add)?;
                    }
                    Some(current)
                }
                _ => {
                    let days = match repeater.unit {
                        RepeaterUnit::Day => repeater.value as i64,
                        RepeaterUnit::Week => (repeater.value * 7) as i64,
                        RepeaterUnit::Hour => 1,
                        RepeaterUnit::Workday => unreachable!(),
                        _ => unreachable!(),
                    };

                    if from_date < base_date {
                        return Some(base_date);
                    }

                    let mut current = base_date;
                    while current <= from_date {
                        current += chrono::Duration::days(days);
                    }
                    Some(current)
                }
            },
            RepeaterType::CatchUp => {
                // CatchUp on the grid (base_date + N*step)
                match repeater.unit {
                    RepeaterUnit::Month => {
                        let months = repeater.value as i32;
                        let mut current = base_date;
                        while current <= from_date {
                            current = add_months(current, months)?;
                        }
                        Some(current)
                    }
                    RepeaterUnit::Year => {
                        let months = (repeater.value * 12) as i32;
                        let mut current = base_date;
                        while current <= from_date {
                            current = add_months(current, months)?;
                        }
                        Some(current)
                    }
                    _ => {
                        let days = match repeater.unit {
                            RepeaterUnit::Day => repeater.value as i64,
                            RepeaterUnit::Week => (repeater.value * 7) as i64,
                            RepeaterUnit::Hour => 1,
                            RepeaterUnit::Workday => unreachable!(),
                            _ => unreachable!(),
                        };
                        let mut current = base_date;
                        while current <= from_date {
                            current += chrono::Duration::days(days);
                        }
                        Some(current)
                    }
                }
            }
            RepeaterType::Restart => {
                let days = match repeater.unit {
                    RepeaterUnit::Day => repeater.value as i64,
                    RepeaterUnit::Week => (repeater.value * 7) as i64,
                    RepeaterUnit::Month => return add_months(from_date, repeater.value as i32),
                    RepeaterUnit::Year => {
                        return add_months(from_date, (repeater.value * 12) as i32)
                    }
                    RepeaterUnit::Hour => 1,
                    RepeaterUnit::Workday => unreachable!(),
                };
                Some(from_date + chrono::Duration::days(days))
            }
        }
    }
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
    fn test_next_occurrence_workday() {
        let base = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap(); // Friday
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Workday,
        };
        let from = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let next = next_occurrence(base, &repeater, from).unwrap();
        let expected = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap(); // Monday
        assert_eq!(next, expected);
    }

    #[test]
    fn test_next_occurrence_workday_skip_holidays() {
        let base = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(); // Monday in holidays
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Workday,
        };
        let from = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let next = next_occurrence(base, &repeater, from).unwrap();
        let expected = NaiveDate::from_ymd_opt(2026, 1, 12).unwrap(); // First workday after holidays
        assert_eq!(next, expected);
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

    #[test]
    fn test_next_occurrence_year() {
        let base = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Year,
        };
        let from = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let next = next_occurrence(base, &repeater, from).unwrap();
        let expected = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_next_occurrence_year_before_base() {
        let base = NaiveDate::from_ymd_opt(2025, 12, 11).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Year,
        };
        let from = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let next = next_occurrence(base, &repeater, from).unwrap();
        let expected = NaiveDate::from_ymd_opt(2025, 12, 11).unwrap();
        assert_eq!(
            next, expected,
            "Next occurrence should be base date when from < base"
        );
    }

    #[test]
    fn test_next_occurrence_month() {
        let base = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Month,
        };
        let from = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let next = next_occurrence(base, &repeater, from).unwrap();
        let expected = NaiveDate::from_ymd_opt(2025, 1, 5).unwrap();
        assert_eq!(next, expected);
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
    fn test_next_occurrence_cumulative_workday_value_2() {
        // base = Mon 2025-12-08, +2wd. from = Wed 12-10 → next should be Fri 12-12
        let base = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 2,
            unit: RepeaterUnit::Workday,
        };
        let from = NaiveDate::from_ymd_opt(2025, 12, 10).unwrap();
        let next = next_occurrence(base, &repeater, from).unwrap();
        assert_eq!(next, NaiveDate::from_ymd_opt(2025, 12, 12).unwrap());
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

    #[test]
    fn test_next_occurrence_catchup_week_with_value() {
        // base = 2024-12-01 Sun, ++2w. Occurrences: 12-01, 12-15, 12-29, 2025-01-12, ...
        // from = 2024-12-10 → next must be 2024-12-15 (NOT 12-08).
        let base = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::CatchUp,
            value: 2,
            unit: RepeaterUnit::Week,
        };
        let from = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap();
        let next = next_occurrence(base, &repeater, from).unwrap();
        assert_eq!(next, NaiveDate::from_ymd_opt(2024, 12, 15).unwrap());
    }
}
