use regex::Regex;
use std::sync::LazyLock;

use super::weekdays::normalize_weekdays;
use crate::regex_limits::{compile_bounded, TS_BODY_MAX};

// `[^>]{0,TS_BODY_MAX}` caps the body length of a single bracketed timestamp
// so that a hostile or malformed line cannot make `[^>]*` scan thousands of
// characters before the engine notices the missing `>`.
static TIMESTAMP_RE: LazyLock<Regex> = LazyLock::new(|| {
    compile_bounded(&format!(
        r"^\s*((?:SCHEDULED|DEADLINE|CLOSED):\s*)<(\d{{4}}-\d{{2}}-\d{{2}}[^>]{{0,{TS_BODY_MAX}}})>"
    ))
});

// Range-timestamp separator matches Emacs' org-tr-regexp: one, two, or three
// dashes between the two bracketed values. The output is always canonicalised
// to the two-dash form, which is the variant produced by Emacs `org-time-stamp`.
static RANGE_TIMESTAMP_RE: LazyLock<Regex> = LazyLock::new(|| {
    compile_bounded(&format!(
        r"^\s*<(\d{{4}}-\d{{2}}-\d{{2}}[^>]{{0,{TS_BODY_MAX}}})>--?-?<(\d{{4}}-\d{{2}}-\d{{2}}[^>]{{0,{TS_BODY_MAX}}})>"
    ))
});

static SIMPLE_TIMESTAMP_RE: LazyLock<Regex> = LazyLock::new(|| {
    compile_bounded(&format!(
        r"^\s*<(\d{{4}}-\d{{2}}-\d{{2}}[^>]{{0,{TS_BODY_MAX}}})>"
    ))
});

static CREATED_RE: LazyLock<Regex> = LazyLock::new(|| {
    compile_bounded(&format!(
        r"^\s*CREATED:\s*<(\d{{4}}-\d{{2}}-\d{{2}}[^>]{{0,{TS_BODY_MAX}}})>"
    ))
});

static DATE_RE: LazyLock<Regex> = LazyLock::new(|| compile_bounded(r"\b(\d{4}-\d{2}-\d{2})"));

static TIME_RANGE_RE: LazyLock<Regex> =
    LazyLock::new(|| compile_bounded(r"\b(\d{1,2}:\d{2})-(\d{1,2}:\d{2})\b"));

static TIME_SINGLE_RE: LazyLock<Regex> = LazyLock::new(|| compile_bounded(r"\b(\d{1,2}:\d{2})\b"));

/// Extract CREATED timestamp from already-weekday-normalized text. Callers in
/// the parser pre-normalize so multiple extractors share one scan; tests pass
/// already-English input.
pub fn extract_created_normalized(text: &str) -> Option<String> {
    // Fast path: every match of CREATED_RE begins with optional whitespace
    // and then literal `CREATED:`. Bail out before paying the regex engine
    // when the leading non-space byte cannot start that keyword.
    if !text.trim_start().starts_with("CREATED:") {
        return None;
    }
    CREATED_RE
        .captures(text)
        .map(|caps| format!("CREATED: <{}>", &caps[1]))
}

/// Extract non-CREATED timestamp from already-weekday-normalized text.
pub fn extract_timestamp_normalized(text: &str) -> Option<String> {
    // Fast path: every regex below anchors to one of the keyword prefixes
    // `SCHEDULED:` / `DEADLINE:` / `CLOSED:` (TIMESTAMP_RE) or to the literal
    // `<` of a bare timestamp (RANGE_TIMESTAMP_RE / SIMPLE_TIMESTAMP_RE).
    // A byte check on the first non-whitespace byte short-circuits the
    // common case where an inline-code line is unrelated free text, sparing
    // three regex compilations of input we cannot match.
    let trimmed = text.trim_start();
    match trimmed.as_bytes().first() {
        Some(b'S' | b'D' | b'C' | b'<') => {}
        _ => return None,
    }

    if let Some(caps) = TIMESTAMP_RE.captures(text) {
        return Some(format!("{}<{}>", &caps[1], &caps[2]));
    }

    if let Some(caps) = RANGE_TIMESTAMP_RE.captures(text) {
        return Some(format!("<{}>--<{}>", &caps[1], &caps[2]));
    }

    if let Some(caps) = SIMPLE_TIMESTAMP_RE.captures(text) {
        return Some(format!("<{}>", &caps[1]));
    }

    None
}

