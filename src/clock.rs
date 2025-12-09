use once_cell::sync::Lazy;
use regex::Regex;

use crate::types::ClockEntry;

/// Regex for CLOCK entries: CLOCK: [timestamp]--[timestamp] => duration
/// Supports both square brackets (like org-mode) and angle brackets (like other timestamps)
static CLOCK_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"CLOCK:\s*[\[<]([^\]>]+)[\]>](?:--[\[<]([^\]>]+)[\]>])?(?:\s*=>\s*([0-9]+:[0-9]+))?")
        .expect("Invalid CLOCK_RE regex")
});

/// Extract all CLOCK entries from text
pub fn extract_clocks(text: &str) -> Vec<ClockEntry> {
    CLOCK_RE
        .captures_iter(text)
        .map(|cap| ClockEntry {
            start: cap[1].to_string(),
            end: cap.get(2).map(|m| m.as_str().to_string()),
            duration: cap.get(3).map(|m| m.as_str().to_string()),
        })
        .collect()
}

/// Calculate total time from clock entries (in minutes)
pub fn calculate_total_minutes(clocks: &[ClockEntry]) -> Option<u32> {
    let mut total = 0u32;
    for clock in clocks {
        if let Some(ref dur) = clock.duration {
            if let Some(mins) = parse_duration(dur) {
                total += mins;
            }
        }
    }
    if total > 0 {
        Some(total)
    } else {
        None
    }
}

/// Format minutes as HH:MM
pub fn format_duration(minutes: u32) -> String {
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

/// Parse duration string like "2:05" to minutes
fn parse_duration(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let hours: u32 = parts[0].parse().ok()?;
    let mins: u32 = parts[1].parse().ok()?;
    Some(hours * 60 + mins)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_closed_clock_square_brackets() {
        let text = "CLOCK: [2023-02-19 Sun 21:30]--[2023-02-19 Sun 23:35] =>  2:05";
        let clocks = extract_clocks(text);
        assert_eq!(clocks.len(), 1);
        assert_eq!(clocks[0].start, "2023-02-19 Sun 21:30");
        assert_eq!(clocks[0].end, Some("2023-02-19 Sun 23:35".to_string()));
        assert_eq!(clocks[0].duration, Some("2:05".to_string()));
    }

    #[test]
    fn test_extract_closed_clock_angle_brackets() {
        let text = "CLOCK: <2023-02-19 Sun 21:30>--<2023-02-19 Sun 23:35> => 2:05";
        let clocks = extract_clocks(text);
        assert_eq!(clocks.len(), 1);
        assert_eq!(clocks[0].start, "2023-02-19 Sun 21:30");
        assert_eq!(clocks[0].end, Some("2023-02-19 Sun 23:35".to_string()));
        assert_eq!(clocks[0].duration, Some("2:05".to_string()));
    }

    #[test]
    fn test_extract_open_clock_square_brackets() {
        let text = "CLOCK: [2025-10-18 Sat 13:00]";
        let clocks = extract_clocks(text);
        assert_eq!(clocks.len(), 1);
        assert_eq!(clocks[0].start, "2025-10-18 Sat 13:00");
        assert_eq!(clocks[0].end, None);
        assert_eq!(clocks[0].duration, None);
    }

    #[test]
    fn test_extract_open_clock_angle_brackets() {
        let text = "CLOCK: <2025-10-18 Sat 13:00>";
        let clocks = extract_clocks(text);
        assert_eq!(clocks.len(), 1);
        assert_eq!(clocks[0].start, "2025-10-18 Sat 13:00");
        assert_eq!(clocks[0].end, None);
        assert_eq!(clocks[0].duration, None);
    }

    #[test]
    fn test_calculate_total() {
        let clocks = vec![
            ClockEntry {
                start: "2023-02-19 Sun 21:30".to_string(),
                end: Some("2023-02-19 Sun 23:35".to_string()),
                duration: Some("2:05".to_string()),
            },
            ClockEntry {
                start: "2023-02-20 Mon 10:00".to_string(),
                end: Some("2023-02-20 Mon 11:30".to_string()),
                duration: Some("1:30".to_string()),
            },
        ];
        let total = calculate_total_minutes(&clocks);
        assert_eq!(total, Some(215)); // 125 + 90
        assert_eq!(format_duration(215), "3:35");
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("2:05"), Some(125));
        assert_eq!(parse_duration("0:30"), Some(30));
        assert_eq!(parse_duration("10:00"), Some(600));
    }
}
