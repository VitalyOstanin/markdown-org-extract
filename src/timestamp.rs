use chrono::{Datelike, NaiveDate};
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

/// Default warning period for deadlines (14 days before)
const DEFAULT_DEADLINE_WARNING_DAYS: i64 = 14;

/// Maximum iterations for repeater calculations to prevent infinite loops
const MAX_REPEATER_ITERATIONS: usize = 1000;

// Date range pattern: <2024-12-20 Fri>--<2024-12-22 Sun>
static RANGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"<(\d{4}-\d{2}-\d{2})",
        r"(?: [A-Za-z]+)?",
        r"(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?",
        r"(?:\s+([+.]+\d+[dwmy]))?",
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
        r"(?:\s+([+.]+\d+[dwmy]))?",
        r"(?:\s+-(\d+)d)?>",
    )).expect("Invalid SINGLE_RE regex")
});

static REPEATER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[+.](\d+)([dwmy])").expect("Invalid REPEATER_RE regex")
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
    pub end_date: Option<NaiveDate>,
    pub end_time: Option<String>,
    pub repeater: Option<Repeater>,
    pub warning: Option<Warning>,
}

/// Type of org-mode timestamp
#[derive(Debug, Clone, PartialEq)]
pub enum TimestampType {
    Scheduled,
    Deadline,
    Closed,
    Plain,
}

/// Repeater configuration for recurring tasks
#[derive(Debug, Clone)]
pub struct Repeater {
    pub interval: i64,
    pub unit: RepeatUnit,
}

/// Unit for repeater interval
#[derive(Debug, Clone)]
pub enum RepeatUnit {
    Day,
    Week,
    Month,
    Year,
}

/// Warning period for deadlines
#[derive(Debug, Clone)]
pub struct Warning {
    pub days: i64,
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
        let end_date = NaiveDate::parse_from_str(&caps[6], "%Y-%m-%d").ok();
        let end_time = caps.get(7).map(|m| m.as_str().to_string());
        let repeater = caps.get(4).and_then(|m| parse_repeater(m.as_str()));
        let warning = caps.get(5).and_then(|m| m.as_str().parse().ok()).map(|days| Warning { days });
        
        return Some(ParsedTimestamp {
            timestamp_type,
            date,
            time,
            end_date,
            end_time,
            repeater,
            warning,
        });
    }

    // Try single timestamp pattern
    if let Some(caps) = SINGLE_RE.captures(&ts) {
        let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
        let time = caps.get(2).map(|m| m.as_str().to_string());
        let end_time = caps.get(3).map(|m| m.as_str().to_string());
        let repeater = caps.get(4).and_then(|m| parse_repeater(m.as_str()));
        let warning = caps.get(5).and_then(|m| m.as_str().parse().ok()).map(|days| Warning { days });
        
        return Some(ParsedTimestamp {
            timestamp_type,
            date,
            time,
            end_date: None,
            end_time,
            repeater,
            warning,
        });
    }

    None
}