/// Parse timestamp fields for JSON output.
///
/// Returns `(timestamp_type, date, time, end_time, active)`.
///
/// `active` is `Some(true)` for an active timestamp `<...>`, `Some(false)`
/// for an inactive one `[...]`, and `None` when the input does not contain
/// a recognisable opening bracket. The bracket form is detected on the
/// first `<` / `[` after the keyword prefix; see ADR-0014 for the
/// per-keyword policy.
///
/// For range timestamps like `<2024-12-05 10:00>--<2024-12-06 14:00>` the result is
/// `(_, Some("2024-12-05"), Some("10:00"), Some("14:00"), _)` — i.e. the second bracket's
/// start time is treated as `end_time`. For inline ranges `<2024-12-05 10:00-12:00>`
/// the explicit range form is used.
// The 5-tuple is grandfathered: callers in `parser.rs` and the test suite
// already destructure it. A struct refactor is tracked separately and does
// not block the active-flag addition (ADR-0014).
#[allow(clippy::type_complexity)]
pub fn parse_timestamp_fields(
    timestamp: &str,
    mappings: &[(&str, &str)],
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<bool>,
) {
    let ts_type = detect_ts_type(timestamp);
    let active = detect_active(timestamp);
    let normalized = normalize_weekdays(timestamp, mappings);

    // Handle ranges: <...>--<...>
    if let Some((first, second)) = split_range(&normalized) {
        let date = DATE_RE.captures(first).map(|c| c[1].to_string());
        let (time, end_from_first) = extract_time_pair(first);
        // If the first bracket already has a range like 10:00-12:00 — keep it.
        // Otherwise use the start time of the second bracket as end_time.
        let end_time = if end_from_first.is_some() {
            end_from_first
        } else {
            extract_time_pair(second).0
        };
        return (ts_type, date, time, end_time, active);
    }

    let date = DATE_RE.captures(&normalized).map(|c| c[1].to_string());
    let (time, end_time) = extract_time_pair(&normalized);
    (ts_type, date, time, end_time, active)
}

fn detect_active(timestamp: &str) -> Option<bool> {
    // The first `<` or `[` after any keyword prefix decides the form.
    // Whichever comes first wins; a string with neither yields `None`.
    let lt = timestamp.find('<');
    let lb = timestamp.find('[');
    match (lt, lb) {
        (Some(i), Some(j)) => Some(i < j),
        (Some(_), None) => Some(true),
        (None, Some(_)) => Some(false),
        (None, None) => None,
    }
}

fn detect_ts_type(timestamp: &str) -> Option<String> {
    // Anchor on the SCHEDULED:/DEADLINE:/CLOSED: prefix at the very start; this
    // prevents misclassification when the body contains a literal "SCHEDULED:".
    let trimmed = timestamp.trim_start();
    if trimmed.starts_with("SCHEDULED:") {
        Some("SCHEDULED".to_string())
    } else if trimmed.starts_with("DEADLINE:") {
        Some("DEADLINE".to_string())
    } else if trimmed.starts_with("CLOSED:") {
        Some("CLOSED".to_string())
    } else {
        Some("PLAIN".to_string())
    }
}

fn split_range(s: &str) -> Option<(&str, &str)> {
    // Find a "<...>(--?-?)<...>" pattern and return the inner bodies (without
    // angle brackets). The dash count matches Emacs' org-tr-regexp: one, two,
    // or three dashes; the canonical wire form is two.
    let start = s.find('<')?;
    let after_first = &s[start + 1..];
    let end_first_rel = after_first.find('>')?;
    let first_body = &after_first[..end_first_rel];
    let rest = &after_first[end_first_rel + 1..];
    let rest = rest.strip_prefix('-')?;
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    let rest = rest.strip_prefix('-').unwrap_or(rest);
    let rest = rest.strip_prefix('<')?;
    let end_second_rel = rest.find('>')?;
    let second_body = &rest[..end_second_rel];
    Some((first_body, second_body))
}

