use chrono::{Datelike, NaiveDate, Weekday};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/holidays_data.rs"));

/// Russian holiday and workday calendar built from compile-time data.
///
/// Internally uses sorted `Vec<NaiveDate>` (one for holidays, one for
/// transferred workdays) and binary search. This trades the hash bucket walk
/// of `HashSet` for a branch-light `log N` lookup on a dense memory layout —
/// noticeably faster on the hot path through `closest_date` for workday
/// repeaters where we probe many sequential dates.
#[derive(Debug)]
pub struct HolidayCalendar {
    holidays: Vec<NaiveDate>,
    workdays: Vec<NaiveDate>,
}

impl HolidayCalendar {
    /// Return the global singleton calendar
    ///
    /// Cheap to call repeatedly: initialization happens once per process.
    pub fn global() -> &'static HolidayCalendar {
        static CALENDAR: OnceLock<HolidayCalendar> = OnceLock::new();
        CALENDAR.get_or_init(HolidayCalendar::build)
    }

    fn build() -> Self {
        let mut holidays: Vec<NaiveDate> = HOLIDAYS
            .iter()
            .filter_map(|&(y, m, d)| NaiveDate::from_ymd_opt(y, m, d))
            .collect();
        holidays.sort_unstable();
        holidays.dedup();

        let mut workdays: Vec<NaiveDate> = WORKDAYS
            .iter()
            .filter_map(|&(y, m, d)| NaiveDate::from_ymd_opt(y, m, d))
            .collect();
        workdays.sort_unstable();
        workdays.dedup();

        Self { holidays, workdays }
    }

    /// Check whether the given date is a workday under the Russian calendar
    pub fn is_workday(&self, date: NaiveDate) -> bool {
        if self.workdays.binary_search(&date).is_ok() {
            return true;
        }
        if self.holidays.binary_search(&date).is_ok() {
            return false;
        }
        !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
    }

    /// Return the next workday strictly after the given date.
    /// Test-only: production code reaches workday occurrences through the
    /// O(log n) `nth_workday_after` / `workdays_between_exclusive` helpers.
    #[cfg(test)]
    pub fn next_workday(&self, date: NaiveDate) -> NaiveDate {
        let mut current = date + chrono::Duration::days(1);
        while !self.is_workday(current) {
            current += chrono::Duration::days(1);
        }
        current
    }

    /// Return all holidays in the given year, sorted ascending
    pub fn get_holidays_for_year(&self, year: i32) -> Vec<NaiveDate> {
        // `holidays` is already sorted, so a simple filter preserves order.
        self.holidays
            .iter()
            .filter(|d| d.year() == year)
            .copied()
            .collect()
    }

    /// Count workdays in the half-open interval `(start, end]`.
    ///
    /// Runs in `O(log H + log W + k)` where `H`, `W` are the holiday and
    /// transfer-workday counts and `k` is the number of holidays/workdays
    /// inside the range (typically a handful for one-month spans).
    ///
    /// Returns 0 if `end <= start`. Used by `nth_workday_after` to avoid
    /// the O(N) day-by-day scan in `closest_date` for long agenda ranges.
    pub fn workdays_between_exclusive(&self, start: NaiveDate, end: NaiveDate) -> i64 {
        if end <= start {
            return 0;
        }
        // Count plain weekdays Mon..Fri in the inclusive range [start+1, end].
        let first = start + chrono::Duration::days(1);
        let weekdays = count_weekdays_inclusive(first, end);

        // Subtract holidays in (start, end] that fall on Mon..Fri.
        let h_lo = self.holidays.partition_point(|d| *d <= start);
        let h_hi = self.holidays.partition_point(|d| *d <= end);
        let holidays_on_weekday = self.holidays[h_lo..h_hi]
            .iter()
            .filter(|d| !matches!(d.weekday(), Weekday::Sat | Weekday::Sun))
            .count() as i64;

        // Add transfer workdays in (start, end] that fall on Sat..Sun.
        let w_lo = self.workdays.partition_point(|d| *d <= start);
        let w_hi = self.workdays.partition_point(|d| *d <= end);
        let workdays_on_weekend = self.workdays[w_lo..w_hi]
            .iter()
            .filter(|d| matches!(d.weekday(), Weekday::Sat | Weekday::Sun))
            .count() as i64;

        weekdays - holidays_on_weekday + workdays_on_weekend
    }

    /// Return the `n`-th workday strictly after `base`.
    ///
    /// Uses binary search on `workdays_between_exclusive`, giving an
    /// `O(log(date_range) * log(holidays))` lookup. `nth_workday_after(d, 0)`
    /// returns `d` itself (mirrors the loop semantics in `closest_date`,
    /// where `k=0` means the base occurrence).
    pub fn nth_workday_after(&self, base: NaiveDate, n: u64) -> NaiveDate {
        if n == 0 {
            return base;
        }
        let n_i64 = n as i64;

        // Initial upper bound: roughly ceil(n * 7/5) days plus generous slack
        // for back-to-back holidays (e.g. 10-day January in Russia).
        let initial_span = (n_i64.saturating_mul(7) / 5).saturating_add(40);
        let mut hi = base + chrono::Duration::days(initial_span);
        while self.workdays_between_exclusive(base, hi) < n_i64 {
            let span = (hi - base).num_days();
            hi = base + chrono::Duration::days(span.saturating_mul(2));
        }

        let mut lo = base + chrono::Duration::days(1);
        while lo < hi {
            let mid = lo + chrono::Duration::days((hi - lo).num_days() / 2);
            if self.workdays_between_exclusive(base, mid) < n_i64 {
                lo = mid + chrono::Duration::days(1);
            } else {
                hi = mid;
            }
        }
        lo
    }
}

