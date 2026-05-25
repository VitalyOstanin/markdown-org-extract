use chrono::NaiveDate;
use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

use super::repeater::{parse_repeater, Repeater};
use super::weekdays::normalize_weekdays;
use crate::regex_limits::{compile_bounded, TS_BODY_MAX};

// Main bracket regexes (one per family, ADR-0014): anchor the
// `<YYYY-MM-DD ...>` (active) or `[YYYY-MM-DD ...]` (inactive) form and
// capture only the date. Body content (weekday, time, repeater, warning
// cookie) is left flexible and scanned separately so that order of
// repeater vs warning follows upstream Org-mode semantics
// (`org-get-wdays` in lisp/org.el just searches the whole timestamp
// string for `-N[hdwmy]`, irrespective of where the repeater sits).
// Date is mandatory; everything else is optional and order-independent.
//
// Range timestamps like `<...>--<...>` or `[...]--[...]` fall through
// these regexes naturally: each regex matches the first bracket and
// captures the start date, which is what `parse_org_timestamp` returns
// for ranges anyway. The `--?-?` separator and the trailing bracket are
// not consumed here; other code paths (e.g., the end-time extraction in
// `extract.rs`) handle ranges with their own regexes. Paired alternation
// (no `[<\[]...[>\]]` shortcut) keeps mixed pairs `<...]` / `[...>` from
// matching by construction.
static SINGLE_ANGLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    // The body quantifier shares `TS_BODY_MAX` with the extractor patterns
    // in `extract.rs`. Using the same bound on both sides guarantees that
    // every timestamp the extractor accepts also parses here: a literal
    // `80` once let bodies in the 81..=256 range pass extraction yet fail
    // to parse, silently dropping the task from every agenda bucket (F2 in
    // the 2026-05-25 logic review). `TS_BODY_MAX` is a defense-in-depth
    // ceiling, not a semantic limit; realistic bodies (full weekday name +
    // HH:MM-HH:MM range + repeater + warning cookie) stay well under it.
    // The square-bracket variant uses `[^\[\]]{0,TS_BODY_MAX}` identically.
    compile_bounded(&format!(
        r"<(\d{{4}}-\d{{2}}-\d{{2}})[^<>]{{0,{TS_BODY_MAX}}}>"
    ))
});

static SINGLE_SQUARE_RE: LazyLock<Regex> = LazyLock::new(|| {
    compile_bounded(&format!(
        r"\[(\d{{4}}-\d{{2}}-\d{{2}})[^\[\]]{{0,{TS_BODY_MAX}}}\]"
    ))
});

// Scan the bracket body for a repeater token. Matches upstream Org-mode
// `org-repeater-regexp-base` shape: a `+`, `++`, or `.+` prefix, followed
// by a positive integer, followed by a unit (d/w/m/y/h or `wd` for the
// project's workday extension).
static REPEATER_BODY_RE: LazyLock<Regex> =
    LazyLock::new(|| compile_bounded(r"([.+]+\d+(?:wd|[dwmyh]))"));