fn extract_time_pair(s: &str) -> (Option<String>, Option<String>) {
    if let Some(c) = TIME_RANGE_RE.captures(s) {
        return (Some(c[1].to_string()), Some(c[2].to_string()));
    }
    if let Some(c) = TIME_SINGLE_RE.captures(s) {
        return (Some(c[1].to_string()), None);
    }
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract_timestamp(text: &str, mappings: &[(&str, &str)]) -> Option<String> {
        extract_timestamp_normalized(&normalize_weekdays(text, mappings))
    }
    fn extract_created(text: &str, mappings: &[(&str, &str)]) -> Option<String> {
        extract_created_normalized(&normalize_weekdays(text, mappings))
    }

    #[test]
    fn extract_timestamp_normalized_short_circuits_free_text() {
        // Free-text inline code that cannot start any of the recognised
        // prefixes (S/D/C/<) must not even reach the regex engine. The
        // assertion is observable through return value only; the perf
        // win lives in the absent regex calls. Pin the contract here so
        // a refactor that drops the prefix gate does not regress quietly.
        assert!(extract_timestamp_normalized("just some inline text").is_none());
        assert!(extract_timestamp_normalized("`code that mentions foo bar`").is_none());
        // Leading whitespace is allowed before the prefix.
        assert!(extract_timestamp_normalized("    <2024-12-05 Thu>").is_some());
    }

    #[test]
    fn extract_created_normalized_short_circuits_free_text() {
        // CREATED has its own fast path: any leading-non-whitespace that is
        // not literal `CREATED:` short-circuits before the regex.
        assert!(extract_created_normalized("inline code without CREATED").is_none());
        assert!(extract_created_normalized("SCHEDULED: <2024-12-05>").is_none());
        assert!(extract_created_normalized("CREATED: <2024-12-05 Thu>").is_some());
    }

    #[test]
    fn extract_timestamp_simple_scheduled() {
        let ts = extract_timestamp("SCHEDULED: <2024-12-05 Thu 10:00>", &[]).unwrap();
        assert_eq!(ts, "SCHEDULED: <2024-12-05 Thu 10:00>");
    }

    #[test]
    fn extract_timestamp_range() {
        let ts = extract_timestamp("<2024-12-05 Thu 10:00>--<2024-12-06 Fri 14:00>", &[]).unwrap();
        assert_eq!(ts, "<2024-12-05 Thu 10:00>--<2024-12-06 Fri 14:00>");
    }

    #[test]
    fn extract_timestamp_range_one_dash() {
        // Emacs' org-tr-regexp accepts a single dash between the two bracketed
        // values (`--?-?`). The output is canonicalised back to two dashes,
        // matching the form produced by Emacs' `org-time-stamp` and the rest of
        // this project's wire format.
        let ts = extract_timestamp("<2024-12-05 Thu>-<2024-12-06 Fri>", &[]).unwrap();
        assert_eq!(ts, "<2024-12-05 Thu>--<2024-12-06 Fri>");
    }

    #[test]
    fn extract_timestamp_range_three_dashes() {
        let ts = extract_timestamp("<2024-12-05 Thu>---<2024-12-06 Fri>", &[]).unwrap();
        assert_eq!(ts, "<2024-12-05 Thu>--<2024-12-06 Fri>");
    }

    #[test]
    fn parse_fields_range_one_dash_recovers_second_time() {
        // Same regression coverage as the two-dash range, but for the single-
        // dash variant that Emacs also accepts.
        let (_, date, time, end_time, _) =
            parse_timestamp_fields("<2024-12-05 Thu 10:00>-<2024-12-06 Fri 14:00>", &[]);
        assert_eq!(date, Some("2024-12-05".to_string()));
        assert_eq!(time, Some("10:00".to_string()));
        assert_eq!(end_time, Some("14:00".to_string()));
    }

    #[test]
    fn extract_timestamp_localized_weekday() {
        let mappings = [("Чт", "Thu")];
        let ts = extract_timestamp("DEADLINE: <2024-12-05 Чт>", &mappings).unwrap();
        assert_eq!(ts, "DEADLINE: <2024-12-05 Thu>");
    }

    #[test]
    fn extract_created_basic() {
        let c = extract_created("CREATED: <2024-12-05 Thu>", &[]).unwrap();
        assert_eq!(c, "CREATED: <2024-12-05 Thu>");
    }

    #[test]
    fn extract_created_returns_none_on_other() {
        assert!(extract_created("SCHEDULED: <2024-12-05>", &[]).is_none());
    }

    #[test]
    fn parse_fields_scheduled_with_time() {
        let (ts_type, date, time, end_time, _) =
            parse_timestamp_fields("SCHEDULED: <2024-12-05 Thu 10:00>", &[]);
        assert_eq!(ts_type, Some("SCHEDULED".to_string()));
        assert_eq!(date, Some("2024-12-05".to_string()));
        assert_eq!(time, Some("10:00".to_string()));
        assert_eq!(end_time, None);
    }

    #[test]
    fn parse_fields_inline_time_range() {
        let (_, _, time, end_time, _) =
            parse_timestamp_fields("SCHEDULED: <2024-12-05 Thu 10:00-12:00>", &[]);
        assert_eq!(time, Some("10:00".to_string()));
        assert_eq!(end_time, Some("12:00".to_string()));
    }

    #[test]
    fn parse_fields_range_timestamp_recovers_second_time() {
        // Regression: <... 10:00>--<... 14:00> used to lose 14:00. Now it must surface as end_time.
        let (ts_type, date, time, end_time, _) =
            parse_timestamp_fields("<2024-12-05 Thu 10:00>--<2024-12-06 Fri 14:00>", &[]);
        assert_eq!(ts_type, Some("PLAIN".to_string()));
        assert_eq!(date, Some("2024-12-05".to_string()));
        assert_eq!(time, Some("10:00".to_string()));
        assert_eq!(end_time, Some("14:00".to_string()));
    }

    #[test]
    fn parse_fields_range_inline_takes_precedence() {
        // If first bracket already has a 10:00-12:00 range, use it; ignore the second bracket time.
        let (_, _, time, end_time, _) =
            parse_timestamp_fields("<2024-12-05 Thu 10:00-12:00>--<2024-12-06 Fri 14:00>", &[]);
        assert_eq!(time, Some("10:00".to_string()));
        assert_eq!(end_time, Some("12:00".to_string()));
    }

    #[test]
    fn detect_ts_type_does_not_match_body_substring() {
        // Regression: previously `.contains("SCHEDULED:")` was used and would misclassify
        // a CREATED timestamp whose body mentioned SCHEDULED.
        let (ts_type, _, _, _, _) =
            parse_timestamp_fields("CREATED: <2024-12-05 see SCHEDULED:>", &[]);
        assert_eq!(ts_type, Some("PLAIN".to_string()));
    }

    #[test]
    fn parse_fields_no_time() {
        let (_, date, time, end_time, _) =
            parse_timestamp_fields("DEADLINE: <2024-12-05 Thu>", &[]);
        assert_eq!(date, Some("2024-12-05".to_string()));
        assert_eq!(time, None);
        assert_eq!(end_time, None);
    }

    // ADR-0014: `active` reports the bracket form so consumers can branch
    // on it. SINGLE_RE accepts only `<...>` today, so production parses
    // always yield Some(true); the inactive case is constructed directly
    // from a string to pin `detect_active`'s behaviour for the future
    // regex update.

    #[test]
    fn parse_fields_marks_angle_bracket_keyword_as_active() {
        let (_, _, _, _, active) =
            parse_timestamp_fields("SCHEDULED: <2024-12-05 Thu>", &[]);
        assert_eq!(active, Some(true));
    }

    #[test]
    fn parse_fields_marks_square_bracket_keyword_as_inactive() {
        // `parse_timestamp_fields` itself does not gate on the keyword
        // policy from ADR-0014 — it only reports the form that was seen.
        // The regex layer (separate task) is what will accept or reject
        // each keyword/form combination. Pinning behaviour here keeps
        // `detect_active` honest once the regex change lands.
        let (_, _, _, _, active) =
            parse_timestamp_fields("CLOSED: [2024-12-05 Thu 14:30]", &[]);
        assert_eq!(active, Some(false));
    }

    #[test]
    fn parse_fields_marks_inline_plain_active() {
        let (_, _, _, _, active) = parse_timestamp_fields("<2024-12-05 Thu>", &[]);
        assert_eq!(active, Some(true));
    }

    #[test]
    fn parse_fields_marks_inline_plain_inactive() {
        let (_, _, _, _, active) = parse_timestamp_fields("[2024-12-05 Thu]", &[]);
        assert_eq!(active, Some(false));
    }

    #[test]
    fn parse_fields_no_bracket_returns_none_active() {
        // A string with neither `<` nor `[` cannot have a bracket form.
        let (_, _, _, _, active) = parse_timestamp_fields("not a timestamp", &[]);
        assert_eq!(active, None);
    }

    #[test]
    fn timestamp_body_within_limit_is_accepted() {
        use crate::regex_limits::TS_BODY_MAX;
        // Build a timestamp whose body length after the date is exactly the cap.
        // Body chars must satisfy `[^>]`, so use ASCII spaces.
        let filler = " ".repeat(TS_BODY_MAX);
        let input = format!("SCHEDULED: <2024-12-05{filler}>");
        let ts = extract_timestamp(&input, &[]).expect("should match at exactly the cap");
        // Body length = "2024-12-05" (10) + filler (TS_BODY_MAX).
        assert!(ts.contains("2024-12-05"));
        assert_eq!(ts.len(), "SCHEDULED: <>".len() + 10 + TS_BODY_MAX);
    }

    #[test]
    fn timestamp_body_just_over_limit_is_rejected() {
        use crate::regex_limits::TS_BODY_MAX;
        // One char past the cap and without a closing `>` after the cap window
        // must NOT match — proves the upper bound is enforced.
        let filler = " ".repeat(TS_BODY_MAX + 1);
        let input = format!("SCHEDULED: <2024-12-05{filler}>");
        assert!(
            extract_timestamp(&input, &[]).is_none(),
            "body of TS_BODY_MAX+1 chars must not match"
        );
    }
}