/// Count Mon-Fri days in the inclusive range `[a, b]`. Returns 0 if `a > b`.
///
/// Uses arithmetic on the weekday of `a` so we don't pay per-day iteration
/// cost. Helper for `workdays_between_exclusive`.
fn count_weekdays_inclusive(a: NaiveDate, b: NaiveDate) -> i64 {
    if a > b {
        return 0;
    }
    let total = (b - a).num_days() + 1; // inclusive count of days
    let full_weeks = total / 7;
    let remainder = total % 7;

    // Mon=0..Sun=6 via num_days_from_monday
    let mut weekend_in_remainder = 0i64;
    let start_wd = a.weekday().num_days_from_monday() as i64; // 0..=6
    for i in 0..remainder {
        let wd = (start_wd + i) % 7;
        if wd == 5 || wd == 6 {
            weekend_in_remainder += 1;
        }
    }

    full_weeks * 5 + (remainder - weekend_in_remainder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_calendar() {
        let calendar = HolidayCalendar::global();
        assert!(!calendar.holidays.is_empty());
    }

    #[test]
    fn test_regular_weekend() {
        let calendar = HolidayCalendar::global();
        let saturday = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let sunday = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        assert!(!calendar.is_workday(saturday));
        assert!(!calendar.is_workday(sunday));
    }

    #[test]
    fn test_regular_weekday() {
        let calendar = HolidayCalendar::global();
        let friday = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        assert!(calendar.is_workday(friday));
    }

    #[test]
    fn test_new_year_holidays_2025() {
        let calendar = HolidayCalendar::global();
        for day in 1..=8 {
            let date = NaiveDate::from_ymd_opt(2025, 1, day).unwrap();
            assert!(
                !calendar.is_workday(date),
                "2025-01-{day:02} should be holiday"
            );
        }
    }

    #[test]
    fn test_new_year_holidays_2026() {
        let calendar = HolidayCalendar::global();
        for day in 1..=9 {
            let date = NaiveDate::from_ymd_opt(2026, 1, day).unwrap();
            assert!(
                !calendar.is_workday(date),
                "2026-01-{day:02} should be holiday"
            );
        }
        let jan_12 = NaiveDate::from_ymd_opt(2026, 1, 12).unwrap();
        assert!(calendar.is_workday(jan_12), "2026-01-12 should be workday");
    }

    #[test]
    fn test_march_8_transfer_2026() {
        let calendar = HolidayCalendar::global();
        let march_9 = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        assert!(
            !calendar.is_workday(march_9),
            "2026-03-09 should be holiday (transfer)"
        );
    }

    #[test]
    fn test_may_9_transfer_2026() {
        let calendar = HolidayCalendar::global();
        let may_11 = NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        assert!(
            !calendar.is_workday(may_11),
            "2026-05-11 should be holiday (transfer)"
        );
    }

    #[test]
    fn test_next_workday_skip_weekend() {
        let calendar = HolidayCalendar::global();
        let friday = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let next = calendar.next_workday(friday);
        let monday = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        assert_eq!(next, monday);
    }

    #[test]
    fn test_next_workday_skip_holidays() {
        let calendar = HolidayCalendar::global();
        let jan_4 = NaiveDate::from_ymd_opt(2026, 1, 4).unwrap();
        let next = calendar.next_workday(jan_4);
        let jan_12 = NaiveDate::from_ymd_opt(2026, 1, 12).unwrap();
        assert_eq!(next, jan_12);
    }

    /// Attribution must stay in the data file: a future contributor stripping
    /// `_meta` accidentally would lose the licensing context the README points
    /// at. Lock the keys we promise are there (description/source/license/schema).
    #[test]
    fn holidays_json_carries_attribution_meta() {
        let raw = include_str!("../holidays_ru.json");
        let parsed: serde_json::Value =
            serde_json::from_str(raw).expect("holidays_ru.json is JSON");
        let meta = parsed
            .get("_meta")
            .and_then(|v| v.as_object())
            .expect("`_meta` block must be present at the top level");
        for key in ["description", "source", "license", "schema"] {
            assert!(
                meta.contains_key(key) && meta[key].is_string(),
                "_meta is missing required string field `{key}`"
            );
        }
    }

    /// End-to-end check of the `build.rs` pipeline: the JSON file shipped with
    /// the crate (`holidays_ru.json`) must round-trip into the compiled-in
    /// `HOLIDAYS` / `WORKDAYS` static arrays. This catches regressions where
    /// `build.rs` silently drops entries (invalid date format, sort/dedup bugs,
    /// schema drift) without requiring us to spawn a separate cargo build.
    #[test]
    fn test_build_pipeline_matches_json_source() {
        let raw = include_str!("../holidays_ru.json");
        let parsed: serde_json::Value =
            serde_json::from_str(raw).expect("holidays_ru.json is JSON");
        let root = parsed.as_object().expect("top-level is object");

        let mut expected_holidays: Vec<(i32, u32, u32)> = Vec::new();
        let mut expected_workdays: Vec<(i32, u32, u32)> = Vec::new();
        for (_year_key, year_data) in root {
            if let Some(arr) = year_data.get("holidays").and_then(|v| v.as_array()) {
                for entry in arr {
                    let s = entry.as_str().expect("date is a string");
                    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                        .unwrap_or_else(|e| panic!("malformed JSON date {s:?}: {e}"));
                    expected_holidays.push((d.year(), d.month(), d.day()));
                }
            }
            if let Some(arr) = year_data.get("workdays").and_then(|v| v.as_array()) {
                for entry in arr {
                    let s = entry.as_str().expect("date is a string");
                    let d = NaiveDate::parse_from_str(s, "%Y-%m-%d")
                        .unwrap_or_else(|e| panic!("malformed JSON date {s:?}: {e}"));
                    expected_workdays.push((d.year(), d.month(), d.day()));
                }
            }
        }

        // `build.rs` is allowed to sort + dedup, so compare by set semantics.
        expected_holidays.sort_unstable();
        expected_holidays.dedup();
        expected_workdays.sort_unstable();
        expected_workdays.dedup();

        let mut compiled_holidays: Vec<(i32, u32, u32)> = HOLIDAYS.to_vec();
        compiled_holidays.sort_unstable();
        compiled_holidays.dedup();
        let mut compiled_workdays: Vec<(i32, u32, u32)> = WORKDAYS.to_vec();
        compiled_workdays.sort_unstable();
        compiled_workdays.dedup();

        assert_eq!(
            compiled_holidays, expected_holidays,
            "HOLIDAYS static must match holidays_ru.json after build.rs run"
        );
        assert_eq!(
            compiled_workdays, expected_workdays,
            "WORKDAYS static must match holidays_ru.json after build.rs run"
        );
    }

    /// `build.rs` must produce a sorted, dedup'd calendar so the runtime
    /// `binary_search` in [`HolidayCalendar::is_workday`] gives correct
    /// results. This regression test asserts the post-build invariant
    /// directly on the live singleton.
    #[test]
    fn test_calendar_is_sorted_and_unique() {
        let calendar = HolidayCalendar::global();
        assert!(
            calendar.holidays.windows(2).all(|w| w[0] < w[1]),
            "holidays must be sorted and unique"
        );
        assert!(
            calendar.workdays.windows(2).all(|w| w[0] < w[1]),
            "workdays must be sorted and unique"
        );
    }
}
