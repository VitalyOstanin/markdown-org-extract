use chrono::NaiveDate;
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

use super::repeater::{parse_repeater, Repeater};
use super::weekdays::normalize_weekdays;
use crate::regex_limits::compile_bounded;

// Main bracket regex: anchor the `<YYYY-MM-DD ...>` form and capture only
// the date. Body content (weekday, time, repeater, warning cookie) is left
// flexible and scanned separately so that order of repeater vs warning
// follows upstream Org-mode semantics (`org-get-wdays` in lisp/org.el
// just searches the whole timestamp string for `-N[hdwmy]`, irrespective
// of where the repeater sits). Date is mandatory; everything else is
// optional and order-independent.
//
// Range timestamps like `<...>--<...>` fall through this regex naturally:
// SINGLE_RE matches the first bracket and captures the start date,
// which is what `parse_org_timestamp` returns for ranges anyway. The
// `--?-?` separator and the trailing bracket are not consumed here;
// other code paths (e.g., the end-time extraction in `extract.rs`)
// handle ranges with their own regexes.
static SINGLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    // `[^<>]{0,80}` bounds the body so a stray `<` or pathological input
    // cannot blow up the regex engine. 80 chars accommodates the longest
    // realistic timestamp body (full weekday name + HH:MM-HH:MM range +
    // repeater + warning cookie).
    compile_bounded(r"<(\d{4}-\d{2}-\d{2})[^<>]{0,80}>")
});

// Scan the bracket body for a repeater token. Matches upstream Org-mode
// `org-repeater-regexp-base` shape: a `+`, `++`, or `.+` prefix, followed
// by a positive integer, followed by a unit (d/w/m/y/h or `wd` for the
// project's workday extension).
static REPEATER_BODY_RE: LazyLock<Regex> =
    LazyLock::new(|| compile_bounded(r"([.+]+\d+(?:wd|[dwmyh]))"));

// Scan the bracket body for a warning-period cookie `-N[hdwmy]`. The
// trailing context `[\s>]|$` mirrors upstream `org-get-wdays`'s
// `\\(\\'\\|>\\| \\)` so a substring like `-3day` (not a cookie) is not
// matched as `-3d`.
static WARNING_BODY_RE: LazyLock<Regex> =
    LazyLock::new(|| compile_bounded(r"\s-(\d+)([hdwmy])(?:[\s>]|$)"));

/// Result of parsing a single org-mode timestamp string.
#[derive(Debug, Clone)]
pub struct ParsedTimestamp {
    /// The base date encoded in the timestamp (start date for ranges).
    pub date: NaiveDate,
    /// Optional repeater (`+1d`, `.+2w`, ...).
    pub repeater: Option<Repeater>,
    /// Optional per-task warning lead time (`-Nd`, `-Nw`, `-Nm`, `-Ny`,
    /// `-Nh`) converted to whole days using upstream Org-mode's factors
    /// (see `org-get-wdays` in `lisp/org.el`). When set, it overrides the
    /// global `DEADLINE_WARNING_DAYS` for the corresponding DEADLINE.
    pub warning_days: Option<i64>,
    /// Bracket form: `true` for active `<...>`, `false` for inactive
    /// `[...]`. See ADR-0014 for which keywords accept which forms and
    /// for the agenda invariant (inactive timestamps never feed agenda).
    pub active: bool,
}

/// Convert a warning cookie's value/unit pair into whole days, mirroring
/// upstream `org-get-wdays`: `floor(N * factor)` with day-equivalents
/// `d=1`, `w=7`, `m=30.4`, `y=365.25`, `h=1/24`. Returns `None` for any
/// unrecognised unit, which keeps the parser fail-closed.
fn warning_cookie_to_days(value: i64, unit: &str) -> Option<i64> {
    let factor = match unit {
        "d" => 1.0,
        "w" => 7.0,
        "m" => 30.4,
        "y" => 365.25,
        "h" => 1.0 / 24.0,
        _ => return None,
    };
    Some((value as f64 * factor).floor() as i64)
}

