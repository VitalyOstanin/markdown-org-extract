use chrono::NaiveDate;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

// Date range pattern: <2024-12-20 Fri>--<2024-12-22 Sun>
static RANGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"<(\d{4}-\d{2}-\d{2})",
        r"(?: [A-Za-z]+)?",
        r"(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?",
        r"(?:\s*([.+]+\d+[dwmyh]))?",
        r"(?:\s+-(\d+)d)?>",
        r"--",
        r"<(\d{4}-\d{2}-\d{2})",
        r"(?: [A-Za-z]+)?",
        r"(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?>",
    )).expect("Invalid RANGE_RE regex")
});

// Single timestamp pattern: <2024-12-05 Wed 10:00-12:00>
static SINGLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"<(\d{4}-\d{2}-\d{2})",
        r"(?: [A-Za-z]+)?",
        r"(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?",
        r"(?:\s*([.+]+\d+[dwmyh]))?",
        r"(?:\s+-(\d+)d)?>",
    )).expect("Invalid SINGLE_RE regex")
});

static TIMESTAMP_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*((?:SCHEDULED|DEADLINE|CLOSED):\s*)<(\d{4}-\d{2}-\d{2}[^>]*)>")
        .expect("Invalid TIMESTAMP_RE regex")
});

static RANGE_TIMESTAMP_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*<(\d{4}-\d{2}-\d{2}[^>]*)>--<(\d{4}-\d{2}-\d{2}[^>]*)>")
        .expect("Invalid RANGE_TIMESTAMP_RE regex")
});

static SIMPLE_TIMESTAMP_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*<(\d{4}-\d{2}-\d{2}[^>]*)>")
        .expect("Invalid SIMPLE_TIMESTAMP_RE regex")
});

static CREATED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*CREATED:\s*<(\d{4}-\d{2}-\d{2}[^>]*)>")
        .expect("Invalid CREATED_RE regex")
});

/// Parsed org-mode timestamp with all components
#[derive(Debug, Clone)]
pub struct ParsedTimestamp {
    pub timestamp_type: TimestampType,
    pub date: NaiveDate,
    pub time: Option<String>,
    pub end_time: Option<String>,
    pub repeater: Option<Repeater>,
}

/// Type of org-mode timestamp
#[derive(Debug, Clone, PartialEq)]
pub enum TimestampType {
    Scheduled,
    Deadline,
    Closed,
    Plain,
}

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
}

/// Parse org-mode timestamp string into structured format
///
/// # Arguments
/// * `ts` - Timestamp string (e.g., "SCHEDULED: <2024-12-10 Tue>")
/// * `mappings` - Optional weekday translations (e.g., Russian to English)
///
/// # Returns
/// Parsed timestamp or None if parsing fails
pub fn parse_org_timestamp(ts: &str, mappings: Option<&[(&str, &str)]>) -> Option<ParsedTimestamp> {
    let ts = if let Some(m) = mappings {
        normalize_weekdays(ts, m)
    } else {
        Cow::Borrowed(ts)
    };
    
    let timestamp_type = if ts.contains("SCHEDULED:") {
        TimestampType::Scheduled
    } else if ts.contains("DEADLINE:") {
        TimestampType::Deadline
    } else if ts.contains("CLOSED:") {
        TimestampType::Closed
    } else {
        TimestampType::Plain
    };

    // Try date range pattern first
    if let Some(caps) = RANGE_RE.captures(&ts) {
        let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
        let time = caps.get(2).map(|m| m.as_str().to_string());
        let end_time = caps.get(7).map(|m| m.as_str().to_string());
        let repeater = caps.get(4).and_then(|m| parse_repeater(m.as_str()));
        
        return Some(ParsedTimestamp {
            timestamp_type,
            date,
            time,
            end_time,
            repeater,
        });
    }

    // Try single timestamp pattern
    if let Some(caps) = SINGLE_RE.captures(&ts) {
        let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
        let time = caps.get(2).map(|m| m.as_str().to_string());
        let end_time = caps.get(3).map(|m| m.as_str().to_string());
        let repeater = caps.get(4).and_then(|m| parse_repeater(m.as_str()));
        
        return Some(ParsedTimestamp {
            timestamp_type,
            date,
            time,
            end_time,
            repeater,
        });
    }

    None
}

