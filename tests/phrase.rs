//! The table of phrase chains behind [ADR-0035] and [ADR-0036].
//!
//! Every row is a chain: the phrases a person says one after another, and the
//! entry they leave behind. A chain is a stricter check than the same phrases
//! taken apart, because it pins down what a later phrase leaves alone as well
//! as what it changes.
//!
//! Columns, in order:
//!
//! | phrases | heading | priority | planning | date | time | repeater |
//!
//! `-` stands for a field the chain never fills. Every chain is relative to
//! [`REFERENCE_DAY`], Monday 2026-08-31, so a weekday or a bare month day
//! resolves to a fixed answer rather than to whatever today happens to be.
//!
//! [`EDITS`] is the second table, for the phrases that change an entry that
//! exists. It states two more columns — the keyword and the emptied fields —
//! and the heading, which for an edit is not a heading but the leftover a
//! caller refuses the phrase over.
//!
//! [ADR-0035]: ../docs/adr/0035-a-phrase-is-parsed-into-an-entry-by-rules.md
//! [ADR-0036]: ../docs/adr/0036-a-later-phrase-refines-the-entry.md

use chrono::NaiveDate;
use markdown_org_extract::{parse_phrases, refine_entry, PhraseEntry, PhraseKeyword, PlanningKind};

/// Monday, 2026-08-31. "Tomorrow" is 2026-09-01 (Tuesday), the coming Friday
/// is 2026-09-04, and "in 3 days" is 2026-09-03.
const REFERENCE_DAY: (i32, u32, u32) = (2026, 8, 31);

fn reference_day() -> NaiveDate {
    NaiveDate::from_ymd_opt(REFERENCE_DAY.0, REFERENCE_DAY.1, REFERENCE_DAY.2).expect("valid day")
}

/// The six fields of a parsed entry as strings, so a table row can state them
/// literally. An absent field is `-`.
fn shown(entry: &PhraseEntry) -> [String; 6] {
    let dash = || "-".to_string();
    [
        entry.heading.clone(),
        entry.priority.as_ref().map_or_else(dash, |p| p.to_string()),
        entry.planning.as_ref().map_or_else(dash, |kind| {
            match kind {
                PlanningKind::Scheduled => "scheduled",
                PlanningKind::Deadline => "deadline",
            }
            .to_string()
        }),
        entry.date.map_or_else(dash, |d| d.to_string()),
        entry
            .time
            .map_or_else(dash, |t| t.format("%H:%M").to_string()),
        entry.repeater.as_ref().map_or_else(dash, |r| r.canonical()),
    ]
}

type Chain = (
    &'static [&'static str],
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

