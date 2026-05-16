use regex::Regex;
use std::sync::LazyLock;

use crate::regex_limits::compile_bounded;
use crate::types::ClockEntry;

/// Regex for CLOCK entries: CLOCK: [timestamp]--[timestamp] => duration
///
/// Supports both square brackets (org-mode inactive timestamps) and angle
/// brackets (active timestamps), but the opening and closing bracket of each
/// timestamp must match — `[…>` or `<…]` are rejected as malformed.
/// Inner bodies capped at 128 chars to bound the work done on malformed input.
static CLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    compile_bounded(
        r"CLOCK:\s*(?:\[([^\]<>]{1,128})\]|<([^\]<>]{1,128})>)(?:--(?:\[([^\]<>]{1,128})\]|<([^\]<>]{1,128})>))?(?:\s*=>\s*([0-9]{1,5}:[0-9]{1,2}))?",
    )
});

/// Extract all CLOCK entries from text.
///
/// Two forms are recognized:
/// - **Closed**: `CLOCK: [start]--[end] =>  HH:MM` — yields all three fields
///   filled (`start`, `Some(end)`, `Some(duration)`).
/// - **Open**: `CLOCK: [start]` — represents an in-progress interval that has
///   not yet been closed; yields `start` only with `end = None` and
///   `duration = None`.
///
/// The duration tail (`=> HH:MM`) is optional even on closed clocks; org-mode
/// inserts it automatically but does not require it for the line to parse.
pub fn extract_clocks(text: &str) -> Vec<ClockEntry> {
    CLOCK_RE
        .captures_iter(text)
        .map(|cap| {
            let start = cap
                .get(1)
                .or_else(|| cap.get(2))
                .expect("CLOCK regex matched without a start timestamp")
                .as_str()
                .to_string();
            let end = cap
                .get(3)
                .or_else(|| cap.get(4))
                .map(|m| m.as_str().to_string());
            let duration = cap.get(5).map(|m| m.as_str().to_string());
            ClockEntry {
                start,
                end,
                duration,
            }
        })
        .collect()
}

/// Calculate total time from clock entries (in minutes). Returns None on overflow
/// or empty result; uses `checked_add` so a malicious or buggy input cannot wrap.
pub fn calculate_total_minutes(clocks: &[ClockEntry]) -> Option<u32> {
    let mut total = 0u32;
    for clock in clocks {
        if let Some(ref dur) = clock.duration {
            if let Some(mins) = parse_duration(dur) {
                total = total.checked_add(mins)?;
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

/// Org-mode duration upper bound. CLOCK durations beyond this are treated as
/// malformed: 10_000 hours = ~416 days. Anything larger almost certainly comes
/// from a parser bug or hostile input.
const MAX_DURATION_HOURS: u32 = 10_000;

/// Parse duration string like "2:05" to minutes. Returns None for malformed
/// strings, out-of-range values, or arithmetic overflow.
fn parse_duration(s: &str) -> Option<u32> {
    let (h_str, m_str) = s.split_once(':')?;
    let hours: u32 = h_str.parse().ok()?;
    let mins: u32 = m_str.parse().ok()?;
    if hours > MAX_DURATION_HOURS || mins >= 60 {
        return None;
    }
    hours.checked_mul(60)?.checked_add(mins)
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
    fn test_rejects_mixed_brackets() {
        // Opening `[` must close with `]`; opening `<` must close with `>`.
        // Mixing them is malformed and should not match.
        assert!(extract_clocks("CLOCK: [2023-02-19 21:30>").is_empty());
        assert!(extract_clocks("CLOCK: <2023-02-19 21:30]").is_empty());
        assert!(extract_clocks("CLOCK: [2023-02-19 21:30>--<2023-02-19 23:35]").is_empty());
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

    #[test]
    fn test_parse_duration_rejects_invalid() {
        assert_eq!(parse_duration(""), None);
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("1:60"), None, "minutes must be < 60");
        assert_eq!(parse_duration("1:99"), None);
        assert_eq!(parse_duration("99999:00"), None, "hours capped");
        assert_eq!(parse_duration("1:2:3"), None, "single colon only");
    }

    #[test]
    fn test_calculate_total_overflow_protected() {
        // Even a very large but in-range duration shouldn't wrap. Two valid maxima
        // sum below u32::MAX, but if MAX_DURATION_HOURS were larger we'd want this
        // to return Some(_) without panic.
        let clocks = vec![
            ClockEntry {
                start: "x".to_string(),
                end: Some("y".to_string()),
                duration: Some("9999:59".to_string()),
            },
            ClockEntry {
                start: "x".to_string(),
                end: Some("y".to_string()),
                duration: Some("9999:59".to_string()),
            },
        ];
        let total = calculate_total_minutes(&clocks).unwrap();
        assert_eq!(total, (9999 * 60 + 59) * 2);
    }
}
