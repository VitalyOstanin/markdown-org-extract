use chrono::NaiveDate;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

use super::repeater::{parse_repeater, Repeater};

static RANGE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"<(\d{4}-\d{2}-\d{2})",
        r"(?: (?:Mon|Tue|Wed|Thu|Fri|Sat|Sun|Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday))?",
        r"(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?",
        r"(?:\s*([.+]+\d+(?:wd|[dwmyh])))?",
        r"(?:\s+-(\d+)d)?>",
        r"--",
        r"<(\d{4}-\d{2}-\d{2})",
        r"(?: (?:Mon|Tue|Wed|Thu|Fri|Sat|Sun|Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday))?",
        r"(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?>",
    )).expect("Invalid RANGE_RE regex")
});

static SINGLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"<(\d{4}-\d{2}-\d{2})",
        r"(?: (?:Mon|Tue|Wed|Thu|Fri|Sat|Sun|Monday|Tuesday|Wednesday|Thursday|Friday|Saturday|Sunday))?",
        r"(?: (\d{1,2}:\d{2})(?:-(\d{1,2}:\d{2}))?)?",
        r"(?:\s*([.+]+\d+(?:wd|[dwmyh])))?",
        r"(?:\s+-(\d+)d)?>",
    )).expect("Invalid SINGLE_RE regex")
});

#[derive(Debug, Clone)]
pub struct ParsedTimestamp {
    pub date: NaiveDate,
    pub repeater: Option<Repeater>,
}

pub fn parse_org_timestamp(ts: &str, mappings: Option<&[(&str, &str)]>) -> Option<ParsedTimestamp> {
    let ts = if let Some(m) = mappings {
        normalize_weekdays(ts, m)
    } else {
        Cow::Borrowed(ts)
    };

    if let Some(caps) = RANGE_RE.captures(&ts) {
        let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
        let repeater = caps.get(4).and_then(|m| parse_repeater(m.as_str()));
        
        return Some(ParsedTimestamp { date, repeater });
    }

    if let Some(caps) = SINGLE_RE.captures(&ts) {
        let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
        let repeater = caps.get(4).and_then(|m| parse_repeater(m.as_str()));
        
        return Some(ParsedTimestamp { date, repeater });
    }

    None
}

fn normalize_weekdays<'a>(text: &'a str, mappings: &[(&str, &str)]) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);
    for (localized, english) in mappings {
        if result.contains(localized) {
            result = Cow::Owned(result.replace(localized, english));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp_with_workday_repeater() {
        let ts = "<2025-12-05 Thu +1wd>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2025, 12, 5).unwrap());
        assert!(parsed.repeater.is_some());
        let repeater = parsed.repeater.unwrap();
        assert_eq!(repeater.value, 1);
        assert_eq!(repeater.unit, super::super::repeater::RepeaterUnit::Workday);
    }

    #[test]
    fn test_parse_timestamp_with_workday_repeater_multiple() {
        let ts = "<2025-12-09 Mon +2wd>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        let repeater = parsed.repeater.unwrap();
        assert_eq!(repeater.value, 2);
        assert_eq!(repeater.unit, super::super::repeater::RepeaterUnit::Workday);
    }

    #[test]
    fn test_parse_timestamp_with_regular_repeater() {
        let ts = "<2025-12-05 Thu +1d>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        let repeater = parsed.repeater.unwrap();
        assert_eq!(repeater.unit, super::super::repeater::RepeaterUnit::Day);
    }
}
