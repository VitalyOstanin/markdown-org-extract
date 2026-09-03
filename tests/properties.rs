//! Properties of the parsers that read a value someone else wrote.
//!
//! The exception keys of ADR-0031 hold free text out of another person's
//! file: `EXDATE` is a list written for a person to read, `RECURRENCE_ID` is a
//! date with an optional time after it. Examples cover the shapes that were
//! thought of; what is stated here holds for every input, which is the part an
//! example set cannot say. See TODO.md, "Property-based and fuzz tests" —
//! properties first, and over the parsers before anything else.
//!
//! The same holds of the phrase parser (ADR-0035, ADR-0036): it reads a
//! sentence a person said, and what it promises — that no word is lost, that
//! the last phrase to name a field wins, and that every value it prints reads
//! back — is stated over generated phrases rather than over a table of
//! examples.
//!
//! Case counts are set per property rather than left to the default, so the
//! run stays a fraction of a second and does not become the longest line of
//! `cargo test`.

use chrono::NaiveDate;
use markdown_org_extract::exceptions::{
    parse_excluded_dates, parse_recurrence_id, recurrence_id_date,
};
use markdown_org_extract::timestamp::parse_repeater;
use markdown_org_extract::{parse_phrases, refine_entry, PhraseEntry};
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

/// Monday 2026-08-31, the day the phrase properties are relative to.
fn reference_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, 31).expect("a date that exists")
}

/// The words the Russian grammar knows, from every rule it has, mixed with
/// words it knows nothing about. A phrase built from these is one a person
/// could say and one the parser has to survive.
const KNOWN_WORDS: &[&str] = &[
    "напомни",
    "мне",
    "добавь",
    "задачу",
    "перенеси",
    "сделай",
    "отметь",
    "сегодня",
    "завтра",
    "послезавтра",
    "в",
    "на",
    "к",
    "до",
    "через",
    "пятницу",
    "вторник",
    "15",
    "января",
    "2026-09-15",
    "каждые",
    "каждый",
    "2",
    "три",
    "недели",
    "день",
    "рабочий",
    "ежедневно",
    "часа",
    "дня",
    "15:00",
    "срочно",
    "очень",
    "важно",
    "приоритет",
    "b",
    "убрать",
    "дату",
    "без",
    "приоритета",
    "выполнено",
    "отменено",
    "работу",
    "и",
    "не",
];

/// A word of a phrase: one the grammar knows, or one it does not.
fn phrase_word() -> impl Strategy<Value = String> {
    prop_oneof![
        8 => prop::sample::select(KNOWN_WORDS).prop_map(str::to_string),
        2 => proptest::string::string_regex("[а-яa-z]{1,8}").expect("a valid generator pattern"),
    ]
}

/// A phrase of up to eight words.
fn any_phrase() -> impl Strategy<Value = String> {
    prop::collection::vec(phrase_word(), 0..8).prop_map(|words| words.join(" "))
}