/// Parse repeater string like "+1d", "++2w", ".+1m"
fn parse_repeater(s: &str) -> Option<Repeater> {
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
    
    match repeater.repeater_type {
        RepeaterType::Cumulative => {
            // + type: strict interval from base_date
            let mut current = base_date;
            let days = match repeater.unit {
                RepeaterUnit::Day => repeater.value as i64,
                RepeaterUnit::Week => (repeater.value * 7) as i64,
                RepeaterUnit::Month => return add_months(base_date, repeater.value as i32),
                RepeaterUnit::Year => return add_months(base_date, (repeater.value * 12) as i32),
                RepeaterUnit::Hour => 1, // treat as daily for date calculation
            };
            
            while current < from_date {
                current += chrono::Duration::days(days);
            }
            Some(current)
        }
        RepeaterType::CatchUp => {
            // ++ type: jump to future, preserve day of week
            let days = match repeater.unit {
                RepeaterUnit::Day => repeater.value as i64,
                RepeaterUnit::Week => (repeater.value * 7) as i64,
                RepeaterUnit::Month => return add_months(from_date, repeater.value as i32),
                RepeaterUnit::Year => return add_months(from_date, (repeater.value * 12) as i32),
                RepeaterUnit::Hour => 1,
            };
            
            if repeater.unit == RepeaterUnit::Week {
                // Preserve day of week
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
            // .+ type: from completion date (from_date)
            let days = match repeater.unit {
                RepeaterUnit::Day => repeater.value as i64,
                RepeaterUnit::Week => (repeater.value * 7) as i64,
                RepeaterUnit::Month => return add_months(from_date, repeater.value as i32),
                RepeaterUnit::Year => return add_months(from_date, (repeater.value * 12) as i32),
                RepeaterUnit::Hour => 1,
            };
            Some(from_date + chrono::Duration::days(days))
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

/// Normalize weekday names using provided mappings
///
/// Uses Cow to avoid allocation when no replacements are needed
pub fn normalize_weekdays<'a>(text: &'a str, mappings: &[(&str, &str)]) -> Cow<'a, str> {
    // Check if any mapping matches
    let has_match = mappings.iter().any(|(from, _)| text.contains(from));
    
    if !has_match {
        return Cow::Borrowed(text);
    }
    
    // Perform all replacements in one pass
    let mut result = text.to_string();
    for (from, to) in mappings {
        if result.contains(from) {
            result = result.replace(from, to);
        }
    }
    Cow::Owned(result)
}

/// Extract timestamp from text (excluding CREATED)
pub fn extract_timestamp(text: &str, mappings: &[(&str, &str)]) -> Option<String> {
    let normalized = normalize_weekdays(text, mappings);
    let clean_text = normalized.trim().trim_matches('`').trim();
    
    if let Some(caps) = TIMESTAMP_RE.captures(clean_text) {
        let prefix = &caps[1];
        let date = &caps[2];
        return Some(format!("{prefix}<{date}>"));
    }

    if let Some(caps) = RANGE_TIMESTAMP_RE.captures(clean_text) {
        return Some(format!("<{}>--<{}>", &caps[1], &caps[2]));
    }

    if let Some(caps) = SIMPLE_TIMESTAMP_RE.captures(clean_text) {
        return Some(format!("<{}>", &caps[1]));
    }

    None
}

/// Extract CREATED timestamp from text
pub fn extract_created(text: &str, mappings: &[(&str, &str)]) -> Option<String> {
    let normalized = normalize_weekdays(text, mappings);
    let clean_text = normalized.trim().trim_matches('`').trim();
    
    if let Some(caps) = CREATED_RE.captures(clean_text) {
        return Some(format!("CREATED: <{}>", &caps[1]));
    }
    None
}

/// Parse timestamp into separate fields for JSON output
pub fn parse_timestamp_fields(
    timestamp: &str,
    mappings: &[(&str, &str)],
) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    if let Some(parsed) = parse_org_timestamp(timestamp, Some(mappings)) {
        let ts_type = match parsed.timestamp_type {
            TimestampType::Scheduled => "SCHEDULED",
            TimestampType::Deadline => "DEADLINE",
            TimestampType::Closed => "CLOSED",
            TimestampType::Plain => "PLAIN",
        };
        let ts_date = parsed.date.format("%Y-%m-%d").to_string();
        (
            Some(ts_type.to_string()),
            Some(ts_date),
            parsed.time,
            parsed.end_time,
        )
    } else {
        (None, None, None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_deadline() {
        let ts = "DEADLINE: <2024-12-15 Sun>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.timestamp_type, TimestampType::Deadline);
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 15).unwrap());
        assert_eq!(parsed.repeater, None);
    }

    #[test]
    fn test_parse_scheduled() {
        let ts = "SCHEDULED: <2024-12-01 Mon>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.timestamp_type, TimestampType::Scheduled);
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 1).unwrap());
        assert_eq!(parsed.repeater, None);
    }

    #[test]
    fn test_parse_repeater_cumulative() {
        let ts = "SCHEDULED: <2024-12-01 Sun +1d>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert!(parsed.repeater.is_some());
        let repeater = parsed.repeater.unwrap();
        assert_eq!(repeater.repeater_type, RepeaterType::Cumulative);
        assert_eq!(repeater.value, 1);
        assert_eq!(repeater.unit, RepeaterUnit::Day);
    }

    #[test]
    fn test_parse_repeater_catchup() {
        let ts = "SCHEDULED: <2024-12-01 Sun ++1w>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert!(parsed.repeater.is_some());
        let repeater = parsed.repeater.unwrap();
        assert_eq!(repeater.repeater_type, RepeaterType::CatchUp);
        assert_eq!(repeater.value, 1);
        assert_eq!(repeater.unit, RepeaterUnit::Week);
    }

    #[test]
    fn test_parse_repeater_restart() {
        let ts = "SCHEDULED: <2024-12-01 Sun .+1m>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert!(parsed.repeater.is_some());
        let repeater = parsed.repeater.unwrap();
        assert_eq!(repeater.repeater_type, RepeaterType::Restart);
        assert_eq!(repeater.value, 1);
        assert_eq!(repeater.unit, RepeaterUnit::Month);
    }

    #[test]
    fn test_parse_repeater_with_time() {
        let ts = "SCHEDULED: <2024-12-01 Sun 10:30-11:00 +1d>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.time, Some("10:30".to_string()));
        assert_eq!(parsed.end_time, Some("11:00".to_string()));
        assert!(parsed.repeater.is_some());
        let repeater = parsed.repeater.unwrap();
        assert_eq!(repeater.repeater_type, RepeaterType::Cumulative);
        assert_eq!(repeater.value, 1);
        assert_eq!(repeater.unit, RepeaterUnit::Day);
    }

    #[test]
    fn test_parse_repeater_various_units() {
        let test_cases = vec![
            ("+2d", RepeaterUnit::Day, 2),
            ("+3w", RepeaterUnit::Week, 3),
            ("+1m", RepeaterUnit::Month, 1),
            ("+1y", RepeaterUnit::Year, 1),
            ("+4h", RepeaterUnit::Hour, 4),
        ];

        for (repeater_str, expected_unit, expected_value) in test_cases {
            let ts = format!("SCHEDULED: <2024-12-01 Sun {repeater_str}>");
            let parsed = parse_org_timestamp(&ts, None).unwrap();
            let repeater = parsed.repeater.unwrap();
            assert_eq!(repeater.unit, expected_unit, "Failed for {repeater_str}");
            assert_eq!(repeater.value, expected_value, "Failed for {repeater_str}");
        }
    }

    #[test]
    fn test_next_occurrence_cumulative_daily() {
        let base = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Day,
        };

        let from = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        let next = next_occurrence(base, &repeater, from).unwrap();
        assert_eq!(next, NaiveDate::from_ymd_opt(2024, 12, 5).unwrap());
    }

    #[test]
    fn test_next_occurrence_cumulative_weekly() {
        let base = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap(); // Sunday
        let repeater = Repeater {
            repeater_type: RepeaterType::Cumulative,
            value: 1,
            unit: RepeaterUnit::Week,
        };

        let from = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap();
        let next = next_occurrence(base, &repeater, from).unwrap();
        assert_eq!(next, NaiveDate::from_ymd_opt(2024, 12, 15).unwrap()); // Next Sunday
    }

    #[test]
    fn test_next_occurrence_catchup() {
        let base = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap(); // Sunday
        let repeater = Repeater {
            repeater_type: RepeaterType::CatchUp,
            value: 1,
            unit: RepeaterUnit::Week,
        };

        let from = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap(); // Tuesday
        let next = next_occurrence(base, &repeater, from).unwrap();
        // Should jump to next Sunday after from date
        assert_eq!(next, NaiveDate::from_ymd_opt(2024, 12, 15).unwrap());
    }

    #[test]
    fn test_next_occurrence_restart() {
        let base = NaiveDate::from_ymd_opt(2024, 12, 1).unwrap();
        let repeater = Repeater {
            repeater_type: RepeaterType::Restart,
            value: 1,
            unit: RepeaterUnit::Day,
        };

        let from = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap();
        let next = next_occurrence(base, &repeater, from).unwrap();
        // Should be 1 day after from date
        assert_eq!(next, NaiveDate::from_ymd_opt(2024, 12, 11).unwrap());
    }

    #[test]
    fn test_add_months() {
        let date = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        let result = add_months(date, 1).unwrap();
        // Jan 31 + 1 month = Feb 29 (2024 is leap year)
        assert_eq!(result, NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());

        let date = NaiveDate::from_ymd_opt(2024, 1, 31).unwrap();
        let result = add_months(date, 13).unwrap();
        // Jan 31 + 13 months = Feb 28, 2025
        assert_eq!(result, NaiveDate::from_ymd_opt(2025, 2, 28).unwrap());
    }

    #[test]
    fn test_normalize_weekdays_cow() {
        let mappings = vec![("Пн", "Mon")];
        let text = "No changes";
        let result = normalize_weekdays(text, &mappings);
        assert!(matches!(result, Cow::Borrowed(_)));
        
        let text = "Пн";
        let result = normalize_weekdays(text, &mappings);
        assert!(matches!(result, Cow::Owned(_)));
        assert_eq!(result, "Mon");
    }
}
