use chrono::{Datelike, NaiveDate};
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

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
}

/// Type of org-mode timestamp
#[derive(Debug, Clone, PartialEq)]
pub enum TimestampType {
    Scheduled,
    Deadline,
    Closed,
    Plain,
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
        
        return Some(ParsedTimestamp {
            timestamp_type,
            date,
            time,
            end_time,
        });
    }

    // Try single timestamp pattern
    if let Some(caps) = SINGLE_RE.captures(&ts) {
        let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
        let time = caps.get(2).map(|m| m.as_str().to_string());
        let end_time = caps.get(3).map(|m| m.as_str().to_string());
        
        return Some(ParsedTimestamp {
            timestamp_type,
            date,
            time,
            end_time,
        });
    }

    None
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
    }

    #[test]
    fn test_parse_scheduled() {
        let ts = "SCHEDULED: <2024-12-01 Mon>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.timestamp_type, TimestampType::Scheduled);
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 1).unwrap());
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