// Scan the bracket body for a warning-period cookie `-N[hdwmy]`.
//
// Upstream Emacs Org-mode `org-get-wdays` (lisp/org.el L14937) uses
// `-\([0-9]+\)\([hdwmy]\)\(\'\|>\| \)`: digits, a unit char, then a
// terminator that is end-of-string, `>`, or a literal space. This
// project's pattern keeps the same terminator idea with two
// deliberate divergences from upstream, recorded in ADR-0018
// ("Warning-cookie boundary divergence from upstream"):
//   1. A leading `\s` is *required* before `-`. Upstream relies on the
//      unit char + terminator alone to avoid matching the date's own
//      `-MM` / `-DD` runs (a date component is never followed by
//      `[hdwmy]`). Requiring a separator is stricter and fail-closed:
//      a cookie glued to preceding text (`...d-3d`) is ignored rather
//      than guessed at.
//   2. The terminator class is `[\s>\]]|$` instead of upstream's
//      ` |>|\'`. It adds `]` so an inactive `[... -3d]` cookie reads
//      the same as the active `<... -3d>` form, and uses `\s` (any
//      whitespace) rather than a single literal space.
// Consequences of (1)+(2): `-3day` is not a cookie (`a` not in the
// terminator class), and a pathological double cookie `-3d-2d` matches
// nothing here (the `-` after `3d` is not a terminator, and `-2d` has
// no leading `\s`), whereas upstream would extract the trailing `-2d`.
// These bodies are not produced by Emacs; the fail-closed reading is
// pinned by `warning_cookie_requires_separator_and_terminator`.
static WARNING_BODY_RE: LazyLock<Regex> =
    LazyLock::new(|| compile_bounded(r"\s-(\d+)([hdwmy])(?:[\s>\]]|$)"));

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

    // Try both bracket families and take whichever starts first. Both
    // regexes anchor on a date pattern, so the earlier match position is
    // the one a human reader would also pick.
    let angle = SINGLE_ANGLE_RE.captures(&ts);
    let square = SINGLE_SQUARE_RE.captures(&ts);
    let caps = match (&angle, &square) {
        (Some(a), Some(s)) => {
            if a.get(0).unwrap().start() <= s.get(0).unwrap().start() {
                angle.as_ref().unwrap()
            } else {
                square.as_ref().unwrap()
            }
        }
        (Some(_), None) => angle.as_ref().unwrap(),
        (None, Some(_)) => square.as_ref().unwrap(),
        (None, None) => return None,
    };
    let date = NaiveDate::parse_from_str(&caps[1], "%Y-%m-%d").ok()?;
    let bracket = caps.get(0).map(|m| m.as_str()).unwrap_or("");

    let repeater = REPEATER_BODY_RE
        .captures(bracket)
        .and_then(|c| parse_repeater(c.get(1)?.as_str()));

    let warning_days = WARNING_BODY_RE.captures(bracket).and_then(|c| {
        let value: i64 = c.get(1)?.as_str().parse().ok()?;
        warning_cookie_to_days(value, c.get(2)?.as_str())
    });

    // `<...>` ⇒ active, `[...]` ⇒ inactive. The opening byte is the
    // single source of truth because the two regex families never
    // produce mixed pairs.
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

    #[test]
    fn parse_org_timestamp_accepts_body_up_to_extractor_limit() {
        // Regression for F2 (2026-05-25 logic review). The extractor in
        // extract.rs bounds the timestamp body with TS_BODY_MAX (256), while
        // parse_org_timestamp used a literal 80. A body longer than 80 but
        // within TS_BODY_MAX was accepted on extraction yet failed to parse
        // here, so the task silently dropped out of every agenda bucket
        // (it stayed in `--tasks`). Both sides now share TS_BODY_MAX, so
        // "what passed extraction passes parsing" holds.
        let filler = " ".repeat(120); // after the date: > 80, < TS_BODY_MAX
        let ts = format!("<2024-12-09 Mon{filler}+1m>");
        assert!(
            ts.len() > 80 + 12,
            "test fixture must exceed the old 80-char body bound"
        );
        let parsed =
            parse_org_timestamp(&ts, None).expect("body within TS_BODY_MAX must still parse");
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 9).unwrap());
        assert!(
            parsed.repeater.is_some(),
            "a repeater after a long body must still be found"
        );
        assert!(parsed.active);
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

    #[test]
    fn warning_cookie_requires_separator_and_terminator() {
        // Pins the two deliberate divergences from upstream `org-get-wdays`
        // documented on WARNING_BODY_RE (ADR-0018, F5 in the 2026-05-25
        // logic review). The parser is fail-closed: a cookie is read only
        // when separated by whitespace and closed by whitespace / `>` /
        // `]` / end-of-string.

        // A well-formed, separated cookie is read (baseline).
        assert_eq!(
            parse_org_timestamp("<2025-12-10 Wed -3d>", None)
                .unwrap()
                .warning_days,
            Some(3)
        );

        // No leading separator: `-2d` is glued to the preceding `d`, so it
        // is NOT a cookie here. Upstream would extract the trailing `-2d`;
        // we deliberately read nothing.
        assert_eq!(
            parse_org_timestamp("<2025-12-10 Wed -3d-2d>", None)
                .unwrap()
                .warning_days,
            None,
            "double cookie -3d-2d must be fail-closed (no leading separator on -2d)"
        );

        // Unit char not followed by a terminator: `-3day` is a word, not a
        // cookie, because `a` is outside the terminator class.
        assert_eq!(
            parse_org_timestamp("<2025-12-10 Wed -3day>", None)
                .unwrap()
                .warning_days,
            None,
            "`-3day` is not a warning cookie"
        );
    }

    // ADR-0014: bracket form is reported via `active`. `<...>` = active,
    // `[...]` = inactive. Both forms parse the same internal fields
    // (date, repeater, warning days); the difference is only in the
    // bracket and in how downstream consumers (e.g., agenda) treat the
    // value (inactive never feeds agenda).
    #[test]
    fn parse_org_timestamp_marks_angle_bracket_as_active() {
        let parsed = parse_org_timestamp("<2025-12-10 Wed>", None).unwrap();
        assert!(parsed.active, "<...> timestamp must be active");
    }

    #[test]
    fn parse_org_timestamp_marks_square_bracket_as_inactive() {
        let parsed = parse_org_timestamp("[2025-12-10 Wed]", None).unwrap();
        assert!(!parsed.active, "[...] timestamp must be inactive");
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2025, 12, 10).unwrap());
    }

    #[test]
    fn parse_inactive_timestamp_with_repeater() {
        // Repeater grammar is identical inside `[...]` — REPEATER_BODY_RE
        // scans bracket-agnostic body text.
        let parsed = parse_org_timestamp("[2025-12-05 Thu +1d]", None).unwrap();
        let repeater = parsed.repeater.unwrap();
        assert_eq!(repeater.value, 1);
        assert_eq!(repeater.unit, super::super::repeater::RepeaterUnit::Day);
        assert!(!parsed.active);
    }

    #[test]
    fn parse_inactive_timestamp_with_warning_period() {
        // WARNING_BODY_RE must accept `]` as a terminator the same way it
        // accepts `>` and whitespace for active timestamps. Upstream Org-mode
        // never emits a warning cookie inside `[...]` (org-expiry / org-closed
        // do not carry warning days), but the parser is symmetric so an
        // author who chose to write `[... -3d]` is not silently ignored.
        let parsed = parse_org_timestamp("[2025-12-10 Wed -3d]", None).unwrap();
        assert_eq!(parsed.warning_days, Some(3));
        assert!(!parsed.active);
    }

    #[test]
    fn parse_inactive_timestamp_normalizes_localized_weekday() {
        // Weekday-normalization runs on the whole string and should not
        // care about bracket form. The `Cow::Borrowed` fast path applies
        // when no mapping changes the input, exactly as for `<...>`.
        let mappings: &[(&str, &str)] = &[("Чт", "Thu")];
        let parsed = parse_org_timestamp("[2025-12-05 Чт]", Some(mappings)).unwrap();
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2025, 12, 5).unwrap());
        assert!(!parsed.active);
    }

    #[test]
    fn parse_org_timestamp_prefers_first_bracket_form() {
        // If a line contains a square bracket before an angle bracket and
        // both look like valid timestamps, the first match wins. This
        // mirrors the existing behaviour for `<...>foo<...>` (first one
        // is taken). The test pins the precedence so a future regex
        // refactor cannot silently flip it.
        let parsed = parse_org_timestamp("[2025-12-05 Thu] <2025-12-06 Fri>", None).unwrap();
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2025, 12, 5).unwrap());
        assert!(!parsed.active);
    }
}
