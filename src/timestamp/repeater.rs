use chrono::NaiveDate;
use crate::holidays::HolidayCalendar;

/// Repeater type and interval
#[derive(Debug, Clone, PartialEq)]
pub struct Repeater {
    pub repeater_type: RepeaterType,
    pub value: u32,
    pub unit: RepeaterUnit,
}

/// Type of repeater
#[derive(Debug, Clone, PartialEq)]
pub enum RepeaterType {
    Cumulative,    // +
    CatchUp,       // ++
    Restart,       // .+
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

/// Parse repeater string like "+1d", "++2w", ".+1m", "+1wd"
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
        return Some(Repeater {
            repeater_type,
            value,
            unit: RepeaterUnit::Workday,
        });
    }
    
    let unit_char = rest.chars().last()?;
    let value_str = &rest[..rest.len() - 1];
    let value: u32 = value_str.parse().ok()?;
    
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

/// Calculate next occurrence date for a repeater
pub fn next_occurrence(base_date: NaiveDate, repeater: &Repeater, from_date: NaiveDate) -> Option<NaiveDate> {
    use chrono::Datelike;
    
    if repeater.unit == RepeaterUnit::Workday {
        let calendar = HolidayCalendar::load().ok()?;
        let mut current = base_date;
        let mut count = 0u32;
        
        match repeater.repeater_type {
            RepeaterType::Cumulative => {
                while current < from_date {
                    current = calendar.next_workday(current);
                    count += 1;
                    if count >= repeater.value {
                        count = 0;
                    }
                }
                for _ in 0..repeater.value.saturating_sub(count) {
                    current = calendar.next_workday(current);
                }
                Some(current)
            }
            RepeaterType::CatchUp | RepeaterType::Restart => {
                current = from_date;
                for _ in 0..repeater.value {
                    current = calendar.next_workday(current);
                }
                Some(current)
            }
        }
    } else {
        match repeater.repeater_type {
            RepeaterType::Cumulative => {
                let mut current = base_date;
                let days = match repeater.unit {
                    RepeaterUnit::Day => repeater.value as i64,
                    RepeaterUnit::Week => (repeater.value * 7) as i64,
                    RepeaterUnit::Month => return add_months(base_date, repeater.value as i32),
                    RepeaterUnit::Year => return add_months(base_date, (repeater.value * 12) as i32),
                    RepeaterUnit::Hour => 1,
                    RepeaterUnit::Workday => unreachable!(),
                };
                
                while current < from_date {
                    current += chrono::Duration::days(days);
                }
                Some(current)
            }
            RepeaterType::CatchUp => {
                let days = match repeater.unit {
                    RepeaterUnit::Day => repeater.value as i64,
                    RepeaterUnit::Week => (repeater.value * 7) as i64,
                    RepeaterUnit::Month => return add_months(from_date, repeater.value as i32),
                    RepeaterUnit::Year => return add_months(from_date, (repeater.value * 12) as i32),
                    RepeaterUnit::Hour => 1,
                    RepeaterUnit::Workday => unreachable!(),
                };
                
                if repeater.unit == RepeaterUnit::Week {
                    let target_weekday = base_date.weekday();
                    let mut current = from_date;
                    while current.weekday() != target_weekday || current <= base_date {
                        current += chrono::Duration::days(1);
                    }
                    Some(current)
                } else {
                    Some(from_date + chrono::Duration::days(days))
                }
            }
            RepeaterType::Restart => {
                let days = match repeater.unit {
                    RepeaterUnit::Day => repeater.value as i64,
                    RepeaterUnit::Week => (repeater.value * 7) as i64,
                    RepeaterUnit::Month => return add_months(from_date, repeater.value as i32),
                    RepeaterUnit::Year => return add_months(from_date, (repeater.value * 12) as i32),
                    RepeaterUnit::Hour => 1,
                    RepeaterUnit::Workday => unreachable!(),
                };
                Some(from_date + chrono::Duration::days(days))
            }
        }
    }
}

fn add_months(date: NaiveDate, months: i32) -> Option<NaiveDate> {
    use chrono::Datelike;
    
    let mut year = date.year();
    let mut month = date.month() as i32 + months;
    
    while month > 12 {
        month -= 12;
        year += 1;
    }
    while month < 1 {
        month += 12;
        year -= 1;
    }
    
    let day = date.day().min(days_in_month(year, month as u32));
    NaiveDate::from_ymd_opt(year, month as u32, day)
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
        _ => 30,
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
}