/// Parse repeater string (e.g., "+1w", ".2d")
fn parse_repeater(s: &str) -> Option<Repeater> {
    let caps = REPEATER_RE.captures(s)?;
    let interval: i64 = caps[1].parse().ok()?;
    let unit = match &caps[2] {
        "d" => RepeatUnit::Day,
        "w" => RepeatUnit::Week,
        "m" => RepeatUnit::Month,
        "y" => RepeatUnit::Year,
        _ => return None,
    };
    Some(Repeater { interval, unit })
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

/// Check if timestamp matches a specific date
///
/// Rules:
/// - DEADLINE: shows from (date - warning) to date
/// - SCHEDULED: shows from date onwards (with repeater support)
/// - CLOSED: shows only on exact date
/// - PLAIN: shows on exact date or within date range
pub fn timestamp_matches_date(parsed: &ParsedTimestamp, target_date: &NaiveDate) -> bool {
    match parsed.timestamp_type {
        TimestampType::Deadline => {
            let warning_days = parsed
                .warning
                .as_ref()
                .map(|w| w.days)
                .unwrap_or(DEFAULT_DEADLINE_WARNING_DAYS);
            let warning_start = parsed.date - chrono::Duration::days(warning_days);
            *target_date >= warning_start && *target_date <= parsed.date
        }
        TimestampType::Scheduled => {
            if let Some(ref repeater) = parsed.repeater {
                check_repeater_match(&parsed.date, repeater, target_date)
            } else {
                *target_date >= parsed.date
            }
        }
        TimestampType::Closed => parsed.date == *target_date,
        TimestampType::Plain => {
            if let Some(end_date) = parsed.end_date {
                *target_date >= parsed.date && *target_date <= end_date
            } else {
                parsed.date == *target_date
            }
        }
    }
}

/// Check if timestamp falls within a date range
pub fn timestamp_in_range(parsed: &ParsedTimestamp, start: &NaiveDate, end: &NaiveDate) -> bool {
    match parsed.timestamp_type {
        TimestampType::Deadline => {
            let warning_days = parsed
                .warning
                .as_ref()
                .map(|w| w.days)
                .unwrap_or(DEFAULT_DEADLINE_WARNING_DAYS);
            let warning_start = parsed.date - chrono::Duration::days(warning_days);
            !(parsed.date < *start || warning_start > *end)
        }
        TimestampType::Scheduled => {
            if let Some(ref repeater) = parsed.repeater {
                check_repeater_in_range(&parsed.date, repeater, start, end)
            } else {
                parsed.date >= *start && parsed.date <= *end
            }
        }
        TimestampType::Closed => parsed.date >= *start && parsed.date <= *end,
        TimestampType::Plain => {
            if let Some(end_date) = parsed.end_date {
                !(end_date < *start || parsed.date > *end)
            } else {
                parsed.date >= *start && parsed.date <= *end
            }
        }
    }
}

/// Check if a repeating task matches a specific date
fn check_repeater_match(base_date: &NaiveDate, repeater: &Repeater, target_date: &NaiveDate) -> bool {
    if *target_date < *base_date {
        return false;
    }

    match repeater.unit {
        RepeatUnit::Day => {
            let days_diff = (*target_date - *base_date).num_days();
            days_diff % repeater.interval == 0
        }
        RepeatUnit::Week => {
            let days_diff = (*target_date - *base_date).num_days();
            days_diff % (repeater.interval * 7) == 0
        }
        RepeatUnit::Month => {
            let months_diff = (target_date.year() - base_date.year()) * 12
                + (target_date.month() as i32 - base_date.month() as i32);
            months_diff as i64 % repeater.interval == 0 && target_date.day() == base_date.day()
        }
        RepeatUnit::Year => {
            let years_diff = target_date.year() - base_date.year();
            years_diff as i64 % repeater.interval == 0
                && target_date.month() == base_date.month()
                && target_date.day() == base_date.day()
        }
    }
}

/// Check if a repeating task has any occurrence within a date range
fn check_repeater_in_range(
    base_date: &NaiveDate,
    repeater: &Repeater,
    start: &NaiveDate,
    end: &NaiveDate,
) -> bool {
    if *base_date > *end {
        return false;
    }

    let mut current = *base_date;
    let mut iterations = 0;

    while current <= *end && iterations < MAX_REPEATER_ITERATIONS {
        if current >= *start {
            return true;
        }

        current = match repeater.unit {
            RepeatUnit::Day => current + chrono::Duration::days(repeater.interval),
            RepeatUnit::Week => current + chrono::Duration::weeks(repeater.interval),
            RepeatUnit::Month => add_months(current, repeater.interval),
            RepeatUnit::Year => add_years(current, repeater.interval),
        };

        iterations += 1;
    }
    false
}

/// Add months to a date, handling month-end edge cases
fn add_months(date: NaiveDate, months: i64) -> NaiveDate {
    let total_months = date.year() as i64 * 12 + date.month() as i64 - 1 + months;
    let year = (total_months / 12) as i32;
    let month = (total_months % 12 + 1) as u32;

    let max_day = NaiveDate::from_ymd_opt(year, month, 1)
        .and_then(|d| d.with_day(28))
        .map(|_d| {
            for day in (28..=31).rev() {
                if let Some(valid) = NaiveDate::from_ymd_opt(year, month, day) {
                    return valid.day();
                }
            }
            28
        })
        .unwrap_or(28);

    let day = date.day().min(max_day);
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or(date)
}

/// Add years to a date, handling leap year edge cases
fn add_years(date: NaiveDate, years: i64) -> NaiveDate {
    let new_year = date.year() + years as i32;
    NaiveDate::from_ymd_opt(new_year, date.month(), date.day())
        .or_else(|| NaiveDate::from_ymd_opt(new_year, date.month(), 28))
        .unwrap_or(date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_deadline_with_warning() {
        let ts = "DEADLINE: <2024-12-15 Sun -7d>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.timestamp_type, TimestampType::Deadline);
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 15).unwrap());
        assert_eq!(parsed.warning.unwrap().days, 7);
    }

    #[test]
    fn test_parse_scheduled_with_repeater() {
        let ts = "SCHEDULED: <2024-12-01 Mon +1w>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.timestamp_type, TimestampType::Scheduled);
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 1).unwrap());
        assert!(parsed.repeater.is_some());
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