/// Parse a single org-mode timestamp like `<2024-12-05 Thu 10:00 +1d>` or
/// `<2024-12-05>--<2024-12-06>`, optionally normalizing localized weekday names.
///
/// Repeater and warning-period cookies are extracted by independent passes
/// on the bracket body, so they may appear in either order
/// (`<... +1y -3d>` or `<... -3d +1y>`), matching upstream Org-mode's
/// position-agnostic handling in `org-get-wdays`.
pub fn parse_org_timestamp(ts: &str, mappings: Option<&[(&str, &str)]>) -> Option<ParsedTimestamp> {
    let ts = if let Some(m) = mappings {
        normalize_weekdays(ts, m)
    } else {
        Cow::Borrowed(ts)
    };

    let caps = SINGLE_RE.captures(&ts)?;
    let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
    let bracket = caps.get(0).map(|m| m.as_str()).unwrap_or("");

    let repeater = REPEATER_BODY_RE
        .captures(bracket)
        .and_then(|c| parse_repeater(c.get(1)?.as_str()));

    let warning_days = WARNING_BODY_RE.captures(bracket).and_then(|c| {
        let value: i64 = c.get(1)?.as_str().parse().ok()?;
        warning_cookie_to_days(value, c.get(2)?.as_str())
    });

    // SINGLE_RE currently only matches `<...>`, so `active` is `true` for
    // every successful parse. When the regex is extended to accept `[...]`
    // (see ADR-0014 and the regex-update task), `active` will be derived
    // from the opening bracket captured by the same regex.
    let active = bracket.starts_with('<');

    Some(ParsedTimestamp {
        date,
        repeater,
        warning_days,
        active,
    })
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

    #[test]
    fn range_separator_accepts_one_two_three_dashes() {
        // Emacs' org-tr-regexp uses `--?-?`, i.e. one, two, or three dashes
        // between the bracketed values. parse_org_timestamp must accept all
        // three; the start date alone is surfaced (end-date support is a
        // separate concern, documented in README + ADR-0002).
        for sep in ["-", "--", "---"] {
            let ts = format!("<2025-12-05 Thu>{sep}<2025-12-06 Fri>");
            let parsed = parse_org_timestamp(&ts, None)
                .unwrap_or_else(|| panic!("must parse range with {sep:?} as separator"));
            assert_eq!(
                parsed.date,
                NaiveDate::from_ymd_opt(2025, 12, 5).unwrap(),
                "start date must be the first bracket for separator {sep:?}"
            );
        }
    }

    // Warning-period cookie semantics mirror upstream Emacs Org-mode's
    // `org-get-wdays` (lisp/org.el L14937-14943): `-N<unit>` where unit is
    // one of h/d/w/m/y, converted to days as floor(N * factor) with
    // factors d=1, w=7, m=30.4, y=365.25, h=1/24. The presence of the
    // cookie on a DEADLINE overrides the global `DEADLINE_WARNING_DAYS`
    // for that one task.

    #[test]
    fn parse_without_warning_period_yields_none() {
        let parsed = parse_org_timestamp("<2025-12-10 Wed>", None).unwrap();
        assert_eq!(parsed.warning_days, None);
    }

    #[test]
    fn parse_warning_period_days() {
        let parsed = parse_org_timestamp("<2025-12-10 Wed -3d>", None).unwrap();
        assert_eq!(parsed.warning_days, Some(3));
    }

    #[test]
    fn parse_warning_period_weeks() {
        // 1w = 7d (floor(1 * 7))
        let parsed = parse_org_timestamp("<2025-12-10 Wed -2w>", None).unwrap();
        assert_eq!(parsed.warning_days, Some(14));
    }

    #[test]
    fn parse_warning_period_months_floored() {
        // 1m = floor(30.4) = 30
        let parsed = parse_org_timestamp("<2025-12-10 Wed -1m>", None).unwrap();
        assert_eq!(parsed.warning_days, Some(30));
    }

    #[test]
    fn parse_warning_period_years_floored() {
        // 1y = floor(365.25) = 365
        let parsed = parse_org_timestamp("<2025-12-10 Wed -1y>", None).unwrap();
        assert_eq!(parsed.warning_days, Some(365));
    }

    #[test]
    fn parse_warning_period_hours_floored_to_zero_for_small_n() {
        // 1h = floor(1/24) = 0. Edge case, but matches upstream's
        // floor-semantics so the agenda code can treat 0 as "show only
        // on the day itself".
        let parsed = parse_org_timestamp("<2025-12-10 Wed -1h>", None).unwrap();
        assert_eq!(parsed.warning_days, Some(0));
    }

    #[test]
    fn parse_warning_period_with_repeater_in_either_order() {
        // Both orderings must be recognised: upstream `org-get-wdays`
        // scans the full bracket body without caring whether the repeater
        // sits before or after the warning cookie.
        let with_repeater_first = parse_org_timestamp("<2025-12-10 Wed +1d -3d>", None).unwrap();
        assert_eq!(with_repeater_first.warning_days, Some(3));
        assert!(with_repeater_first.repeater.is_some());

        let with_warning_first = parse_org_timestamp("<2025-12-10 Wed -3d +1d>", None).unwrap();
        assert_eq!(with_warning_first.warning_days, Some(3));
        assert!(with_warning_first.repeater.is_some());
    }

    // ADR-0014 adds an `active` flag to ParsedTimestamp. SINGLE_RE today
    // matches only `<...>`, so every successful parse yields `active = true`.
    // The inactive branch becomes reachable when the regex is widened in
    // a follow-up task; pinning the current behaviour here protects the
    // agenda invariant (inactive never feeds agenda) once that lands.
    #[test]
    fn parse_org_timestamp_marks_angle_bracket_as_active() {
        let parsed = parse_org_timestamp("<2025-12-10 Wed>", None).unwrap();
        assert!(parsed.active, "<...> timestamp must be active");
    }
}