/// Whether `part` appears inside `whole` in order, word by word.
fn is_a_subsequence_of(part: &str, whole: &str) -> bool {
    let mut words = whole.split_whitespace();

    part.split_whitespace()
        .all(|word| words.any(|candidate| candidate == word))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Nothing said is lost in silence: every word of the heading was said,
    /// in the order it was said in. ADR-0036 calls losing a phrase quietly
    /// the worst failure the parser has, and this is that statement over
    /// every input rather than over the chains of the table.
    #[test]
    fn a_phrase_keeps_the_words_it_did_not_consume_in_order(phrase in any_phrase()) {
        let entry = parse_phrases([phrase.as_str()], "ru", reference_day());

        prop_assert!(
            is_a_subsequence_of(&entry.heading, &phrase),
            "heading {:?} is not what was said in {:?}",
            entry.heading,
            phrase
        );
    }

    /// A phrase that names a field says what that field is, whatever stood
    /// before it: refining an entry a first phrase built gives the same value
    /// as refining the empty one. This is the rule of ADR-0036 — the chain
    /// says what its last phrase says — over generated pairs.
    #[test]
    fn the_later_phrase_wins_over_whatever_the_earlier_one_said(
        earlier in any_phrase(),
        later in any_phrase(),
    ) {
        let today = reference_day();
        let alone = refine_entry(PhraseEntry::default(), &later, "ru", today);
        let after = refine_entry(
            refine_entry(PhraseEntry::default(), &earlier, "ru", today),
            &later,
            "ru",
            today,
        );

        // A field the later phrase named — filled or emptied, which are the
        // two ways of naming one — stands as that phrase left it. A field it
        // said nothing about keeps whatever the earlier phrase did, which is
        // the other half of the same rule and not stated here.
        if alone.date.is_some() || alone.cleared.date {
            prop_assert_eq!(after.date, alone.date, "date, after {:?}", earlier);
            prop_assert_eq!(after.planning, alone.planning, "planning, after {:?}", earlier);
            prop_assert_eq!(
                after.cleared.date,
                alone.cleared.date,
                "date emptied, after {:?}",
                earlier
            );
        }
        if alone.time.is_some() || alone.cleared.time {
            prop_assert_eq!(after.time, alone.time, "time, after {:?}", earlier);
            prop_assert_eq!(
                after.cleared.time,
                alone.cleared.time,
                "time emptied, after {:?}",
                earlier
            );
        }
        if alone.repeater.is_some() || alone.cleared.repeater {
            prop_assert_eq!(after.repeater, alone.repeater, "repeater, after {:?}", earlier);
            prop_assert_eq!(
                after.cleared.repeater,
                alone.cleared.repeater,
                "repeater emptied, after {:?}",
                earlier
            );
        }
        if alone.priority.is_some() || alone.cleared.priority {
            prop_assert_eq!(after.priority, alone.priority, "priority, after {:?}", earlier);
            prop_assert_eq!(
                after.cleared.priority,
                alone.cleared.priority,
                "priority emptied, after {:?}",
                earlier
            );
        }
        if alone.keyword.is_some() {
            prop_assert_eq!(after.keyword, alone.keyword, "keyword, after {:?}", earlier);
        }
    }

    /// Every value the JSON of `parse-phrase` prints reads back: the date by
    /// `%Y-%m-%d`, the time by `%H:%M`, the repeater by the timestamp grammar
    /// that wrote it. Both clients read this payload and hand the values on
    /// to their own parsers.
    #[test]
    fn every_value_a_phrase_prints_reads_back(phrase in any_phrase()) {
        let entry = parse_phrases([phrase.as_str()], "ru", reference_day());
        let json = serde_json::to_value(&entry).expect("an entry serializes");

        if let Some(date) = json["date"].as_str() {
            prop_assert!(
                NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok(),
                "{date:?} came out of {phrase:?} and is not a date"
            );
        }
        if let Some(time) = json["time"].as_str() {
            prop_assert!(
                chrono::NaiveTime::parse_from_str(time, "%H:%M").is_ok(),
                "{time:?} came out of {phrase:?} and is not a time"
            );
        }
        if let Some(repeater) = json["repeater"].as_str() {
            prop_assert!(
                parse_repeater(repeater).is_some(),
                "{repeater:?} came out of {phrase:?} and the timestamp grammar refuses it"
            );
        }
        let cleared = json["cleared"].as_array().expect("cleared is a list");
        for name in cleared {
            let name = name.as_str().expect("a field name is a string");
            prop_assert!(
                ["date", "time", "repeater", "priority"].contains(&name),
                "{name:?} is not a field a phrase can empty"
            );
        }
    }

    /// A repeater said in words is one the timestamp grammar reads back, for
    /// every count and every unit — including the counts nobody would say.
    /// A value that only the phrase parser understands would be written into
    /// a file and lost on the next scan.
    #[test]
    fn every_repeater_a_phrase_yields_reads_back(
        count in 0..2000_u32,
        unit in prop::sample::select(&["дня", "недели", "месяца", "года", "рабочих дня"][..]),
    ) {
        let phrase = format!("зарядка каждые {count} {unit}");
        let entry = parse_phrases([phrase.as_str()], "ru", reference_day());

        let Some(repeater) = entry.repeater else {
            return Ok(());
        };
        let written = repeater.canonical();

        let read_back = parse_repeater(&written);

        prop_assert_eq!(
            read_back.as_ref(),
            Some(&repeater),
            "{:?} came out of {:?}",
            written,
            phrase
        );
    }
}
