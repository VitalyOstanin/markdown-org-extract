//! Properties of the parsers that read a value someone else wrote.
//!
//! The exception keys of ADR-0031 hold free text out of another person's
//! file: `EXDATE` is a list written for a person to read, `RECURRENCE_ID` is a
//! date with an optional time after it. Examples cover the shapes that were
//! thought of; what is stated here holds for every input, which is the part an
//! example set cannot say. See TODO.md, "Property-based and fuzz tests" —
//! properties first, and over the parsers before anything else.
//!
//! Case counts are set per property rather than left to the default, so the
//! run stays a fraction of a second and does not become the longest line of
//! `cargo test`.

use chrono::NaiveDate;
use markdown_org_extract::exceptions::{
    parse_excluded_dates, parse_recurrence_id, recurrence_id_date,
};
use proptest::prelude::*;

/// Any date the calendar has, as the parsers see it: a `YYYY-MM-DD` string.
///
/// Generated from the day number rather than from year/month/day parts, so
/// month lengths and leap years come out right without the generator having to
/// know about them.
fn any_date() -> impl Strategy<Value = NaiveDate> {
    // 0001-01-01 through 9999-12-31, the whole range `NaiveDate` holds and
    // `%Y-%m-%d` can write back without widening the year field.
    (1..=3_652_058_i32).prop_map(|days| {
        NaiveDate::from_num_days_from_ce_opt(days).expect("day number inside the calendar")
    })
}

/// A date out of one season, so a list built from these repeats itself often
/// enough for the properties about duplicates to be about something.
fn date_from_a_short_season() -> impl Strategy<Value = NaiveDate> {
    (0..30_i64).prop_map(|offset| {
        NaiveDate::from_ymd_opt(2026, 8, 1).expect("a date that exists")
            + chrono::Days::new(offset as u64)
    })
}

/// A field that is not a date and not a clock time: no `:` in it, so it cannot
/// be read as a time, and no `-` so it cannot begin to look like a date.
fn other_field() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-zA-Z0-9_.]{1,12}").expect("a valid generator pattern")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Whatever the value holds, reading it returns — and what it returns
    /// reads back as a date. A caller that hands these strings on (the JSON
    /// payload does, and both clients parse them) can rely on the form.
    #[test]
    fn every_date_an_exdate_yields_reads_back_as_one(raw in ".{0,200}") {
        let dates = parse_excluded_dates(&raw, |_| {});

        for date in &dates {
            prop_assert!(
                NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok(),
                "{date:?} came out of {raw:?} and is not a date"
            );
        }
    }

    /// Nothing is dropped in silence. Every non-empty field of a value that
    /// holds no times is either a date that was kept, a date already seen, or
    /// a field the caller was told about — the counts have to add up, or a
    /// reader is looking at an exception that lost part of what was written.
    #[test]
    fn an_exdate_without_times_accounts_for_every_field(
        fields in prop::collection::vec(
            prop_oneof![
                date_from_a_short_season().prop_map(|d| d.format("%Y-%m-%d").to_string()),
                other_field(),
            ],
            0..12,
        ),
    ) {
        let raw = fields.join(", ");
        let mut rejected = 0_usize;
        let dates = parse_excluded_dates(&raw, |_| rejected += 1);

        let distinct_dates = {
            let mut seen: Vec<&String> = fields
                .iter()
                .filter(|f| NaiveDate::parse_from_str(f, "%Y-%m-%d").is_ok())
                .collect();
            seen.sort_unstable();
            seen.dedup();
            seen.len()
        };
        let not_dates = fields
            .iter()
            .filter(|f| NaiveDate::parse_from_str(f, "%Y-%m-%d").is_err())
            .count();

        prop_assert_eq!(dates.len(), distinct_dates, "kept dates, from {:?}", raw);
        prop_assert_eq!(rejected, not_dates, "reported fields, from {:?}", raw);
    }

    /// A date written twice is one occurrence, however the value spelled it —
    /// the set the agenda matches against is built from this list, and a
    /// duplicate there is work repeated on every day drawn.
    #[test]
    fn an_exdate_holds_one_entry_per_date(
        date in any_date(),
        times in 1..6_usize,
    ) {
        let written = date.format("%Y-%m-%d").to_string();
        let raw = vec![written.clone(); times].join(" ");

        prop_assert_eq!(parse_excluded_dates(&raw, |_| {}), vec![written]);
    }

    /// The value `parse_recurrence_id` produces always names a day. This is
    /// the invariant `OccurrenceExceptions::from_tasks` is built on: it reads
    /// the date half back out with `recurrence_id_date` and would drop the
    /// exception where that answered `None`.
    #[test]
    fn a_recurrence_id_that_parsed_always_names_a_day(raw in ".{0,120}") {
        let Some(parsed) = parse_recurrence_id(&raw, |_| {}) else {
            return Ok(());
        };

        prop_assert!(
            recurrence_id_date(&parsed).is_some(),
            "{parsed:?} came out of {raw:?} and names no day"
        );
    }

    /// A date and a time survive the round trip whatever separates them, and
    /// the seconds a calendar export writes are cut to the minute occurrences
    /// are matched on.
    #[test]
    fn a_recurrence_id_keeps_the_day_and_the_minute(
        date in any_date(),
        hour in 0..24_u32,
        minute in 0..60_u32,
        seconds in prop::option::of(0..60_u32),
    ) {
        let clock = match seconds {
            Some(s) => format!("{hour:02}:{minute:02}:{s:02}"),
            None => format!("{hour:02}:{minute:02}"),
        };
        let raw = format!("{} {clock}", date.format("%Y-%m-%d"));
        let mut dropped = 0_usize;

        let parsed = parse_recurrence_id(&raw, |_| dropped += 1);
        let expected = format!("{} {hour:02}:{minute:02}", date.format("%Y-%m-%d"));

        prop_assert_eq!(parsed.as_deref(), Some(expected.as_str()));
        prop_assert_eq!(dropped, 0, "a time that reads is not a dropped tail");
    }
}
