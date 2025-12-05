use chrono::{Datelike, NaiveDate, Weekday};
use std::collections::HashSet;

include!(concat!(env!("OUT_DIR"), "/holidays_data.rs"));

#[derive(Debug)]
pub struct HolidayCalendar {
    holidays: HashSet<NaiveDate>,
    workdays: HashSet<NaiveDate>,
}

impl HolidayCalendar {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let mut holidays = HashSet::new();
        for &(year, month, day) in HOLIDAYS {
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                holidays.insert(date);
            }
        }
        
        let mut workdays = HashSet::new();
        for &(year, month, day) in WORKDAYS {
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                workdays.insert(date);
            }
        }
        
        Ok(Self { holidays, workdays })
    }
    
    pub fn is_workday(&self, date: NaiveDate) -> bool {
        if self.workdays.contains(&date) {
            return true;
        }
        if self.holidays.contains(&date) {
            return false;
        }
        !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
    }
    
    pub fn next_workday(&self, date: NaiveDate) -> NaiveDate {
        let mut current = date + chrono::Duration::days(1);
        while !self.is_workday(current) {
            current = current + chrono::Duration::days(1);
        }
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_calendar() {
        let calendar = HolidayCalendar::load().unwrap();
        assert!(calendar.holidays.len() > 0);
    }

    #[test]
    fn test_regular_weekend() {
        let calendar = HolidayCalendar::load().unwrap();
        let saturday = NaiveDate::from_ymd_opt(2025, 12, 6).unwrap();
        let sunday = NaiveDate::from_ymd_opt(2025, 12, 7).unwrap();
        assert!(!calendar.is_workday(saturday));
        assert!(!calendar.is_workday(sunday));
    }

    #[test]
    fn test_regular_weekday() {
        let calendar = HolidayCalendar::load().unwrap();
        let friday = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        assert!(calendar.is_workday(friday));
    }

    #[test]
    fn test_new_year_holidays_2025() {
        let calendar = HolidayCalendar::load().unwrap();
        for day in 1..=8 {
            let date = NaiveDate::from_ymd_opt(2025, 1, day).unwrap();
            assert!(!calendar.is_workday(date), "2025-01-{:02} should be holiday", day);
        }
    }

    #[test]
    fn test_new_year_holidays_2026() {
        let calendar = HolidayCalendar::load().unwrap();
        for day in 1..=9 {
            let date = NaiveDate::from_ymd_opt(2026, 1, day).unwrap();
            assert!(!calendar.is_workday(date), "2026-01-{:02} should be holiday", day);
        }
        let jan_12 = NaiveDate::from_ymd_opt(2026, 1, 12).unwrap();
        assert!(calendar.is_workday(jan_12), "2026-01-12 should be workday");
    }

    #[test]
    fn test_march_8_transfer_2026() {
        let calendar = HolidayCalendar::load().unwrap();
        let march_9 = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
        assert!(!calendar.is_workday(march_9), "2026-03-09 should be holiday (transfer)");
    }

    #[test]
    fn test_may_9_transfer_2026() {
        let calendar = HolidayCalendar::load().unwrap();
        let may_11 = NaiveDate::from_ymd_opt(2026, 5, 11).unwrap();
        assert!(!calendar.is_workday(may_11), "2026-05-11 should be holiday (transfer)");
    }

    #[test]
    fn test_next_workday_skip_weekend() {
        let calendar = HolidayCalendar::load().unwrap();
        let friday = NaiveDate::from_ymd_opt(2025, 12, 5).unwrap();
        let next = calendar.next_workday(friday);
        let monday = NaiveDate::from_ymd_opt(2025, 12, 8).unwrap();
        assert_eq!(next, monday);
    }

    #[test]
    fn test_next_workday_skip_holidays() {
        let calendar = HolidayCalendar::load().unwrap();
        let jan_4 = NaiveDate::from_ymd_opt(2026, 1, 4).unwrap();
        let next = calendar.next_workday(jan_4);
        let jan_12 = NaiveDate::from_ymd_opt(2026, 1, 12).unwrap();
        assert_eq!(next, jan_12);
    }
}