/// phrases, heading, priority, planning, date, time, repeater
const CHAINS: &[Chain] = &[
    // --- Russian: one phrase ---------------------------------------------
    (
        &["позвонить врачу завтра в 15:00"],
        "позвонить врачу",
        "-",
        "scheduled",
        "2026-09-01",
        "15:00",
        "-",
    ),
    (
        &["отчёт послезавтра"],
        "отчёт",
        "-",
        "scheduled",
        "2026-09-02",
        "-",
        "-",
    ),
    (
        &["уборка сегодня"],
        "уборка",
        "-",
        "scheduled",
        "2026-08-31",
        "-",
        "-",
    ),
    (
        &["планёрка в понедельник"],
        "планёрка",
        "-",
        "scheduled",
        "2026-08-31",
        "-",
        "-",
    ),
    (
        &["созвон во вторник в три часа дня"],
        "созвон",
        "-",
        "scheduled",
        "2026-09-01",
        "15:00",
        "-",
    ),
    (
        &["сдать отчёт к пятнице"],
        "сдать отчёт",
        "-",
        "deadline",
        "2026-09-04",
        "-",
        "-",
    ),
    (
        &["оплатить счёт до 15 сентября"],
        "оплатить счёт",
        "-",
        "deadline",
        "2026-09-15",
        "-",
        "-",
    ),
    (
        &["позвонить через 3 дня"],
        "позвонить",
        "-",
        "scheduled",
        "2026-09-03",
        "-",
        "-",
    ),
    (
        &["отпуск через неделю"],
        "отпуск",
        "-",
        "scheduled",
        "2026-09-07",
        "-",
        "-",
    ),
    (
        &["встреча 15 сентября"],
        "встреча",
        "-",
        "scheduled",
        "2026-09-15",
        "-",
        "-",
    ),
    (
        &["встреча 1 сентября 2026"],
        "встреча",
        "-",
        "scheduled",
        "2026-09-01",
        "-",
        "-",
    ),
    (
        &["созвон 2026-09-15"],
        "созвон",
        "-",
        "scheduled",
        "2026-09-15",
        "-",
        "-",
    ),
    (
        &["зарядка каждый день в 7:00"],
        "зарядка",
        "-",
        "-",
        "-",
        "07:00",
        "+1d",
    ),
    (
        &["оплата каждые 2 недели"],
        "оплата",
        "-",
        "-",
        "-",
        "-",
        "+2w",
    ),
    (&["уборка еженедельно"], "уборка", "-", "-", "-", "-", "+1w"),
    (
        &["зарядка каждый рабочий день"],
        "зарядка",
        "-",
        "-",
        "-",
        "-",
        "+1wd",
    ),
    (
        &["срочно позвонить в банк"],
        "позвонить в банк",
        "A",
        "-",
        "-",
        "-",
        "-",
    ),
    (&["важно позвонить"], "позвонить", "B", "-", "-", "-", "-"),
    (
        &["приоритет C прочитать статью"],
        "прочитать статью",
        "C",
        "-",
        "-",
        "-",
        "-",
    ),
    // --- Russian: the lead-in verbs ---------------------------------------
    (
        &["напомни позвонить врачу"],
        "позвонить врачу",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    (
        &["напомни мне выкинуть мусор"],
        "выкинуть мусор",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    // The cost the closed list is accepted with: an entry that really starts
    // with one of these verbs loses the word, and is corrected on the screen.
    (
        &["Создай отчёт по проекту"],
        "отчёт по проекту",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    // The same verb away from the start is left where it is.
    (
        &["позвонить и напомни про отчёт"],
        "позвонить и напомни про отчёт",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    // --- Russian: chains ---------------------------------------------------
    (
        &["купить молоко", "завтра"],
        "купить молоко",
        "-",
        "scheduled",
        "2026-09-01",
        "-",
        "-",
    ),
    (
        &["купить молоко завтра", "в пятницу"],
        "купить молоко",
        "-",
        "scheduled",
        "2026-09-04",
        "-",
        "-",
    ),
    (
        &["сдать отчёт завтра", "к пятнице"],
        "сдать отчёт",
        "-",
        "deadline",
        "2026-09-04",
        "-",
        "-",
    ),
    (
        &["созвон завтра в 15:00", "в 16:00"],
        "созвон",
        "-",
        "scheduled",
        "2026-09-01",
        "16:00",
        "-",
    ),
    (
        &["созвон завтра в 15:00", "каждую неделю"],
        "созвон",
        "-",
        "scheduled",
        "2026-09-01",
        "15:00",
        "+1w",
    ),
    (
        &["позвонить врачу", "важно", "срочно"],
        "позвонить врачу",
        "A",
        "-",
        "-",
        "-",
        "-",
    ),
    // Nothing in the grammar removes a field: "не завтра" sets no date and
    // clears none, and the words stay visible in the heading (ADR-0036).
    (
        &["купить молоко завтра", "не завтра"],
        "купить молоко не завтра",
        "-",
        "scheduled",
        "2026-09-01",
        "-",
        "-",
    ),
    // Unrecognised text lands in the heading rather than being dropped.
    (
        &["купить молоко", "в проект работа"],
        "купить молоко в проект работа",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    (
        &["купить молоко", ""],
        "купить молоко",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    // A bare number is a number: without a preposition, an "hours" word or an
    // am/pm qualifier nothing turns it into a time.
    (
        &["купить 5 яблок"],
        "купить 5 яблок",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    (&["buy 5 apples"], "buy 5 apples", "-", "-", "-", "-", "-"),
    // The cost of reading "в N" as an hour, pinned rather than hidden: the
    // number becomes 05:00 and the rest of the phrase stays in the heading,
    // where it is seen and corrected.
    (
        &["встретиться в 5 минутах ходьбы"],
        "встретиться минутах ходьбы",
        "-",
        "-",
        "-",
        "05:00",
        "-",
    ),
    // --- English -----------------------------------------------------------
    (
        &["remind me to call the doctor tomorrow at 3pm"],
        "call the doctor",
        "-",
        "scheduled",
        "2026-09-01",
        "15:00",
        "-",
    ),
    (
        &["review today"],
        "review",
        "-",
        "scheduled",
        "2026-08-31",
        "-",
        "-",
    ),
    (
        &["report due friday"],
        "report",
        "-",
        "deadline",
        "2026-09-04",
        "-",
        "-",
    ),
    (
        &["pay the invoice by september 15"],
        "pay the invoice",
        "-",
        "deadline",
        "2026-09-15",
        "-",
        "-",
    ),
    (
        &["buy milk in 3 days"],
        "buy milk",
        "-",
        "scheduled",
        "2026-09-03",
        "-",
        "-",
    ),
    (
        &["meeting on september 15"],
        "meeting",
        "-",
        "scheduled",
        "2026-09-15",
        "-",
        "-",
    ),
    (
        &["meeting 1 september 2026"],
        "meeting",
        "-",
        "scheduled",
        "2026-09-01",
        "-",
        "-",
    ),
    (
        &["standup every week at 10:00"],
        "standup",
        "-",
        "-",
        "-",
        "10:00",
        "+1w",
    ),
    (&["daily standup"], "standup", "-", "-", "-", "-", "+1d"),
    (
        &["urgent call the bank"],
        "call the bank",
        "A",
        "-",
        "-",
        "-",
        "-",
    ),
    (
        &["call the doctor tomorrow at 15:00", "every week"],
        "call the doctor",
        "-",
        "scheduled",
        "2026-09-01",
        "15:00",
        "+1w",
    ),
];

#[test]
fn every_chain_leaves_the_entry_the_table_states() {
    for (phrases, heading, priority, planning, date, time, repeater) in CHAINS {
        let entry = parse_phrases(phrases.iter().copied(), "ru,en", reference_day());
        let expected = [*heading, *priority, *planning, *date, *time, *repeater];
        let actual = shown(&entry);

        assert_eq!(
            actual.as_slice(),
            expected.map(str::to_string).as_slice(),
            "chain {phrases:?}"
        );
        // A phrase that creates an entry names no keyword and empties no
        // field: the two columns the edit table adds stay untouched, which is
        // what lets a caller tell the two uses of the grammar apart.
        assert_eq!(entry.keyword, None, "chain {phrases:?} named a keyword");
        assert!(
            entry.cleared.is_empty(),
            "chain {phrases:?} emptied {:?}",
            entry.cleared.names()
        );
    }
}

/// The two columns only an edit fills, and the six [`shown`] states, in the
/// order the table writes them: keyword, cleared, priority, planning, date,
/// time, repeater, leftover.
fn shown_edit(entry: &PhraseEntry) -> [String; 8] {
    let dash = || "-".to_string();
    let [heading, priority, planning, date, time, repeater] = shown(entry);
    [
        entry.keyword.map_or_else(dash, |k| k.as_str().to_string()),
        if entry.cleared.is_empty() {
            dash()
        } else {
            entry.cleared.names().join("+")
        },
        priority,
        planning,
        date,
        time,
        repeater,
        if heading.is_empty() { dash() } else { heading },
    ]
}

type Edit = (
    &'static [&'static str],
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

/// phrases, keyword, cleared, priority, planning, date, time, repeater, leftover
const EDITS: &[Edit] = &[
    // --- Russian: the keyword --------------------------------------------
    (
        &["отметь выполненной"],
        "DONE",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    (&["сделано"], "DONE", "-", "-", "-", "-", "-", "-", "-"),
    (&["в работу"], "TODO", "-", "-", "-", "-", "-", "-", "-"),
    (
        &["отменено"],
        "CANCELLED",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    // --- Russian: moving and grading --------------------------------------
    (
        &["перенеси на пятницу"],
        "-",
        "-",
        "-",
        "scheduled",
        "2026-09-04",
        "-",
        "-",
        "-",
    ),
    (&["сделай срочной"], "-", "-", "A", "-", "-", "-", "-", "-"),
    (
        &["смени приоритет C"],
        "-",
        "-",
        "C",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    // --- Russian: emptying a field ----------------------------------------
    (&["убрать дату"], "-", "date", "-", "-", "-", "-", "-", "-"),
    (&["снять срок"], "-", "date", "-", "-", "-", "-", "-", "-"),
    (&["убрать время"], "-", "time", "-", "-", "-", "-", "-", "-"),
    (&["без времени"], "-", "time", "-", "-", "-", "-", "-", "-"),
    (
        &["убрать повтор"],
        "-",
        "repeater",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    (
        &["без приоритета"],
        "-",
        "priority",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    // --- Russian: two instructions in one phrase --------------------------
    (
        &["перенеси на пятницу в 16:00 и сделай срочной"],
        "-",
        "-",
        "A",
        "scheduled",
        "2026-09-04",
        "16:00",
        "-",
        "-",
    ),
    (
        &["отметь выполненной и убрать повтор"],
        "DONE",
        "repeater",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    // Naming a field and emptying it cancel each other, whichever comes last.
    (
        &["убрать дату", "в пятницу"],
        "-",
        "-",
        "-",
        "scheduled",
        "2026-09-04",
        "-",
        "-",
        "-",
    ),
    (
        &["в пятницу", "убрать дату"],
        "-",
        "date",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    // A word no rule knows is left over, which is what a caller refuses the
    // phrase over rather than writing it into the entry.
    (
        &["перенеси на пятницу совсем"],
        "-",
        "-",
        "-",
        "scheduled",
        "2026-09-04",
        "-",
        "-",
        "совсем",
    ),
    // Negation still empties nothing (ADR-0035): both words are leftover.
    (
        &["не завтра"],
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
        "не завтра",
    ),
    // --- English -----------------------------------------------------------
    (&["mark as done"], "DONE", "-", "-", "-", "-", "-", "-", "-"),
    (
        &["mark as cancelled"],
        "CANCELLED",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    (
        &["move to friday"],
        "-",
        "-",
        "-",
        "scheduled",
        "2026-09-04",
        "-",
        "-",
        "-",
    ),
    (
        &["change it to friday at 9am"],
        "-",
        "-",
        "-",
        "scheduled",
        "2026-09-04",
        "09:00",
        "-",
        "-",
    ),
    (&["make it urgent"], "-", "-", "A", "-", "-", "-", "-", "-"),
    (
        &["set the priority B"],
        "-",
        "-",
        "B",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    (&["no date"], "-", "date", "-", "-", "-", "-", "-", "-"),
    (
        &["remove the time"],
        "-",
        "time",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    (
        &["no repeat"],
        "-",
        "repeater",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
    (
        &["clear the priority"],
        "-",
        "priority",
        "-",
        "-",
        "-",
        "-",
        "-",
        "-",
    ),
];

#[test]
fn every_edit_leaves_the_entry_the_table_states() {
    for (phrases, keyword, cleared, priority, planning, date, time, repeater, leftover) in EDITS {
        let entry = parse_phrases(phrases.iter().copied(), "ru,en", reference_day());
        let expected = [
            *keyword, *cleared, *priority, *planning, *date, *time, *repeater, *leftover,
        ];

        assert_eq!(
            shown_edit(&entry).as_slice(),
            expected.map(str::to_string).as_slice(),
            "edit {phrases:?}"
        );
    }
}

#[test]
fn an_edit_is_a_fold_over_single_steps_too() {
    for (phrases, ..) in EDITS {
        let folded = parse_phrases(phrases.iter().copied(), "ru,en", reference_day());
        let stepped = phrases
            .iter()
            .fold(PhraseEntry::default(), |entry, phrase| {
                refine_entry(entry, phrase, "ru,en", reference_day())
            });

        assert_eq!(
            shown_edit(&folded),
            shown_edit(&stepped),
            "edit {phrases:?}"
        );
    }
}

#[test]
fn a_verb_of_creating_after_a_conjunction_stays_in_the_heading() {
    // The conjunction rule lets a second instruction follow ("и сделай
    // срочной"), and it must not reach further than that: "напомни" in the
    // middle of a sentence is part of what was said.
    let entry = parse_phrases(["позвонить и напомни про отчёт"], "ru,en", reference_day());

    assert_eq!(entry.heading, "позвонить и напомни про отчёт");
    assert_eq!(entry.keyword, None);
}

#[test]
fn a_chain_is_a_fold_over_single_steps() {
    // `parse_phrases` is the fold; `refine_entry` is its step. Both must
    // agree, otherwise a client that refines one phrase at a time (which is
    // what the creation screen does) would drift from the table above.
    for (phrases, ..) in CHAINS {
        let folded = parse_phrases(phrases.iter().copied(), "ru,en", reference_day());
        let stepped = phrases
            .iter()
            .fold(PhraseEntry::default(), |entry, phrase| {
                refine_entry(entry, phrase, "ru,en", reference_day())
            });

        assert_eq!(shown(&folded), shown(&stepped), "chain {phrases:?}");
    }
}

#[test]
fn the_empty_entry_is_the_first_step_and_not_a_special_case() {
    let empty = PhraseEntry::default();

    assert_eq!(empty.heading, "");
    assert_eq!(
        &shown(&empty)[1..],
        ["-", "-", "-", "-", "-"].map(str::to_string)
    );
}

#[test]
fn a_locale_that_is_off_leaves_its_phrasings_alone() {
    // `--locale ru` must not parse English dates: the phrasing stays in the
    // heading rather than silently setting a field the person did not name in
    // the language they chose.
    let entry = parse_phrases(["buy milk tomorrow"], "ru", reference_day());

    assert_eq!(entry.heading, "buy milk tomorrow");
    assert_eq!(entry.date, None);
}

#[test]
fn the_parser_reads_the_clock_from_its_argument_only() {
    // Same phrase, two reference days, two answers — the parser has no notion
    // of today of its own (ADR-0035).
    let first = parse_phrases(["позвонить завтра"], "ru,en", reference_day());
    let later = parse_phrases(
        ["позвонить завтра"],
        "ru,en",
        NaiveDate::from_ymd_opt(2027, 1, 31).expect("valid day"),
    );

    assert_eq!(first.date, NaiveDate::from_ymd_opt(2026, 9, 1));
    assert_eq!(later.date, NaiveDate::from_ymd_opt(2027, 2, 1));
}

#[test]
fn the_readme_example_behaves_as_it_says() {
    // The snippet under "Parsing a phrase into an entry" in README.md. Kept
    // here so an edit to the grammar that invalidates the documentation
    // fails a test rather than living on in the README.
    let today = reference_day();

    let entry = parse_phrases(["позвонить врачу завтра в 15:00"], "ru,en", today);
    assert_eq!(entry.heading, "позвонить врачу");

    let entry = refine_entry(entry, "каждую неделю", "ru,en", today);
    assert_eq!(entry.date, NaiveDate::from_ymd_opt(2026, 9, 1));
    assert_eq!(
        entry.repeater.as_ref().map(|r| r.canonical()).as_deref(),
        Some("+1w")
    );

    let edit = parse_phrases(["отметь выполненной и убрать повтор"], "ru,en", today);
    assert_eq!(edit.keyword, Some(PhraseKeyword::Done));
    assert_eq!(edit.cleared.names(), ["repeater"]);
}
