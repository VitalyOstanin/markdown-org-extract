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

    /// Return the next workday strictly after the given date
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
}
