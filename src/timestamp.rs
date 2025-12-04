use chrono::{Datelike, NaiveDate};
use once_cell::sync::Lazy;
use regex::Regex;

const DEFAULT_DEADLINE_WARNING_DAYS: i64 = 14;

static RANGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"<(\d{4}-\d{2}-\d{2})(?: [A-Za-z]+)?(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?(?:\s+([+.]+\d+[dwmy]))?(?:\s+-(\d+)d)?>--<(\d{4}-\d{2}-\d{2})(?: [A-Za-z]+)?(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?>").unwrap()
});

static SINGLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"<(\d{4}-\d{2}-\d{2})(?: [A-Za-z]+)?(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?(?:\s+([+.]+\d+[dwmy]))?(?:\s+-(\d+)d)?>").unwrap()
});

static REPEATER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[+.](\d+)([dwmy])").unwrap()
});

static TIMESTAMP_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*((?:SCHEDULED|DEADLINE|CLOSED):\s*)<(\d{4}-\d{2}-\d{2}[^>]*)>").unwrap()
});

static RANGE_TIMESTAMP_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*<(\d{4}-\d{2}-\d{2}[^>]*)>--<(\d{4}-\d{2}-\d{2}[^>]*)>").unwrap()
});

static SIMPLE_TIMESTAMP_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*<(\d{4}-\d{2}-\d{2}[^>]*)>").unwrap()
});

static CREATED_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*CREATED:\s*<(\d{4}-\d{2}-\d{2}[^>]*)>").unwrap()
});

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

#[derive(Debug, Clone, PartialEq)]
pub enum TimestampType {
    Scheduled,
    Deadline,
    Closed,
    Plain,
}

#[derive(Debug, Clone)]
pub struct Repeater {
    pub interval: i64,
    pub unit: RepeatUnit,
}

#[derive(Debug, Clone)]
pub enum RepeatUnit {
    Day,
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone)]
pub struct Warning {
    pub days: i64,
}

pub fn parse_org_timestamp(ts: &str, mappings: Option<&[(&str, &str)]>) -> Option<ParsedTimestamp> {
    let ts = if let Some(m) = mappings {
        normalize_weekdays(ts, m)
    } else {
        ts.to_string()
    };
    let ts = ts.as_str();
    
    let timestamp_type = if ts.contains("SCHEDULED:") {
        TimestampType::Scheduled
    } else if ts.contains("DEADLINE:") {
        TimestampType::Deadline
    } else if ts.contains("CLOSED:") {
        TimestampType::Closed
    } else {
        TimestampType::Plain
    };

    if let Some(caps) = RANGE_RE.captures(ts) {
        let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
        let time = caps.get(2).map(|m| m.as_str().to_string());
        let end_date = NaiveDate::parse_from_str(&caps[6], "%Y-%m-%d").ok();
        let end_time = caps.get(7).map(|m| m.as_str().to_string());
        let repeater = caps.get(4).and_then(|m| parse_repeater(m.as_str()));
        let warning = caps.get(5).map(|m| Warning { days: m.as_str().parse().unwrap_or(0) });
        
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

    if let Some(caps) = SINGLE_RE.captures(ts) {
        let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
        let time = caps.get(2).map(|m| m.as_str().to_string());
        let end_time = caps.get(3).map(|m| m.as_str().to_string());
        let repeater = caps.get(4).and_then(|m| parse_repeater(m.as_str()));
        let warning = caps.get(5).map(|m| Warning { days: m.as_str().parse().unwrap_or(0) });
        
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

pub fn normalize_weekdays(text: &str, mappings: &[(&str, &str)]) -> String {
    let mut result = text.to_string();
    for (from, to) in mappings {
        result = result.replace(from, to);
    }
    result
}

pub fn extract_timestamp(text: &str, mappings: &[(&str, &str)]) -> Option<String> {
    let normalized = normalize_weekdays(text, mappings);
    let clean_text = normalized.trim().trim_matches('`').trim();
    
    if let Some(caps) = TIMESTAMP_RE.captures(clean_text) {
        let prefix = &caps[1];
        let date = &caps[2];
        return Some(format!("{}<{}>", prefix, date));
    }

    if let Some(caps) = RANGE_TIMESTAMP_RE.captures(clean_text) {
        return Some(format!("<{}>--<{}>", &caps[1], &caps[2]));
    }

    if let Some(caps) = SIMPLE_TIMESTAMP_RE.captures(clean_text) {
        return Some(format!("<{}>", &caps[1]));
    }

    None
}

pub fn extract_created(text: &str, mappings: &[(&str, &str)]) -> Option<String> {
    let normalized = normalize_weekdays(text, mappings);
    let clean_text = normalized.trim().trim_matches('`').trim();
    
    if let Some(caps) = CREATED_RE.captures(clean_text) {
        return Some(format!("CREATED: <{}>", &caps[1]));
    }
    None
}

pub fn parse_timestamp_fields(timestamp: &str, mappings: &[(&str, &str)]) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
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

pub fn timestamp_matches_date(parsed: &ParsedTimestamp, target_date: &NaiveDate) -> bool {
    match parsed.timestamp_type {
        TimestampType::Deadline => {
            let warning_days = parsed.warning.as_ref().map(|w| w.days).unwrap_or(DEFAULT_DEADLINE_WARNING_DAYS);
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
        TimestampType::Closed => {
            parsed.date == *target_date
        }
        TimestampType::Plain => {
            if let Some(end_date) = parsed.end_date {
                *target_date >= parsed.date && *target_date <= end_date
            } else {
                parsed.date == *target_date
            }
        }
    }
}

pub fn timestamp_in_range(parsed: &ParsedTimestamp, start: &NaiveDate, end: &NaiveDate) -> bool {
    match parsed.timestamp_type {
        TimestampType::Deadline => {
            let warning_days = parsed.warning.as_ref().map(|w| w.days).unwrap_or(DEFAULT_DEADLINE_WARNING_DAYS);
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
        TimestampType::Closed => {
            parsed.date >= *start && parsed.date <= *end
        }
        TimestampType::Plain => {
            if let Some(end_date) = parsed.end_date {
                !(end_date < *start || parsed.date > *end)
            } else {
                parsed.date >= *start && parsed.date <= *end
            }
        }
    }
}

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

fn check_repeater_in_range(base_date: &NaiveDate, repeater: &Repeater, start: &NaiveDate, end: &NaiveDate) -> bool {
    if *base_date > *end {
        return false;
    }
    
    let mut current = *base_date;
    let max_iterations = 1000;
    let mut iterations = 0;
    
    while current <= *end && iterations < max_iterations {
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

fn add_months(date: NaiveDate, months: i64) -> NaiveDate {
    let total_months = date.year() as i64 * 12 + date.month() as i64 - 1 + months;
    let year = (total_months / 12) as i32;
    let month = (total_months % 12 + 1) as u32;
    
    let max_day = NaiveDate::from_ymd_opt(year, month, 1)
        .and_then(|d| d.with_day(28))
        .and_then(|_d| {
            for day in (28..=31).rev() {
                if let Some(valid) = NaiveDate::from_ymd_opt(year, month, day) {
                    return Some(valid.day());
                }
            }
            Some(28)
        })
        .unwrap_or(28);
    
    let day = date.day().min(max_day);
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or(date)
}

fn add_years(date: NaiveDate, years: i64) -> NaiveDate {
    let new_year = date.year() + years as i32;
    NaiveDate::from_ymd_opt(new_year, date.month(), date.day())
        .or_else(|| NaiveDate::from_ymd_opt(new_year, date.month(), 28))
        .unwrap_or(date)
}
