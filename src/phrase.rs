//! Parsing a phrase in natural language into the fields of an entry.
//!
//! The rules live here because both clients already call this crate, and
//! because they extend a grammar that is here already — the timestamp parser
//! and the weekday tables of [`crate::locale`]. See
//! [ADR-0035](https://github.com/VitalyOstanin/markdown-org-extract/blob/master/docs/adr/0035-a-phrase-is-parsed-into-an-entry-by-rules.md)
//! for why rules rather than a model, and
//! [ADR-0036](https://github.com/VitalyOstanin/markdown-org-extract/blob/master/docs/adr/0036-a-later-phrase-refines-the-entry.md)
//! for why a phrase refines what is already parsed instead of replacing it.
//!
//! What the rules recognise, in Russian and in English:
//!
//! | № | Field    | Phrasings                                                                  |
//! |---|----------|----------------------------------------------------------------------------|
//! | 1 | date     | today / tomorrow / the day after, a weekday name, `in N days`, `15 сентября`, `1 september 2026`, `2026-09-15` |
//! | 2 | planning | `к`, `до`, `by`, `due` make it a deadline; anything else scheduled          |
//! | 3 | time     | `в 15:00`, `at 3pm`, `в три часа дня`, `at 10 o'clock`                     |
//! | 4 | repeater | `каждый день`, `каждые 2 недели`, `еженедельно`, `every week`, `daily`     |
//! | 5 | priority | `срочно`, `важно`, `urgent`, `important`, `приоритет B`, `priority B`      |
//! | 6 | keyword  | `выполнено`, `в работу`, `отменено`, `done`, `todo`, `cancelled`           |
//! | 7 | reminder | `за час до`, `напомни за 15 минут`, `за полчаса`, `remind me an hour before` |
//! | 8 | cleared  | `убрать дату`, `без приоритета`, `no repeat`, `remove the time`, `убрать напоминание` |
//! | 9 | heading  | everything the rules did not consume                                       |
//!
//! Two costs are accepted deliberately, both visible on the screen the fields
//! are shown on and correctable there:
//!
//! - a lead-in verb (`напомни`, `создай`, `remind me to`, `add a task`) is
//!   eaten at the start of a phrase, so an entry that really begins with one
//!   loses that word; a verb of reminding is eaten anywhere a lead time
//!   follows it ("позвонить врачу, напомни за час"), and left alone where
//!   none does;
//! - `в N` / `at N` with nothing after the number reads as an hour, so
//!   "в 5 минутах ходьбы" sets 05:00 and leaves "минутах ходьбы" in the
//!   heading;
//! - `за` in front of a span reads as a reminder's lead time, so "отчёт за
//!   неделю" asks to be reminded a week ahead and leaves "отчёт" as the
//!   heading. The mark is required in both languages — "before" behind the
//!   English one — because without it every span in a phrase would be one.
//!
//! The last two fields are what an edit of an existing entry needs: a keyword
//! to move the entry between TODO and DONE, and a field said to be empty. The
//! grammar is one and the same for both uses — whether a phrase creates an
//! entry or edits one is the caller's reading of the fields it gets back.
//!
//! Removal is said outright: "убрать дату" empties the date and names it in
//! [`PhraseEntry::cleared`], which is how a caller tells an emptied field from
//! one the phrase never mentioned. Negation removes nothing: "не завтра" sets
//! no date and clears none — the words land in the heading like any other text
//! the rules do not know.

use chrono::{Datelike, Days, Months, NaiveDate, NaiveTime, Timelike, Weekday};
use serde::ser::{Serialize, SerializeStruct, Serializer};

use crate::reminder::{ReminderLead, ReminderUnit};
use crate::timestamp::{Repeater, RepeaterType, RepeaterUnit};
use crate::types::Priority;

/// Which planning line a parsed date belongs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningKind {
    /// `SCHEDULED:` — the day work on the entry starts.
    Scheduled,
    /// `DEADLINE:` — the day the entry is due.
    Deadline,
}

impl PlanningKind {
    /// The wire spelling of the planning kind: `scheduled` or `deadline`.
    /// Same string in the JSON of `parse-phrase` and in a client that formats
    /// the fields itself.
    pub fn as_str(self) -> &'static str {
        match self {
            PlanningKind::Scheduled => "scheduled",
            PlanningKind::Deadline => "deadline",
        }
    }
}

/// The keyword a phrase names for an entry.
///
/// Not [`crate::types::TaskType`]: the cancelled variant of that type carries
/// the spelling found in the source file (`CANCELLED` / `CANCELED`, ADR-0021),
/// and a phrase says neither — the spelling stays whatever the file already
/// uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhraseKeyword {
    /// Open task (`TODO`).
    Todo,
    /// Completed task (`DONE`).
    Done,
    /// Abandoned task, in whichever spelling the file uses.
    Cancelled,
}

impl PhraseKeyword {
    /// The wire spelling of the keyword. Same string in the JSON of
    /// `parse-phrase` and in a client that applies the fields itself.
    pub fn as_str(self) -> &'static str {
        match self {
            PhraseKeyword::Todo => "TODO",
            PhraseKeyword::Done => "DONE",
            PhraseKeyword::Cancelled => "CANCELLED",
        }
    }
}

/// The fields a phrase asked to empty.
///
/// A field is either named with a value, not named at all, or named as empty,
/// and the first two are already told apart by `Option`. This is the third
/// case: `date: true` means the phrase said "убрать дату", which is not the
/// same as saying nothing about the date.
///
/// Emptying the date empties the planning line with it: a `SCHEDULED:` line
/// without a day is not a line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ClearedFields {
    /// The date, and the planning line it stands on.
    pub date: bool,
    /// The hour and minute.
    pub time: bool,
    /// The repeater.
    pub repeater: bool,
    /// The priority cookie.
    pub priority: bool,
    /// The lead time of the entry's own reminder.
    pub reminder: bool,
}

impl ClearedFields {
    /// The wire names of the emptied fields, in a fixed order, which is what
    /// the JSON of `parse-phrase` prints. Empty when the phrase emptied
    /// nothing.
    pub fn names(self) -> Vec<&'static str> {
        [
            (self.date, "date"),
            (self.time, "time"),
            (self.repeater, "repeater"),
            (self.priority, "priority"),
            (self.reminder, "reminder"),
        ]
        .into_iter()
        .filter_map(|(cleared, name)| cleared.then_some(name))
        .collect()
    }

    /// Whether the phrase emptied nothing at all.
    pub fn is_empty(self) -> bool {
        !(self.date || self.time || self.repeater || self.priority || self.reminder)
    }
}

/// The fields a phrase can fill.
///
/// [`Default`] is the empty entry, which is where a chain of phrases starts;
/// it is not a special case, only the first step of the fold (ADR-0036). The
/// struct is `#[non_exhaustive]` so a later field is an additive change for
/// embedders: build it with [`PhraseEntry::default`] and assign the fields
/// you have rather than with a struct literal.
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct PhraseEntry {
    /// Everything the rules did not consume, in the order it was said.
    pub heading: String,
    /// Priority cookie, when a phrase named one.
    pub priority: Option<Priority>,
    /// Which planning line [`PhraseEntry::date`] belongs on. Set together
    /// with the date and never on its own.
    pub planning: Option<PlanningKind>,
    /// The day the entry is planned for.
    pub date: Option<NaiveDate>,
    /// The hour and minute, when a phrase named one. A time without a date is
    /// possible — the rules fill what was said and guess nothing.
    pub time: Option<NaiveTime>,
    /// The repeater, as the timestamp grammar spells it (`+1w`, `+1wd`).
    pub repeater: Option<Repeater>,
    /// The keyword, when a phrase named one. Only an edit of an entry that
    /// exists has anywhere to put it.
    pub keyword: Option<PhraseKeyword>,
    /// How far ahead of the entry a reminder is wanted, when a phrase said
    /// so ("за час до", "an hour before"). Written to the entry as the
    /// `REMINDER` property of ADR-0041; when the phrase said nothing, the
    /// reminding client's own setting stands.
    pub reminder: Option<ReminderLead>,
    /// The fields a phrase said to empty, which is not the same as the fields
    /// it left unnamed.
    pub cleared: ClearedFields,
}

/// Refine `entry` with one more `phrase`.
///
/// A field the phrase names replaces what was there; a field it does not name
/// keeps its value; a field it says to empty is emptied and listed in
/// [`PhraseEntry::cleared`]; text the rules do not consume is appended to the
/// heading, separated by a space. On the first phrase, where the heading is
/// empty, that is exactly "what is left over becomes the heading".
///
/// The two ways of naming a field cancel each other, so a chain says what its
/// last phrase says: "убрать дату" then "в пятницу" leaves the date set and
/// nothing cleared.
///
/// `locale` is the comma-separated `--locale` value (`"ru"`, `"en"`,
/// `"ru,en"`): only the grammars it names are consulted, so a phrase in a
/// language that is switched off stays in the heading. A value naming neither
/// language enables both, which is what the `--locale` default does.
///
/// `today` is the day the phrase is relative to. The parser never reads the
/// clock: "tomorrow" is meaningless without saying tomorrow from when, and
/// deciding what today is belongs to the caller (ADR-0009, ADR-0035).
pub fn refine_entry(
    mut entry: PhraseEntry,
    phrase: &str,
    locale: &str,
    today: NaiveDate,
) -> PhraseEntry {
    let langs = Languages::from_locale(locale);
    let tokens = tokenize(phrase);
    let mut leftover: Vec<&str> = Vec::new();
    let mut i = lead_in_len(&tokens, langs);

    while i < tokens.len() {
        // "перенеси на пятницу и сделай срочной": a conjunction joins two
        // instructions, so a verb of editing may start again after it, and the
        // conjunction itself is not part of the heading. A conjunction that
        // joins plain words ("хлеб и молоко") is left where it is, and so is
        // one that stands in front of a verb of creating: "позвонить и напомни
        // про отчёт" is one entry, not an entry and an instruction.
        if is_conjunction(&tokens[i].key, langs) {
            let lead = edit_in_len(&tokens[i + 1..], langs);
            if lead > 0 {
                i += 1 + lead;
                continue;
            }
            if match_rule(&tokens, i + 1, langs, today).is_some() {
                i += 1;
                continue;
            }
        }
        // "не завтра" / "not tomorrow" sets nothing and clears nothing: both
        // words go to the heading, and so does the phrasing they negate.
        if is_negation(&tokens[i].key, langs) {
            if let Some((consumed, _)) = match_rule(&tokens, i + 1, langs, today) {
                leftover.extend(tokens[i..i + 1 + consumed].iter().map(|t| t.raw));
                i += 1 + consumed;
                continue;
            }
        }
        if let Some((consumed, effect)) = match_rule(&tokens, i, langs, today) {
            apply(&mut entry, effect);
            i += consumed;
            continue;
        }
        leftover.push(tokens[i].raw);
        i += 1;
    }

    append_heading(&mut entry.heading, &leftover);
    entry
}

/// Fold [`refine_entry`] over a chain of phrases, starting from the empty
/// entry. This is what a caller with the whole sentence in hand uses; a screen
/// that refines one phrase at a time calls [`refine_entry`] and keeps the
/// entry between calls.
pub fn parse_phrases<'a>(
    phrases: impl IntoIterator<Item = &'a str>,
    locale: &str,
    today: NaiveDate,
) -> PhraseEntry {
    phrases
        .into_iter()
        .fold(PhraseEntry::default(), |entry, phrase| {
            refine_entry(entry, phrase, locale, today)
        })
}

/// Which grammars are consulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Languages {
    ru: bool,
    en: bool,
}

impl Languages {
    fn from_locale(locale: &str) -> Self {
        let mut langs = Languages {
            ru: false,
            en: false,
        };
        for segment in locale.split(',') {
            match segment.trim() {
                "ru" => langs.ru = true,
                "en" => langs.en = true,
                _ => {}
            }
        }
        if !langs.ru && !langs.en {
            // A caller that names no language gets the `--locale` default
            // rather than a parser that recognises nothing.
            langs.ru = true;
            langs.en = true;
        }
        langs
    }
}

/// One word of the phrase: what was typed, and what the rules match against.
#[derive(Debug)]
struct Token<'a> {
    /// The word as it was written; this is what reaches the heading.
    raw: &'a str,
    /// Lowercased, stripped of surrounding punctuation, with `ё` folded to
    /// `е` so a dictated "ещё" and a typed "еще" match the same rule.
    key: String,
}

fn tokenize(phrase: &str) -> Vec<Token<'_>> {
    phrase
        .split_whitespace()
        .map(|raw| Token {
            raw,
            key: key_of(raw),
        })
        .collect()
}

fn key_of(raw: &str) -> String {
    raw.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
        .replace('ё', "е")
}

/// What a matched rule does to the entry.
#[derive(Debug)]
enum Effect {
    Date {
        date: NaiveDate,
        planning: PlanningKind,
    },
    Time(NaiveTime),
    Repeat(Repeater),
    Prio(Priority),
    Keyword(PhraseKeyword),
    Lead(ReminderLead),
    Clear(Field),
}

/// A field a phrase can say to empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Date,
    Time,
    Repeater,
    Priority,
    Reminder,
}

/// Filling a field and emptying it are the same rule read twice, so each of
/// them undoes the other: a value clears the "emptied" mark, and emptying
/// drops the value.
fn apply(entry: &mut PhraseEntry, effect: Effect) {
    match effect {
        Effect::Date { date, planning } => {
            entry.date = Some(date);
            entry.planning = Some(planning);
            entry.cleared.date = false;
        }
        Effect::Time(time) => {
            entry.time = Some(time);
            entry.cleared.time = false;
        }
        Effect::Repeat(repeater) => {
            entry.repeater = Some(repeater);
            entry.cleared.repeater = false;
        }
        Effect::Prio(priority) => {
            entry.priority = Some(priority);
            entry.cleared.priority = false;
        }
        Effect::Keyword(keyword) => entry.keyword = Some(keyword),
        Effect::Lead(lead) => {
            entry.reminder = Some(lead);
            entry.cleared.reminder = false;
        }
        Effect::Clear(Field::Date) => {
            entry.date = None;
            entry.planning = None;
            entry.cleared.date = true;
        }
        Effect::Clear(Field::Time) => {
            entry.time = None;
            entry.cleared.time = true;
        }
        Effect::Clear(Field::Repeater) => {
            entry.repeater = None;
            entry.cleared.repeater = true;
        }
        Effect::Clear(Field::Priority) => {
            entry.priority = None;
            entry.cleared.priority = true;
        }
        Effect::Clear(Field::Reminder) => {
            entry.reminder = None;
            entry.cleared.reminder = true;
        }
    }
}

/// Try every rule at `i`, returning how many tokens it ate and what it did.
fn match_rule(
    tokens: &[Token<'_>],
    i: usize,
    langs: Languages,
    today: NaiveDate,
) -> Option<(usize, Effect)> {
    if i >= tokens.len() {
        return None;
    }
    // Emptying and the keyword go first: both are headed by a word the other
    // rules would take apart ("снять срок" starts with a deadline preposition,
    // "в работу" with a scheduling one).
    if let Some((consumed, field)) = match_clear(tokens, i, langs) {
        return Some((consumed, Effect::Clear(field)));
    }
    if let Some((consumed, keyword)) = match_keyword(tokens, i, langs) {
        return Some((consumed, Effect::Keyword(keyword)));
    }
    // Before the date: "за 2 дня" is a lead time and "через 2 дня" a day,
    // and the two are one word apart.
    if let Some((consumed, lead)) = match_lead_time(tokens, i, langs) {
        return Some((consumed, Effect::Lead(lead)));
    }
    if let Some((consumed, date, planning)) = match_planned_date(tokens, i, langs, today) {
        return Some((consumed, Effect::Date { date, planning }));
    }
    if let Some((consumed, time)) = match_time(tokens, i, langs) {
        return Some((consumed, Effect::Time(time)));
    }
    if let Some((consumed, repeater)) = match_repeater(tokens, i, langs) {
        return Some((consumed, Effect::Repeat(repeater)));
    }
    if let Some((consumed, priority)) = match_priority(tokens, i, langs) {
        return Some((consumed, Effect::Prio(priority)));
    }
    None
}

const RU_NEGATIONS: &[&str] = &["не"];

const EN_NEGATIONS: &[&str] = &["not"];

const RU_CONJUNCTIONS: &[&str] = &["и", "а"];

const EN_CONJUNCTIONS: &[&str] = &["and"];

fn is_negation(key: &str, langs: Languages) -> bool {
    listed(langs, RU_NEGATIONS, EN_NEGATIONS, key)
}

fn is_conjunction(key: &str, langs: Languages) -> bool {
    listed(langs, RU_CONJUNCTIONS, EN_CONJUNCTIONS, key)
}

// --- emptying a field ------------------------------------------------------

const RU_REMOVES: &[&str] = &[
    "убрать",
    "убери",
    "снять",
    "сними",
    "удалить",
    "удали",
    "очистить",
    "очисти",
];

const EN_REMOVES: &[&str] = &["remove", "clear", "drop", "delete", "unset"];

const RU_WITHOUTS: &[&str] = &["без"];

const EN_WITHOUTS: &[&str] = &["no", "without"];

/// "убрать дату", "без приоритета", "remove the time", "no repeat".
///
/// Two shapes, and both name the field outright: a verb of removal, or a
/// preposition of absence. Neither is a negation — "не завтра" still says
/// nothing about the date (ADR-0035).
fn match_clear(tokens: &[Token<'_>], i: usize, langs: Languages) -> Option<(usize, Field)> {
    let head = tokens.get(i)?.key.as_str();
    let removes = listed(langs, RU_REMOVES, EN_REMOVES, head);
    let says_without = listed(langs, RU_WITHOUTS, EN_WITHOUTS, head);
    if !removes && !says_without {
        return None;
    }

    let mut j = i + 1;
    if langs.en && removes && word_at(tokens, j) == "the" {
        j += 1;
    }
    let field = field_noun(&tokens.get(j)?.key, langs)?;
    Some((j + 1 - i, field))
}

const RU_FIELD_NOUNS: &[(&str, Field)] = &[
    ("дату", Field::Date),
    ("дата", Field::Date),
    ("даты", Field::Date),
    ("срок", Field::Date),
    ("срока", Field::Date),
    ("сроки", Field::Date),
    ("дедлайн", Field::Date),
    ("дедлайна", Field::Date),
    ("время", Field::Time),
    ("времени", Field::Time),
    ("час", Field::Time),
    ("часа", Field::Time),
    ("повтор", Field::Repeater),
    ("повтора", Field::Repeater),
    ("повторы", Field::Repeater),
    ("повторение", Field::Repeater),
    ("повторения", Field::Repeater),
    ("приоритет", Field::Priority),
    ("приоритета", Field::Priority),
    ("напоминание", Field::Reminder),
    ("напоминания", Field::Reminder),
    ("напоминаний", Field::Reminder),
];

const EN_FIELD_NOUNS: &[(&str, Field)] = &[
    ("date", Field::Date),
    ("deadline", Field::Date),
    ("due", Field::Date),
    ("schedule", Field::Date),
    ("scheduled", Field::Date),
    ("time", Field::Time),
    ("hour", Field::Time),
    ("repeat", Field::Repeater),
    ("repeater", Field::Repeater),
    ("repetition", Field::Repeater),
    ("recurrence", Field::Repeater),
    ("priority", Field::Priority),
    ("reminder", Field::Reminder),
    ("reminders", Field::Reminder),
];

/// The names of the fields as they are said when one is emptied.
fn field_noun(key: &str, langs: Languages) -> Option<Field> {
    in_tables(langs, RU_FIELD_NOUNS, EN_FIELD_NOUNS, key)
}

// --- keyword ---------------------------------------------------------------

/// The Russian words that say an entry is done or cancelled, in the genders
/// and cases they are said in.
const RU_KEYWORDS: &[(&str, PhraseKeyword)] = &[
    ("выполнено", PhraseKeyword::Done),
    ("выполнена", PhraseKeyword::Done),
    ("выполнен", PhraseKeyword::Done),
    ("выполненной", PhraseKeyword::Done),
    ("выполненную", PhraseKeyword::Done),
    ("сделано", PhraseKeyword::Done),
    ("сделана", PhraseKeyword::Done),
    ("готово", PhraseKeyword::Done),
    ("завершено", PhraseKeyword::Done),
    ("завершена", PhraseKeyword::Done),
    ("отменено", PhraseKeyword::Cancelled),
    ("отменена", PhraseKeyword::Cancelled),
    ("отменен", PhraseKeyword::Cancelled),
    ("отмененной", PhraseKeyword::Cancelled),
    ("отмененную", PhraseKeyword::Cancelled),
];

/// The English words that say the same, plus the one that reopens an entry.
const EN_KEYWORDS: &[(&str, PhraseKeyword)] = &[
    ("done", PhraseKeyword::Done),
    ("completed", PhraseKeyword::Done),
    ("todo", PhraseKeyword::Todo),
    ("cancelled", PhraseKeyword::Cancelled),
    ("canceled", PhraseKeyword::Cancelled),
];

/// The nouns that follow "в" to say the entry is open again: "в работу",
/// "в работе".
const RU_BACK_TO_WORK: &[&str] = &["работу", "работе"];

/// "отметь выполненной", "в работу", "mark as done".
///
/// The forms are the ones said about an entry that exists, in the genders and
/// cases they are said in; the imperative in front of them is a lead-in verb
/// and is eaten before the rules run.
fn match_keyword(
    tokens: &[Token<'_>],
    i: usize,
    langs: Languages,
) -> Option<(usize, PhraseKeyword)> {
    let key = tokens.get(i)?.key.as_str();

    // "в работу" / "в работе" — the entry goes back to being open.
    if langs.ru && key == "в" && RU_BACK_TO_WORK.contains(&word_at(tokens, i + 1)) {
        return Some((2, PhraseKeyword::Todo));
    }
    in_tables(langs, RU_KEYWORDS, EN_KEYWORDS, key).map(|keyword| (1, keyword))
}

// --- lead-in verbs ---------------------------------------------------------

/// The closed list of ways a person addresses the application before saying
/// what a new entry is. Longest first: the first sequence that matches wins.
const RU_LEAD_INS: &[&[&str]] = &[
    &["напомни", "мне"],
    &["напомни"],
    &["добавь", "задачу"],
    &["добавь"],
    &["создай", "задачу"],
    &["создай"],
    &["поставь"],
    &["запиши"],
];

const EN_LEAD_INS: &[&[&str]] = &[
    &["remind", "me", "to"],
    &["remind", "me"],
    &["remind"],
    &["add", "a", "task", "to"],
    &["add", "a", "task"],
    &["add"],
    &["create", "a", "task", "to"],
    &["create", "a", "task"],
    &["create"],
    &["write", "down"],
];

/// The same list for changing an entry that exists. Kept apart from the verbs
/// of creating because a conjunction may start one of these again in the
/// middle of a phrase ("перенеси на пятницу и сделай срочной") while a verb of
/// creating stays where it stands ("позвонить и напомни про отчёт").
const RU_EDIT_INS: &[&[&str]] = &[
    &["перенеси"],
    &["перенести"],
    &["сделай"],
    &["сделать"],
    &["отметь"],
    &["отметить"],
    &["смени"],
    &["поменяй"],
    &["измени"],
    &["установи"],
];

/// `to` belongs to the verb here rather than to the date: on its own it says
/// nothing about which planning line a date goes on, and taking it for a
/// preposition would read "call to discuss" as a date.
const EN_EDIT_INS: &[&[&str]] = &[
    &["move", "it", "to"],
    &["move", "it"],
    &["move", "to"],
    &["move"],
    &["reschedule", "to"],
    &["reschedule"],
    &["mark", "it", "as"],
    &["mark", "as"],
    &["mark"],
    &["make", "it"],
    &["make"],
    &["change", "it", "to"],
    &["change", "the"],
    &["change"],
    &["set", "the"],
    &["set"],
];

/// How many tokens the lead-in takes at the start of a phrase, where either
/// kind of verb may stand. Only ever matched at the start: the same verb
/// further along is part of what was said.
fn lead_in_len(tokens: &[Token<'_>], langs: Languages) -> usize {
    let creating = first_match(tokens, langs, RU_LEAD_INS, EN_LEAD_INS);
    if creating > 0 {
        return creating;
    }
    edit_in_len(tokens, langs)
}

/// How many tokens a verb of editing takes. This is what may follow a
/// conjunction as well as start a phrase.
fn edit_in_len(tokens: &[Token<'_>], langs: Languages) -> usize {
    first_match(tokens, langs, RU_EDIT_INS, EN_EDIT_INS)
}

fn first_match(tokens: &[Token<'_>], langs: Languages, ru: &[&[&str]], en: &[&[&str]]) -> usize {
    let mut lists: Vec<&&[&str]> = Vec::new();
    if langs.ru {
        lists.extend(ru.iter());
    }
    if langs.en {
        lists.extend(en.iter());
    }
    for words in lists {
        if words.len() <= tokens.len()
            && words
                .iter()
                .zip(tokens.iter())
                .all(|(word, token)| *word == token.key)
        {
            return words.len();
        }
    }
    0
}

// --- dates -----------------------------------------------------------------

/// A date with the preposition that says which planning line it belongs on.
fn match_planned_date(
    tokens: &[Token<'_>],
    i: usize,
    langs: Languages,
    today: NaiveDate,
) -> Option<(usize, NaiveDate, PlanningKind)> {
    if let Some(kind) = date_prefix(&tokens.get(i)?.key, langs) {
        if let Some((consumed, date)) = match_date(tokens, i + 1, langs, today) {
            return Some((consumed + 1, date, kind));
        }
    }
    let (consumed, date) = match_date(tokens, i, langs, today)?;
    Some((consumed, date, PlanningKind::Scheduled))
}

const RU_DATE_PREFIXES: &[(&str, PlanningKind)] = &[
    ("в", PlanningKind::Scheduled),
    ("во", PlanningKind::Scheduled),
    ("на", PlanningKind::Scheduled),
    ("к", PlanningKind::Deadline),
    ("ко", PlanningKind::Deadline),
    ("до", PlanningKind::Deadline),
    ("срок", PlanningKind::Deadline),
    ("дедлайн", PlanningKind::Deadline),
];

const EN_DATE_PREFIXES: &[(&str, PlanningKind)] = &[
    ("on", PlanningKind::Scheduled),
    ("at", PlanningKind::Scheduled),
    ("by", PlanningKind::Deadline),
    ("due", PlanningKind::Deadline),
    ("before", PlanningKind::Deadline),
    ("until", PlanningKind::Deadline),
    ("deadline", PlanningKind::Deadline),
];

/// A preposition in front of a date, and what it says about the date.
fn date_prefix(key: &str, langs: Languages) -> Option<PlanningKind> {
    in_tables(langs, RU_DATE_PREFIXES, EN_DATE_PREFIXES, key)
}

/// The days said by name, as how far they stand from the reference day.
const RU_NAMED_DAYS: &[(&str, u64)] = &[("сегодня", 0), ("завтра", 1), ("послезавтра", 2)];

const EN_NAMED_DAYS: &[(&str, u64)] = &[("today", 0), ("tomorrow", 1)];

fn match_date(
    tokens: &[Token<'_>],
    i: usize,
    langs: Languages,
    today: NaiveDate,
) -> Option<(usize, NaiveDate)> {
    let key = tokens.get(i)?.key.as_str();

    if let Some(days) = in_tables(langs, RU_NAMED_DAYS, EN_NAMED_DAYS, key) {
        return Some((1, today.checked_add_days(Days::new(days))?));
    }
    if langs.en
        && key == "day"
        && word_at(tokens, i + 1) == "after"
        && word_at(tokens, i + 2) == "tomorrow"
    {
        return Some((3, today.checked_add_days(Days::new(2))?));
    }
    if let Ok(date) = NaiveDate::parse_from_str(key, "%Y-%m-%d") {
        return Some((1, date));
    }
    if let Some(weekday) = weekday_of(key, langs) {
        return Some((1, nearest_weekday(today, weekday)));
    }
    if let Some(found) = match_relative_date(tokens, i, langs, today) {
        return Some(found);
    }
    match_calendar_date(tokens, i, langs, today)
}

fn word_at<'a>(tokens: &'a [Token<'_>], i: usize) -> &'a str {
    tokens.get(i).map_or("", |t| t.key.as_str())
}

/// The nearest `weekday` on or after `from`.
///
/// This is upstream's rule for a bare weekday name in `org-read-date`: "pick
/// that day in the week on or after the derived date" (org.el,
/// `org-read-date-analyze`). Saying "во вторник" on a Tuesday therefore means
/// today, not a week from today — the relative form `+tue`, which upstream
/// pushes a week forward, has no spelling in this grammar.
fn nearest_weekday(from: NaiveDate, weekday: Weekday) -> NaiveDate {
    let delta = (weekday.num_days_from_monday() + 7 - from.weekday().num_days_from_monday()) % 7;
    from.checked_add_days(Days::new(u64::from(delta)))
        .unwrap_or(from)
}

/// A span of time counted from the reference day: "через 3 дня", "in a week".
fn match_relative_date(
    tokens: &[Token<'_>],
    i: usize,
    langs: Languages,
    today: NaiveDate,
) -> Option<(usize, NaiveDate)> {
    let head = tokens.get(i)?.key.as_str();
    if !((langs.ru && head == "через") || (langs.en && head == "in")) {
        return None;
    }
    let mut j = i + 1;
    let mut count: u32 = 1;
    if let Some(token) = tokens.get(j) {
        if let Ok(parsed) = token.key.parse::<u32>() {
            count = parsed;
            j += 1;
        } else if langs.en && matches!(token.key.as_str(), "a" | "an") {
            j += 1;
        } else if let Some(parsed) = langs.ru.then(|| ru_numeral(&token.key)).flatten() {
            count = parsed;
            j += 1;
        }
    }
    let span = span_of(&tokens.get(j)?.key, langs)?;
    let date = shift(today, count, span)?;
    Some((j + 1 - i, date))
}

// --- lead time of a reminder ----------------------------------------------

/// The word a Russian lead time is said with: "за час до звонка".
const RU_LEAD_HEADS: &[&str] = &["за"];

/// The word an English lead time ends with: "an hour before the call". It is
/// required rather than optional, because a count and a unit on their own
/// ("an hour") say nothing about a reminder.
const EN_LEAD_TAILS: &[&str] = &["before"];

/// The Russian word that is a count and a unit at once.
const RU_HALF_HOUR: &str = "полчаса";

/// The verbs a lead time is asked with, and the pronoun they take. A verb of
/// reminding is eaten as a lead-in only at the head of a phrase; said where a
/// person naturally says it -- after what the entry is, "позвонить врачу,
/// напомни за час" -- it introduces the lead time behind it instead.
const RU_REMIND_VERBS: &[&str] = &[
    "напомни",
    "напомнить",
    "напомните",
    "напоминай",
    "напоминать",
];

const EN_REMIND_VERBS: &[&str] = &["remind"];

/// The pronoun the verb takes: "напомни мне за час", "remind me an hour
/// before".
const RU_REMIND_OBJECT: &str = "мне";

const EN_REMIND_OBJECT: &str = "me";

/// The units a lead time is counted in, in the cases they are said in.
const RU_LEAD_UNITS: &[(&str, ReminderUnit)] = &[
    ("минуту", ReminderUnit::Minute),
    ("минуты", ReminderUnit::Minute),
    ("минут", ReminderUnit::Minute),
    ("мин", ReminderUnit::Minute),
    ("час", ReminderUnit::Hour),
    ("часа", ReminderUnit::Hour),
    ("часов", ReminderUnit::Hour),
    ("день", ReminderUnit::Day),
    ("дня", ReminderUnit::Day),
    ("дней", ReminderUnit::Day),
    ("сутки", ReminderUnit::Day),
    ("суток", ReminderUnit::Day),
    ("неделю", ReminderUnit::Week),
    ("недели", ReminderUnit::Week),
    ("недель", ReminderUnit::Week),
    ("месяц", ReminderUnit::Month),
    ("месяца", ReminderUnit::Month),
    ("месяцев", ReminderUnit::Month),
    ("год", ReminderUnit::Year),
    ("года", ReminderUnit::Year),
    ("лет", ReminderUnit::Year),
];

const EN_LEAD_UNITS: &[(&str, ReminderUnit)] = &[
    ("minute", ReminderUnit::Minute),
    ("minutes", ReminderUnit::Minute),
    ("min", ReminderUnit::Minute),
    ("mins", ReminderUnit::Minute),
    ("hour", ReminderUnit::Hour),
    ("hours", ReminderUnit::Hour),
    ("day", ReminderUnit::Day),
    ("days", ReminderUnit::Day),
    ("week", ReminderUnit::Week),
    ("weeks", ReminderUnit::Week),
    ("month", ReminderUnit::Month),
    ("months", ReminderUnit::Month),
    ("year", ReminderUnit::Year),
    ("years", ReminderUnit::Year),
];

/// How long before an occurrence a reminder is wanted: "за час до созвона",
/// "15 minutes before the call" (ADR-0041).
///
/// Each language marks it with a word of its own, and the mark is required:
/// "за" in front, "before" behind. Without it a count and a unit are a span
/// like any other, and "через 2 дня" — a day, not a lead time — is one word
/// away from it.
fn match_lead_time(
    tokens: &[Token<'_>],
    i: usize,
    langs: Languages,
) -> Option<(usize, ReminderLead)> {
    if langs.ru {
        if let Some(found) = asked_for(tokens, i, RU_REMIND_VERBS, RU_REMIND_OBJECT, |at| {
            match_russian_lead_time(tokens, at)
        }) {
            return Some(found);
        }
    }
    if langs.en {
        if let Some(found) = asked_for(tokens, i, EN_REMIND_VERBS, EN_REMIND_OBJECT, |at| {
            match_english_lead_time(tokens, at)
        }) {
            return Some(found);
        }
    }
    None
}

/// The lead time at `i`, with the verb that asks for it where one stands in
/// front: "напомни за час", "remind me an hour before".
///
/// The verb is eaten only together with a lead time that follows it. On its
/// own it is what the entry is called -- "напомни про отчёт" is a task, and a
/// rule that ate the verb alone would leave it named "про отчёт".
fn asked_for(
    tokens: &[Token<'_>],
    i: usize,
    verbs: &[&str],
    object: &str,
    lead_at: impl Fn(usize) -> Option<(usize, ReminderLead)>,
) -> Option<(usize, ReminderLead)> {
    if let Some((consumed, lead)) = lead_at(i) {
        return Some((consumed, lead));
    }
    if !verbs.contains(&word_at(tokens, i)) {
        return None;
    }
    let mut j = i + 1;
    if word_at(tokens, j) == object {
        j += 1;
    }
    let (consumed, lead) = lead_at(j)?;
    Some((j + consumed - i, lead))
}

/// "за час", "за 15 минут", "за два дня", "за полчаса до созвона".
///
/// A "до" right behind the unit belongs to the phrasing rather than to the
/// heading: what follows it is the entry the reminder is about, which the
/// entry already is.
fn match_russian_lead_time(tokens: &[Token<'_>], i: usize) -> Option<(usize, ReminderLead)> {
    if !RU_LEAD_HEADS.contains(&tokens.get(i)?.key.as_str()) {
        return None;
    }
    let mut j = i + 1;
    let lead = if tokens.get(j)?.key == RU_HALF_HOUR {
        j += 1;
        ReminderLead {
            value: 30,
            unit: ReminderUnit::Minute,
        }
    } else {
        let mut value = 1;
        if let Some(token) = tokens.get(j) {
            if let Ok(count) = token.key.parse::<u32>() {
                value = count;
                j += 1;
            } else if let Some(count) = ru_numeral(&token.key) {
                value = count;
                j += 1;
            }
        }
        let unit = lookup(RU_LEAD_UNITS, &tokens.get(j)?.key)?;
        j += 1;
        ReminderLead { value, unit }
    };
    if word_at(tokens, j) == "до" {
        j += 1;
    }
    Some((j - i, lead))
}

/// "an hour before", "15 minutes before", "half an hour before".
fn match_english_lead_time(tokens: &[Token<'_>], i: usize) -> Option<(usize, ReminderLead)> {
    let mut j = i;
    let head = tokens.get(j)?.key.as_str();
    let lead = if head == "half" {
        j += 1;
        if matches!(word_at(tokens, j), "a" | "an") {
            j += 1;
        }
        if lookup(EN_LEAD_UNITS, word_at(tokens, j))? != ReminderUnit::Hour {
            return None;
        }
        j += 1;
        ReminderLead {
            value: 30,
            unit: ReminderUnit::Minute,
        }
    } else {
        let mut value = 1;
        if let Ok(count) = head.parse::<u32>() {
            value = count;
            j += 1;
        } else if matches!(head, "a" | "an") {
            j += 1;
        }
        let unit = lookup(EN_LEAD_UNITS, word_at(tokens, j))?;
        j += 1;
        ReminderLead { value, unit }
    };
    if !EN_LEAD_TAILS.contains(&word_at(tokens, j)) {
        return None;
    }
    Some((j + 1 - i, lead))
}

/// A calendar date: `15 сентября`, `15 september 2026`, `september 15`.
fn match_calendar_date(
    tokens: &[Token<'_>],
    i: usize,
    langs: Languages,
    today: NaiveDate,
) -> Option<(usize, NaiveDate)> {
    if let Ok(day) = tokens.get(i)?.key.parse::<u32>() {
        if !(1..=31).contains(&day) {
            return None;
        }
        let (month, _) = month_of(&tokens.get(i + 1)?.key, langs)?;
        let (extra, year) = year_after(tokens, i + 2);
        return Some((2 + extra, resolve_date(year, month, day, today)?));
    }
    // "september 15" reads as a date only in English: the Russian month names
    // are genitive and never lead ("сентября 15" is not something said).
    let (month, lang) = month_of(&tokens.get(i)?.key, langs)?;
    if lang != Lang::En {
        return None;
    }
    let day = tokens.get(i + 1)?.key.parse::<u32>().ok()?;
    if !(1..=31).contains(&day) {
        return None;
    }
    let (extra, year) = year_after(tokens, i + 2);
    Some((2 + extra, resolve_date(year, month, day, today)?))
}

/// A four-digit year right after a date, if there is one.
fn year_after(tokens: &[Token<'_>], i: usize) -> (usize, Option<i32>) {
    match tokens.get(i).and_then(|t| t.key.parse::<i32>().ok()) {
        Some(year) if (1900..=2100).contains(&year) => (1, Some(year)),
        _ => (0, None),
    }
}

/// Build the date. Without a year, take the nearest occurrence on or after
/// the reference day — the same "on or after" the weekday rule uses. A day
/// that only exists in leap years walks forward until it does.
fn resolve_date(year: Option<i32>, month: u32, day: u32, today: NaiveDate) -> Option<NaiveDate> {
    if let Some(year) = year {
        return NaiveDate::from_ymd_opt(year, month, day);
    }
    (today.year()..=today.year() + 8)
        .filter_map(|year| NaiveDate::from_ymd_opt(year, month, day))
        .find(|date| *date >= today)
}

// --- time ------------------------------------------------------------------

/// A preposition in front of a clock reading: "в 15:00", "at 15:00".
const RU_TIME_PREFIXES: &[&str] = &["в", "во"];

const EN_TIME_PREFIXES: &[&str] = &["at"];

/// The word for "hours" that turns a bare number into a time.
const RU_HOUR_UNITS: &[&str] = &["час", "часа", "часов"];

const EN_HOUR_UNITS: &[&str] = &["oclock", "o'clock"];

/// The half of the day a reading belongs to: `true` is the afternoon.
const RU_HALF_DAYS: &[(&str, bool)] = &[
    ("дня", true),
    ("вечера", true),
    ("утра", false),
    ("ночи", false),
];

const EN_HALF_DAYS: &[(&str, bool)] = &[("pm", true), ("am", false)];

fn match_time(tokens: &[Token<'_>], i: usize, langs: Languages) -> Option<(usize, NaiveTime)> {
    let mut j = i;
    let mut prefixed = false;
    if let Some(token) = tokens.get(j) {
        if listed(langs, RU_TIME_PREFIXES, EN_TIME_PREFIXES, &token.key) {
            j += 1;
            prefixed = true;
        }
    }
    let token = tokens.get(j)?;
    if let Some(time) = parse_hhmm(&token.key) {
        return Some((j + 1 - i, time));
    }
    if let Some(time) = langs.en.then(|| parse_am_pm(&token.key)).flatten() {
        return Some((j + 1 - i, time));
    }

    let mut hour = match token.key.parse::<u32>() {
        Ok(hour) => hour,
        Err(_) => langs.ru.then(|| ru_hour_word(&token.key)).flatten()?,
    };
    if hour > 23 {
        return None;
    }
    let mut k = j + 1;
    let mut named_unit = false;
    if let Some(next) = tokens.get(k) {
        if listed(langs, RU_HOUR_UNITS, EN_HOUR_UNITS, &next.key) {
            named_unit = true;
            k += 1;
        }
    }
    let mut qualified = false;
    if let Some(next) = tokens.get(k) {
        if let Some(afternoon) = in_tables(langs, RU_HALF_DAYS, EN_HALF_DAYS, &next.key) {
            hour = shift_half_day(hour, afternoon);
            qualified = true;
            k += 1;
        }
    }
    // A bare number is a number: only a preposition, an "hours" word or an
    // am/pm qualifier turns it into a time.
    if !prefixed && !named_unit && !qualified {
        return None;
    }
    Some((k - i, NaiveTime::from_hms_opt(hour, 0, 0)?))
}

/// `15:00`, `15:30` — the only written form that needs no preposition.
fn parse_hhmm(key: &str) -> Option<NaiveTime> {
    let (hour, minute) = key.split_once(':')?;
    if hour.is_empty() || hour.len() > 2 || minute.len() != 2 {
        return None;
    }
    NaiveTime::from_hms_opt(hour.parse().ok()?, minute.parse().ok()?, 0)
}

/// `3pm`, `11am`, `3:30pm`.
fn parse_am_pm(key: &str) -> Option<NaiveTime> {
    let (body, afternoon) = match (key.strip_suffix("pm"), key.strip_suffix("am")) {
        (Some(body), _) => (body, true),
        (_, Some(body)) => (body, false),
        _ => return None,
    };
    let body = body.trim();
    if let Some(time) = parse_hhmm(body) {
        return NaiveTime::from_hms_opt(shift_half_day(time.hour(), afternoon), time.minute(), 0);
    }
    let hour: u32 = body.parse().ok()?;
    if hour > 12 {
        return None;
    }
    NaiveTime::from_hms_opt(shift_half_day(hour, afternoon), 0, 0)
}

/// "three in the afternoon" is 15:00; "twelve at night" is 00:00.
fn shift_half_day(hour: u32, afternoon: bool) -> u32 {
    match (afternoon, hour) {
        (true, 0..=11) => hour + 12,
        (false, 12) => 0,
        _ => hour,
    }
}

// --- repeaters -------------------------------------------------------------

/// The words that head a repeat: "каждый вторник", "every week".
const RU_REPEAT_HEADS: &[&str] = &["каждый", "каждая", "каждую", "каждое", "каждые", "каждых"];

const EN_REPEAT_HEADS: &[&str] = &["every"];

/// "каждый рабочий день" / "every working day".
const RU_WORKING_WORDS: &[&str] = &["рабочий", "рабочих", "рабочие"];

const EN_WORKING_WORDS: &[&str] = &["working", "work"];

/// The same said as one noun, which English has and Russian does not.
const EN_WORKDAY_NOUNS: &[&str] = &["workday", "workdays"];

fn match_repeater(tokens: &[Token<'_>], i: usize, langs: Languages) -> Option<(usize, Repeater)> {
    let key = tokens.get(i)?.key.as_str();

    if let Some(unit) = single_word_repeater(key, langs) {
        return Some((1, every(1, unit)));
    }

    if !listed(langs, RU_REPEAT_HEADS, EN_REPEAT_HEADS, key) {
        return None;
    }

    let mut j = i + 1;
    let mut count: u32 = 1;
    if let Some(token) = tokens.get(j) {
        if let Ok(parsed) = token.key.parse::<u32>() {
            count = parsed;
            j += 1;
        } else if let Some(parsed) = langs.ru.then(|| ru_numeral(&token.key)).flatten() {
            count = parsed;
            j += 1;
        }
    }

    // A zero step is no repeater: `parse_repeater` refuses `+0d` because the
    // occurrence math divides by the step, so the words stay in the heading
    // instead of becoming a value the grammar cannot read back.
    if count == 0 {
        return None;
    }

    // "каждый рабочий день" / "every working day" — the working-day repeater
    // the timestamp grammar spells `+1wd`.
    if let Some(token) = tokens.get(j) {
        if listed(langs, RU_WORKING_WORDS, EN_WORKING_WORDS, &token.key) {
            let unit = span_of(&tokens.get(j + 1)?.key, langs)?;
            if unit != Span::Day {
                return None;
            }
            return Some((j + 2 - i, every(count, RepeaterUnit::Workday)));
        }
        if langs.en && EN_WORKDAY_NOUNS.contains(&token.key.as_str()) {
            return Some((j + 1 - i, every(count, RepeaterUnit::Workday)));
        }
    }

    let span = span_of(&tokens.get(j)?.key, langs)?;
    Some((j + 1 - i, every(count, span.unit())))
}

const RU_SINGLE_WORD_REPEATERS: &[(&str, RepeaterUnit)] = &[
    ("ежедневно", RepeaterUnit::Day),
    ("еженедельно", RepeaterUnit::Week),
    ("ежемесячно", RepeaterUnit::Month),
    ("ежегодно", RepeaterUnit::Year),
];

const EN_SINGLE_WORD_REPEATERS: &[(&str, RepeaterUnit)] = &[
    ("daily", RepeaterUnit::Day),
    ("weekly", RepeaterUnit::Week),
    ("monthly", RepeaterUnit::Month),
    ("yearly", RepeaterUnit::Year),
    ("annually", RepeaterUnit::Year),
];

fn single_word_repeater(key: &str, langs: Languages) -> Option<RepeaterUnit> {
    in_tables(
        langs,
        RU_SINGLE_WORD_REPEATERS,
        EN_SINGLE_WORD_REPEATERS,
        key,
    )
}

/// A phrase names a plain `+N` repeater: what a person says is "every N of
/// these", not "catch up" or "restart from completion", which the timestamp
/// grammar spells `++` and `.+` and which no phrasing here produces.
fn every(value: u32, unit: RepeaterUnit) -> Repeater {
    Repeater {
        repeater_type: RepeaterType::Cumulative,
        value,
        unit,
    }
}

// --- priority --------------------------------------------------------------

const RU_PRIORITY_WORDS: &[(&str, Priority)] = &[
    ("срочно", Priority::A),
    ("срочное", Priority::A),
    ("срочная", Priority::A),
    ("срочную", Priority::A),
    ("срочной", Priority::A),
    ("критично", Priority::A),
    ("критичное", Priority::A),
    ("критичная", Priority::A),
    ("критичную", Priority::A),
    ("критичной", Priority::A),
    ("важно", Priority::B),
    ("важное", Priority::B),
    ("важная", Priority::B),
    ("важную", Priority::B),
    ("важной", Priority::B),
];

const EN_PRIORITY_WORDS: &[(&str, Priority)] = &[
    ("urgent", Priority::A),
    ("asap", Priority::A),
    ("critical", Priority::A),
    ("important", Priority::B),
];

fn match_priority(tokens: &[Token<'_>], i: usize, langs: Languages) -> Option<(usize, Priority)> {
    let key = tokens.get(i)?.key.as_str();

    if langs.ru {
        // "очень важно" is what "срочно" says in two words.
        if key == "очень" && ru_priority_word(word_at(tokens, i + 1)) == Some(Priority::B) {
            return Some((2, Priority::A));
        }
        if let Some(priority) = ru_priority_word(key) {
            return Some((1, priority));
        }
        if key == "приоритет" {
            return named_priority(tokens, i);
        }
    }
    if langs.en {
        if let Some(priority) = lookup(EN_PRIORITY_WORDS, key) {
            return Some((1, priority));
        }
        if key == "priority" {
            return named_priority(tokens, i);
        }
    }
    None
}

/// How urgency is said in Russian, in the genders and cases it is said in:
/// "срочно" of a new entry, "сделай срочной" of one that exists.
fn ru_priority_word(key: &str) -> Option<Priority> {
    lookup(RU_PRIORITY_WORDS, key)
}

/// `приоритет B` / `priority 3` — the cookie said outright.
fn named_priority(tokens: &[Token<'_>], i: usize) -> Option<(usize, Priority)> {
    let value = tokens.get(i + 1)?.key.to_uppercase();
    Priority::parse(&value).map(|priority| (2, priority))
}

// --- word tables -----------------------------------------------------------

/// Which language a table entry came from. Only the month tables need it: the
/// "month day" order is English.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Ru,
    En,
}

/// Look a key up in a word table.
fn lookup<T: Clone>(table: &[(&str, T)], key: &str) -> Option<T> {
    table
        .iter()
        .find(|(word, _)| *word == key)
        .map(|(_, value)| value.clone())
}

/// Look a key up in the tables of the languages the caller allows, Russian
/// first, so a word that stands in both is read as Russian.
fn in_tables<T: Clone>(
    langs: Languages,
    ru: &[(&str, T)],
    en: &[(&str, T)],
    key: &str,
) -> Option<T> {
    langs
        .ru
        .then(|| lookup(ru, key))
        .flatten()
        .or_else(|| langs.en.then(|| lookup(en, key)).flatten())
}

/// Whether a key stands in the lists of the languages the caller allows.
fn listed(langs: Languages, ru: &[&str], en: &[&str], key: &str) -> bool {
    (langs.ru && ru.contains(&key)) || (langs.en && en.contains(&key))
}

/// A span of calendar time, shared by "через N …" and "каждые N …".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Span {
    Day,
    Week,
    Month,
    Year,
}

impl Span {
    fn unit(self) -> RepeaterUnit {
        match self {
            Span::Day => RepeaterUnit::Day,
            Span::Week => RepeaterUnit::Week,
            Span::Month => RepeaterUnit::Month,
            Span::Year => RepeaterUnit::Year,
        }
    }
}

fn shift(from: NaiveDate, count: u32, span: Span) -> Option<NaiveDate> {
    match span {
        Span::Day => from.checked_add_days(Days::new(u64::from(count))),
        Span::Week => from.checked_add_days(Days::new(u64::from(count) * 7)),
        Span::Month => from.checked_add_months(Months::new(count)),
        Span::Year => from.checked_add_months(Months::new(count.checked_mul(12)?)),
    }
}

const RU_SPANS: &[(&str, Span)] = &[
    ("день", Span::Day),
    ("дня", Span::Day),
    ("дней", Span::Day),
    ("дни", Span::Day),
    ("неделя", Span::Week),
    ("неделю", Span::Week),
    ("недели", Span::Week),
    ("недель", Span::Week),
    ("месяц", Span::Month),
    ("месяца", Span::Month),
    ("месяцев", Span::Month),
    ("год", Span::Year),
    ("года", Span::Year),
    ("лет", Span::Year),
];

const EN_SPANS: &[(&str, Span)] = &[
    ("day", Span::Day),
    ("days", Span::Day),
    ("week", Span::Week),
    ("weeks", Span::Week),
    ("month", Span::Month),
    ("months", Span::Month),
    ("year", Span::Year),
    ("years", Span::Year),
];

fn span_of(key: &str, langs: Languages) -> Option<Span> {
    in_tables(langs, RU_SPANS, EN_SPANS, key)
}

const RU_NUMERALS: &[(&str, u32)] = &[
    ("два", 2),
    ("две", 2),
    ("три", 3),
    ("четыре", 4),
    ("пять", 5),
    ("шесть", 6),
    ("семь", 7),
    ("восемь", 8),
    ("девять", 9),
    ("десять", 10),
];

fn ru_numeral(key: &str) -> Option<u32> {
    lookup(RU_NUMERALS, key)
}

/// The Russian hour words, "час" through "двенадцать".
const RU_HOUR_WORDS: &[(&str, u32)] = &[
    ("час", 1),
    ("два", 2),
    ("две", 2),
    ("три", 3),
    ("четыре", 4),
    ("пять", 5),
    ("шесть", 6),
    ("семь", 7),
    ("восемь", 8),
    ("девять", 9),
    ("десять", 10),
    ("одиннадцать", 11),
    ("двенадцать", 12),
];

/// The hours as they are said: "в час дня", "в три часа".
fn ru_hour_word(key: &str) -> Option<u32> {
    lookup(RU_HOUR_WORDS, key)
}

/// The Russian weekday names, in every case the rules look up.
const RU_WEEKDAYS: &[(&str, Weekday)] = &[
    ("понедельник", Weekday::Mon),
    ("понедельника", Weekday::Mon),
    ("понедельнику", Weekday::Mon),
    ("пн", Weekday::Mon),
    ("вторник", Weekday::Tue),
    ("вторника", Weekday::Tue),
    ("вторнику", Weekday::Tue),
    ("вт", Weekday::Tue),
    ("среда", Weekday::Wed),
    ("среду", Weekday::Wed),
    ("среды", Weekday::Wed),
    ("среде", Weekday::Wed),
    ("ср", Weekday::Wed),
    ("четверг", Weekday::Thu),
    ("четверга", Weekday::Thu),
    ("четвергу", Weekday::Thu),
    ("чт", Weekday::Thu),
    ("пятница", Weekday::Fri),
    ("пятницу", Weekday::Fri),
    ("пятницы", Weekday::Fri),
    ("пятнице", Weekday::Fri),
    ("пт", Weekday::Fri),
    ("суббота", Weekday::Sat),
    ("субботу", Weekday::Sat),
    ("субботы", Weekday::Sat),
    ("субботе", Weekday::Sat),
    ("сб", Weekday::Sat),
    ("воскресенье", Weekday::Sun),
    ("воскресенья", Weekday::Sun),
    ("воскресенью", Weekday::Sun),
    ("вс", Weekday::Sun),
];

const EN_WEEKDAYS: &[(&str, Weekday)] = &[
    ("monday", Weekday::Mon),
    ("mon", Weekday::Mon),
    ("tuesday", Weekday::Tue),
    ("tue", Weekday::Tue),
    ("tues", Weekday::Tue),
    ("wednesday", Weekday::Wed),
    ("wed", Weekday::Wed),
    ("thursday", Weekday::Thu),
    ("thu", Weekday::Thu),
    ("thurs", Weekday::Thu),
    ("friday", Weekday::Fri),
    ("fri", Weekday::Fri),
    ("saturday", Weekday::Sat),
    ("sat", Weekday::Sat),
    ("sunday", Weekday::Sun),
    ("sun", Weekday::Sun),
];

/// Weekday names in the cases they are said in: nominative, accusative after
/// "в", dative after "к", genitive after "до", and the abbreviations the
/// timestamp grammar already knows.
fn weekday_of(key: &str, langs: Languages) -> Option<Weekday> {
    in_tables(langs, RU_WEEKDAYS, EN_WEEKDAYS, key)
}

const RU_MONTHS: &[(&str, u32)] = &[
    ("январь", 1),
    ("января", 1),
    ("февраль", 2),
    ("февраля", 2),
    ("март", 3),
    ("марта", 3),
    ("апрель", 4),
    ("апреля", 4),
    ("май", 5),
    ("мая", 5),
    ("июнь", 6),
    ("июня", 6),
    ("июль", 7),
    ("июля", 7),
    ("август", 8),
    ("августа", 8),
    ("сентябрь", 9),
    ("сентября", 9),
    ("октябрь", 10),
    ("октября", 10),
    ("ноябрь", 11),
    ("ноября", 11),
    ("декабрь", 12),
    ("декабря", 12),
];

const EN_MONTHS: &[(&str, u32)] = &[
    ("january", 1),
    ("jan", 1),
    ("february", 2),
    ("feb", 2),
    ("march", 3),
    ("mar", 3),
    ("april", 4),
    ("apr", 4),
    ("may", 5),
    ("june", 6),
    ("jun", 6),
    ("july", 7),
    ("jul", 7),
    ("august", 8),
    ("aug", 8),
    ("september", 9),
    ("sep", 9),
    ("sept", 9),
    ("october", 10),
    ("oct", 10),
    ("november", 11),
    ("nov", 11),
    ("december", 12),
    ("dec", 12),
];

fn month_of(key: &str, langs: Languages) -> Option<(u32, Lang)> {
    if langs.ru {
        if let Some(month) = lookup(RU_MONTHS, key) {
            return Some((month, Lang::Ru));
        }
    }
    if langs.en {
        if let Some(month) = lookup(EN_MONTHS, key) {
            return Some((month, Lang::En));
        }
    }
    None
}

// --- heading ---------------------------------------------------------------

/// Append what the rules did not consume, separated by a space. Trailing
/// punctuation left behind by a consumed tail ("позвонить врачу, завтра")
/// is trimmed; punctuation inside the text is not touched.
fn append_heading(heading: &mut String, leftover: &[&str]) {
    let joined = leftover.join(" ");
    let text = joined.trim_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':'));
    if text.is_empty() {
        return;
    }
    if heading.is_empty() {
        heading.push_str(text);
    } else {
        heading.push(' ');
        heading.push_str(text);
    }
}

/// The JSON shape of a parsed entry, which is what `parse-phrase` prints and
/// what the VS Code extension reads. Every field is nullable except the
/// heading and `cleared`, which is an array of field names and is empty when
/// the phrase emptied nothing; dates are `YYYY-MM-DD`, times are `HH:MM`, and
/// the repeater is the org-mode spelling (`+1w`). Per ADR-0015 a consumer
/// ignores keys it does not know.
impl Serialize for PhraseEntry {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut state = ser.serialize_struct("PhraseEntry", 9)?;
        state.serialize_field("heading", &self.heading)?;
        state.serialize_field("keyword", &self.keyword.map(PhraseKeyword::as_str))?;
        state.serialize_field("priority", &self.priority)?;
        state.serialize_field("planning", &self.planning.map(PlanningKind::as_str))?;
        state.serialize_field("date", &self.date.map(|date| date.to_string()))?;
        state.serialize_field(
            "time",
            &self.time.map(|time| time.format("%H:%M").to_string()),
        )?;
        state.serialize_field("repeater", &self.repeater.as_ref().map(Repeater::canonical))?;
        // The pair rather than the string it is written as: a client that
        // subtracts it needs the number and the unit apart, and the string is
        // one `canonical()` away for a client that writes the property.
        state.serialize_field("reminder", &self.reminder)?;
        state.serialize_field("cleared", &self.cleared.names())?;
        state.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: Languages = Languages { ru: true, en: true };

    fn day(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("valid day")
    }

    #[test]
    fn a_key_drops_surrounding_punctuation_and_folds_yo() {
        assert_eq!(key_of("«Завтра»,"), "завтра");
        assert_eq!(key_of("Ещё"), "еще");
        // A colon and a hyphen inside the word are what make a time and an
        // ISO date one token, so trimming must not reach them.
        assert_eq!(key_of("(15:00)"), "15:00");
        assert_eq!(key_of("2026-09-15."), "2026-09-15");
    }

    #[test]
    fn a_weekday_resolves_on_or_after_the_reference_day() {
        // Monday 2026-08-31. Upstream's bare-weekday rule picks the day "on or
        // after" the reference, so Monday said on a Monday is that Monday.
        let monday = day(2026, 8, 31);

        assert_eq!(nearest_weekday(monday, Weekday::Mon), monday);
        assert_eq!(nearest_weekday(monday, Weekday::Tue), day(2026, 9, 1));
        assert_eq!(nearest_weekday(monday, Weekday::Sun), day(2026, 9, 6));
    }

    #[test]
    fn a_month_day_without_a_year_takes_the_next_occurrence() {
        let today = day(2026, 8, 31);

        // Still ahead this year.
        assert_eq!(resolve_date(None, 9, 15, today), Some(day(2026, 9, 15)));
        // Already gone: the same day next year.
        assert_eq!(resolve_date(None, 1, 15, today), Some(day(2027, 1, 15)));
        // A day that does not exist every year walks forward until it does.
        assert_eq!(resolve_date(None, 2, 29, today), Some(day(2028, 2, 29)));
        // A named year is taken as said, past or future.
        assert_eq!(
            resolve_date(Some(2020), 2, 29, today),
            Some(day(2020, 2, 29))
        );
    }

    #[test]
    fn a_locale_naming_no_known_language_enables_both() {
        assert_eq!(Languages::from_locale("ru,en"), BOTH);
        assert_eq!(Languages::from_locale(""), BOTH);
        assert_eq!(Languages::from_locale("de"), BOTH);
        assert_eq!(
            Languages::from_locale("ru"),
            Languages {
                ru: true,
                en: false
            }
        );
    }

    #[test]
    fn the_longest_lead_in_wins() {
        // "напомни мне" must eat both words: matching "напомни" alone would
        // leave "мне" in the heading.
        assert_eq!(lead_in_len(&tokenize("напомни мне купить хлеб"), BOTH), 2);
        assert_eq!(lead_in_len(&tokenize("напомни купить хлеб"), BOTH), 1);
        assert_eq!(lead_in_len(&tokenize("remind me to call"), BOTH), 3);
        assert_eq!(lead_in_len(&tokenize("купить хлеб"), BOTH), 0);
    }

    #[test]
    fn the_heading_grows_by_one_space_and_loses_a_dangling_comma() {
        let mut heading = String::new();
        append_heading(&mut heading, &["позвонить", "врачу,"]);
        assert_eq!(heading, "позвонить врачу");

        append_heading(&mut heading, &["из", "поликлиники"]);
        assert_eq!(heading, "позвонить врачу из поликлиники");

        // A phrase the rules consumed entirely leaves the heading alone.
        append_heading(&mut heading, &[]);
        assert_eq!(heading, "позвонить врачу из поликлиники");
    }

    #[test]
    fn the_emptied_fields_are_named_in_a_fixed_order() {
        let mut cleared = ClearedFields::default();
        assert!(cleared.is_empty());
        assert_eq!(cleared.names(), Vec::<&str>::new());

        cleared.priority = true;
        cleared.date = true;
        // The order is the one the JSON prints, not the order the fields were
        // emptied in: a caller comparing two answers compares two arrays.
        assert_eq!(cleared.names(), vec!["date", "priority"]);
        assert!(!cleared.is_empty());
    }

    #[test]
    fn a_removal_is_read_only_when_it_names_a_field() {
        let field = |phrase| match_clear(&tokenize(phrase), 0, BOTH).map(|(_, field)| field);

        assert_eq!(field("убрать дату"), Some(Field::Date));
        assert_eq!(field("remove the repeater"), Some(Field::Repeater));
        // A verb of removal with no field after it is a word like any other,
        // and so is a phrase about something that is not a field.
        assert_eq!(field("убрать"), None);
        assert_eq!(field("без сахара"), None);
        assert_eq!(field("remove the milk"), None);
    }

    #[test]
    fn an_hour_is_read_from_the_forms_a_person_says() {
        let time = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0);

        assert_eq!(parse_hhmm("15:00"), time(15, 0));
        assert_eq!(parse_hhmm("7:05"), time(7, 5));
        assert_eq!(parse_hhmm("25:00"), None);
        assert_eq!(parse_hhmm("15:0"), None);
        assert_eq!(parse_am_pm("3pm"), time(15, 0));
        assert_eq!(parse_am_pm("12am"), time(0, 0));
        assert_eq!(parse_am_pm("3:30pm"), time(15, 30));
        assert_eq!(parse_am_pm("15pm"), None);
    }

    // --- the word tables, walked whole ------------------------------------
    //
    // Every table below is walked entry by entry, so a word added to the
    // grammar is checked by the same test without anyone widening a list of
    // examples. What each test states is that the word reaches its rule
    // through the whole parser, not merely that the table holds it.

    const RU: Languages = Languages {
        ru: true,
        en: false,
    };
    const EN: Languages = Languages {
        ru: false,
        en: true,
    };

    /// The day every phrase in these tests is relative to: Monday 2026-08-31.
    fn reference_day() -> NaiveDate {
        day(2026, 8, 31)
    }

    fn parsed(phrase: &str, locale: &str) -> PhraseEntry {
        parse_phrases([phrase], locale, reference_day())
    }

    /// The wire name a field is listed under once it is emptied.
    fn field_name(field: Field) -> &'static str {
        match field {
            Field::Date => "date",
            Field::Time => "time",
            Field::Repeater => "repeater",
            Field::Priority => "priority",
            Field::Reminder => "reminder",
        }
    }

    /// One language: which grammar to read with, the two lists of verbs it
    /// has, and a phrase in it for the verbs to stand in front of.
    type LeadInCase = (
        Languages,
        [&'static [&'static [&'static str]]; 2],
        &'static str,
    );

    #[test]
    fn every_lead_in_verb_is_taken_off_the_front_of_a_phrase() {
        let cases: [LeadInCase; 2] = [
            (RU, [RU_LEAD_INS, RU_EDIT_INS], "позвонить врачу"),
            (EN, [EN_LEAD_INS, EN_EDIT_INS], "call the doctor"),
        ];

        for (langs, lists, rest) in cases {
            for list in lists {
                for words in list {
                    let phrase = format!("{} {rest}", words.join(" "));
                    let tokens = tokenize(&phrase);

                    assert_eq!(
                        lead_in_len(&tokens, langs),
                        words.len(),
                        "lead-in {words:?} in {phrase:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_lead_in_list_states_the_longer_verb_first() {
        for list in [RU_LEAD_INS, EN_LEAD_INS, RU_EDIT_INS, EN_EDIT_INS] {
            for (i, words) in list.iter().enumerate() {
                for earlier in &list[..i] {
                    // A shorter verb standing first would swallow the longer
                    // one: "add a task" before "add a task to" leaves the "to"
                    // in the heading.
                    assert!(
                        !(earlier.len() < words.len() && words.starts_with(earlier)),
                        "{earlier:?} stands before the longer {words:?}"
                    );
                }
                assert!(!list[..i].contains(words), "{words:?} is listed twice");
            }
        }
    }

    // --- the tables against a reference written apart from them -----------
    //
    // The walks above state that a word reaches its rule; they cannot state
    // that the rule gives the right answer, because the answer they compare
    // against comes out of the same table. These check the meaning: the
    // Russian forms against a list written here, the English ones against
    // what `chrono` itself knows.

    /// The Russian month names, nominative and genitive, listed apart from
    /// the parser's own table.
    const RU_MONTH_REFERENCE: &[(&[&str], u32)] = &[
        (&["январь", "января"], 1),
        (&["февраль", "февраля"], 2),
        (&["март", "марта"], 3),
        (&["апрель", "апреля"], 4),
        (&["май", "мая"], 5),
        (&["июнь", "июня"], 6),
        (&["июль", "июля"], 7),
        (&["август", "августа"], 8),
        (&["сентябрь", "сентября"], 9),
        (&["октябрь", "октября"], 10),
        (&["ноябрь", "ноября"], 11),
        (&["декабрь", "декабря"], 12),
    ];

    /// The Russian weekday names in the cases the grammar accepts, and the
    /// two-letter abbreviations of the timestamp grammar.
    const RU_WEEKDAY_REFERENCE: &[(&[&str], Weekday)] = &[
        (
            &["понедельник", "понедельника", "понедельнику", "пн"],
            Weekday::Mon,
        ),
        (&["вторник", "вторника", "вторнику", "вт"], Weekday::Tue),
        (&["среда", "среду", "среды", "среде", "ср"], Weekday::Wed),
        (&["четверг", "четверга", "четвергу", "чт"], Weekday::Thu),
        (
            &["пятница", "пятницу", "пятницы", "пятнице", "пт"],
            Weekday::Fri,
        ),
        (
            &["суббота", "субботу", "субботы", "субботе", "сб"],
            Weekday::Sat,
        ),
        (
            &["воскресенье", "воскресенья", "воскресенью", "вс"],
            Weekday::Sun,
        ),
    ];

    const RU_NUMERAL_REFERENCE: &[(&[&str], u32)] = &[
        (&["два", "две"], 2),
        (&["три"], 3),
        (&["четыре"], 4),
        (&["пять"], 5),
        (&["шесть"], 6),
        (&["семь"], 7),
        (&["восемь"], 8),
        (&["девять"], 9),
        (&["десять"], 10),
    ];

    const RU_HOUR_REFERENCE: &[(&[&str], u32)] = &[
        (&["час"], 1),
        (&["два", "две"], 2),
        (&["три"], 3),
        (&["четыре"], 4),
        (&["пять"], 5),
        (&["шесть"], 6),
        (&["семь"], 7),
        (&["восемь"], 8),
        (&["девять"], 9),
        (&["десять"], 10),
        (&["одиннадцать"], 11),
        (&["двенадцать"], 12),
    ];

    /// The English forms `chrono` does not know: it parses neither `sept` nor
    /// the four-letter weekday abbreviations, and the grammar accepts both.
    const EN_MONTHS_CHRONO_MISSES: &[(&str, u32)] = &[("sept", 9)];

    const EN_WEEKDAYS_CHRONO_MISSES: &[(&str, Weekday)] =
        &[("tues", Weekday::Tue), ("thurs", Weekday::Thu)];

    /// Both directions at once: every form of the reference is in the table
    /// with the same value, and the table holds nothing the reference does
    /// not name.
    fn agrees_with<T: PartialEq + std::fmt::Debug + Clone>(
        name: &str,
        table: &[(&str, T)],
        reference: &[(&[&str], T)],
    ) {
        for (words, value) in reference {
            for word in *words {
                assert_eq!(
                    lookup(table, word).as_ref(),
                    Some(value),
                    "{name} reads {word} as something else"
                );
            }
        }
        for (word, _) in table {
            assert!(
                reference.iter().any(|(words, _)| words.contains(word)),
                "{name} states {word}, which the reference list does not"
            );
        }
    }

    /// What each word says about the state of an entry, written apart from
    /// the parser's table: "выполнено" and its kin close an entry, "отменено"
    /// and its kin cancel it. Swapping the two halves of the table reads a
    /// phrase as the opposite of what was said, and the walk over the table
    /// cannot see it.
    const RU_KEYWORD_REFERENCE: &[(&[&str], PhraseKeyword)] = &[
        (
            &[
                "выполнено",
                "выполнена",
                "выполнен",
                "выполненной",
                "выполненную",
                "сделано",
                "сделана",
                "готово",
                "завершено",
                "завершена",
            ],
            PhraseKeyword::Done,
        ),
        (
            &[
                "отменено",
                "отменена",
                "отменен",
                "отмененной",
                "отмененную",
            ],
            PhraseKeyword::Cancelled,
        ),
    ];

    const EN_KEYWORD_REFERENCE: &[(&[&str], PhraseKeyword)] = &[
        (&["done", "completed"], PhraseKeyword::Done),
        (&["todo"], PhraseKeyword::Todo),
        (&["cancelled", "canceled"], PhraseKeyword::Cancelled),
    ];

    /// Which level each word asks for. "срочно" and "критично" are the top
    /// one, "важно" the one below; the three levels are not visible in the
    /// words themselves, so the table alone cannot say a form was filed
    /// under the wrong one.
    const RU_PRIORITY_REFERENCE: &[(&[&str], Priority)] = &[
        (
            &[
                "срочно",
                "срочное",
                "срочная",
                "срочную",
                "срочной",
                "критично",
                "критичное",
                "критичная",
                "критичную",
                "критичной",
            ],
            Priority::A,
        ),
        (
            &["важно", "важное", "важная", "важную", "важной"],
            Priority::B,
        ),
    ];

    const EN_PRIORITY_REFERENCE: &[(&[&str], Priority)] = &[
        (&["urgent", "asap", "critical"], Priority::A),
        (&["important"], Priority::B),
    ];

    /// Which half of the day each word names. `true` is the afternoon, the
    /// half that shifts an hour by twelve.
    const RU_HALF_DAY_REFERENCE: &[(&[&str], bool)] =
        &[(&["дня", "вечера"], true), (&["утра", "ночи"], false)];

    const EN_HALF_DAY_REFERENCE: &[(&[&str], bool)] = &[(&["pm"], true), (&["am"], false)];

    #[test]
    fn the_russian_tables_agree_with_a_list_written_apart_from_them() {
        agrees_with("RU_MONTHS", RU_MONTHS, RU_MONTH_REFERENCE);
        agrees_with("RU_WEEKDAYS", RU_WEEKDAYS, RU_WEEKDAY_REFERENCE);
        agrees_with("RU_NUMERALS", RU_NUMERALS, RU_NUMERAL_REFERENCE);
        agrees_with("RU_HOUR_WORDS", RU_HOUR_WORDS, RU_HOUR_REFERENCE);
        agrees_with("RU_KEYWORDS", RU_KEYWORDS, RU_KEYWORD_REFERENCE);
        agrees_with(
            "RU_PRIORITY_WORDS",
            RU_PRIORITY_WORDS,
            RU_PRIORITY_REFERENCE,
        );
        agrees_with("RU_HALF_DAYS", RU_HALF_DAYS, RU_HALF_DAY_REFERENCE);
    }

    #[test]
    fn the_english_tables_agree_with_a_list_written_apart_from_them() {
        agrees_with("EN_KEYWORDS", EN_KEYWORDS, EN_KEYWORD_REFERENCE);
        agrees_with(
            "EN_PRIORITY_WORDS",
            EN_PRIORITY_WORDS,
            EN_PRIORITY_REFERENCE,
        );
        agrees_with("EN_HALF_DAYS", EN_HALF_DAYS, EN_HALF_DAY_REFERENCE);
    }

    #[test]
    fn the_english_tables_agree_with_what_chrono_reads() {
        for (word, month) in EN_MONTHS {
            if let Some(expected) = lookup(EN_MONTHS_CHRONO_MISSES, word) {
                assert_eq!(*month, expected, "month word {word}");
                continue;
            }
            let read = NaiveDate::parse_from_str(&format!("{word} 15 2026"), "%B %d %Y")
                .or_else(|_| NaiveDate::parse_from_str(&format!("{word} 15 2026"), "%b %d %Y"))
                .unwrap_or_else(|e| panic!("chrono does not read the month {word}: {e}"));

            assert_eq!(read.month(), *month, "month word {word}");
        }

        for (word, weekday) in EN_WEEKDAYS {
            if let Some(expected) = lookup(EN_WEEKDAYS_CHRONO_MISSES, word) {
                assert_eq!(*weekday, expected, "weekday word {word}");
                continue;
            }
            let read: Weekday = word
                .parse()
                .unwrap_or_else(|_| panic!("chrono does not read the weekday {word}"));

            assert_eq!(read, *weekday, "weekday word {word}");
        }
    }

    #[test]
    fn every_month_word_resolves_to_its_month() {
        for (word, month) in RU_MONTHS {
            let entry = parsed(&format!("встреча 15 {word}"), "ru");
            let date = entry.date.unwrap_or_else(|| panic!("no date from {word}"));

            assert_eq!(date.month(), *month, "month word {word}");
            assert_eq!(date.day(), 15, "month word {word}");
        }
        for (word, month) in EN_MONTHS {
            let entry = parsed(&format!("meeting {word} 15"), "en");
            let date = entry.date.unwrap_or_else(|| panic!("no date from {word}"));

            assert_eq!(date.month(), *month, "month word {word}");
            assert_eq!(date.day(), 15, "month word {word}");
        }

        for month in 1..=12 {
            assert!(
                RU_MONTHS.iter().any(|(_, value)| *value == month),
                "no Russian word for month {month}"
            );
            assert!(
                EN_MONTHS.iter().any(|(_, value)| *value == month),
                "no English word for month {month}"
            );
        }
    }

    #[test]
    fn every_weekday_word_resolves_to_its_day() {
        for (word, weekday) in RU_WEEKDAYS {
            let entry = parsed(&format!("позвонить в {word}"), "ru");
            let date = entry.date.unwrap_or_else(|| panic!("no date from {word}"));

            assert_eq!(date.weekday(), *weekday, "weekday word {word}");
        }
        for (word, weekday) in EN_WEEKDAYS {
            let entry = parsed(&format!("call on {word}"), "en");
            let date = entry.date.unwrap_or_else(|| panic!("no date from {word}"));

            assert_eq!(date.weekday(), *weekday, "weekday word {word}");
        }

        for weekday in [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
            Weekday::Sat,
            Weekday::Sun,
        ] {
            assert!(
                RU_WEEKDAYS.iter().any(|(_, value)| *value == weekday),
                "no Russian word for {weekday:?}"
            );
            assert!(
                EN_WEEKDAYS.iter().any(|(_, value)| *value == weekday),
                "no English word for {weekday:?}"
            );
        }
    }

    /// One language: its table of spans, its locale, and the two phrasings
    /// that count in spans, with `{word}` where the span word stands.
    type SpanCase = (
        &'static [(&'static str, Span)],
        &'static str,
        &'static str,
        &'static str,
    );

    #[test]
    fn every_span_word_is_read_by_both_rules_that_count_spans() {
        let today = reference_day();
        let cases: [SpanCase; 2] = [
            (
                RU_SPANS,
                "ru",
                "оплата через 2 {word}",
                "оплата каждые 2 {word}",
            ),
            (EN_SPANS, "en", "pay in 2 {word}", "pay every 2 {word}"),
        ];

        for (table, locale, ahead, repeated) in cases {
            for (word, span) in table {
                let shifted = parsed(&ahead.replace("{word}", word), locale);
                assert_eq!(
                    shifted.date,
                    shift(today, 2, *span),
                    "span word {word} counted forward"
                );

                let repeating = parsed(&repeated.replace("{word}", word), locale);
                let repeater = repeating
                    .repeater
                    .unwrap_or_else(|| panic!("no repeater from {word}"));
                assert_eq!(
                    repeater.canonical(),
                    every(2, span.unit()).canonical(),
                    "span word {word} repeated"
                );
            }

            for span in [Span::Day, Span::Week, Span::Month, Span::Year] {
                assert!(
                    table.iter().any(|(_, value)| *value == span),
                    "no {locale} word for {span:?}"
                );
            }
        }
    }

    #[test]
    fn every_numeral_word_counts_that_many_days() {
        for (word, count) in RU_NUMERALS {
            let entry = parsed(&format!("отчет через {word} дня"), "ru");

            assert_eq!(
                entry.date,
                shift(reference_day(), *count, Span::Day),
                "numeral {word}"
            );
        }
    }

    #[test]
    fn every_hour_word_is_read_as_that_hour() {
        for (word, hour) in RU_HOUR_WORDS {
            let entry = parsed(&format!("созвон в {word}"), "ru");

            assert_eq!(
                entry.time,
                NaiveTime::from_hms_opt(*hour, 0, 0),
                "hour word {word}"
            );
        }
    }

    #[test]
    fn every_word_around_an_hour_turns_a_number_into_a_time() {
        // Without one of these words a bare number stays a number.
        assert_eq!(parsed("позвонить 3", "ru").time, None);

        for word in RU_HOUR_UNITS {
            assert_eq!(
                parsed(&format!("позвонить 3 {word}"), "ru").time,
                NaiveTime::from_hms_opt(3, 0, 0),
                "hours word {word}"
            );
        }
        for word in EN_HOUR_UNITS {
            assert_eq!(
                parsed(&format!("call 3 {word}"), "en").time,
                NaiveTime::from_hms_opt(3, 0, 0),
                "hours word {word}"
            );
        }
        // The hour is written down rather than computed by `shift_half_day`:
        // the parser applies that same function, so an expectation built with
        // it agrees with any table, including one that files "утра" as the
        // afternoon and reads "3 утра" as 15:00.
        for (word, hour) in [("дня", 15), ("вечера", 15), ("утра", 3), ("ночи", 3)]
        {
            assert_eq!(
                parsed(&format!("позвонить 3 {word}"), "ru").time,
                NaiveTime::from_hms_opt(hour, 0, 0),
                "half-day word {word}"
            );
        }
        for (word, hour) in [("pm", 15), ("am", 3)] {
            assert_eq!(
                parsed(&format!("call 3 {word}"), "en").time,
                NaiveTime::from_hms_opt(hour, 0, 0),
                "half-day word {word}"
            );
        }
        for prefix in RU_TIME_PREFIXES {
            assert_eq!(
                parsed(&format!("позвонить {prefix} 15:30"), "ru").time,
                NaiveTime::from_hms_opt(15, 30, 0),
                "time preposition {prefix}"
            );
        }
        for prefix in EN_TIME_PREFIXES {
            assert_eq!(
                parsed(&format!("call {prefix} 15:30"), "en").time,
                NaiveTime::from_hms_opt(15, 30, 0),
                "time preposition {prefix}"
            );
        }
    }

    #[test]
    fn every_named_day_stands_where_the_table_says() {
        for (word, days) in RU_NAMED_DAYS {
            assert_eq!(
                parsed(&format!("позвонить {word}"), "ru").date,
                reference_day().checked_add_days(Days::new(*days)),
                "named day {word}"
            );
        }
        for (word, days) in EN_NAMED_DAYS {
            assert_eq!(
                parsed(&format!("call {word}"), "en").date,
                reference_day().checked_add_days(Days::new(*days)),
                "named day {word}"
            );
        }
        // The English day after tomorrow is said in three words.
        assert_eq!(
            parsed("call day after tomorrow", "en").date,
            reference_day().checked_add_days(Days::new(2))
        );
    }

    #[test]
    fn every_single_word_repeater_names_its_unit() {
        for (word, unit) in RU_SINGLE_WORD_REPEATERS {
            let entry = parsed(&format!("зарядка {word}"), "ru");

            assert_eq!(
                entry.repeater.map(|r| r.canonical()),
                Some(every(1, unit.clone()).canonical()),
                "repeater word {word}"
            );
        }
        for (word, unit) in EN_SINGLE_WORD_REPEATERS {
            let entry = parsed(&format!("exercise {word}"), "en");

            assert_eq!(
                entry.repeater.map(|r| r.canonical()),
                Some(every(1, unit.clone()).canonical()),
                "repeater word {word}"
            );
        }
    }

    #[test]
    fn every_word_of_a_working_day_repeater_is_read() {
        for word in RU_WORKING_WORDS {
            let entry = parsed(&format!("зарядка каждый {word} день"), "ru");

            assert_eq!(
                entry.repeater.map(|r| r.canonical()),
                Some(every(1, RepeaterUnit::Workday).canonical()),
                "working word {word}"
            );
        }
        for word in EN_WORKING_WORDS {
            let entry = parsed(&format!("exercise every {word} day"), "en");

            assert_eq!(
                entry.repeater.map(|r| r.canonical()),
                Some(every(1, RepeaterUnit::Workday).canonical()),
                "working word {word}"
            );
        }
        for word in EN_WORKDAY_NOUNS {
            let entry = parsed(&format!("exercise every {word}"), "en");

            assert_eq!(
                entry.repeater.map(|r| r.canonical()),
                Some(every(1, RepeaterUnit::Workday).canonical()),
                "workday noun {word}"
            );
        }
    }

    #[test]
    fn every_head_of_a_repeat_is_read() {
        for word in RU_REPEAT_HEADS {
            let entry = parsed(&format!("оплата {word} месяц"), "ru");

            assert_eq!(
                entry.repeater.map(|r| r.canonical()),
                Some(every(1, RepeaterUnit::Month).canonical()),
                "repeat head {word}"
            );
        }
        for word in EN_REPEAT_HEADS {
            let entry = parsed(&format!("pay {word} month"), "en");

            assert_eq!(
                entry.repeater.map(|r| r.canonical()),
                Some(every(1, RepeaterUnit::Month).canonical()),
                "repeat head {word}"
            );
        }
    }

    #[test]
    fn every_priority_word_names_its_cookie() {
        for (word, priority) in RU_PRIORITY_WORDS {
            assert_eq!(
                parsed(&format!("позвонить {word}"), "ru").priority,
                Some(priority.clone()),
                "priority word {word}"
            );
        }
        for (word, priority) in EN_PRIORITY_WORDS {
            assert_eq!(
                parsed(&format!("call {word}"), "en").priority,
                Some(priority.clone()),
                "priority word {word}"
            );
        }
        // "очень" raises the lower cookie rather than naming one of its own.
        assert_eq!(
            parsed("позвонить очень важно", "ru").priority,
            Some(Priority::A)
        );
    }

    #[test]
    fn every_keyword_word_names_its_keyword() {
        for (word, keyword) in RU_KEYWORDS {
            assert_eq!(
                parsed(word, "ru").keyword,
                Some(*keyword),
                "keyword word {word}"
            );
        }
        for (word, keyword) in EN_KEYWORDS {
            assert_eq!(
                parsed(word, "en").keyword,
                Some(*keyword),
                "keyword word {word}"
            );
        }
        for word in RU_BACK_TO_WORK {
            assert_eq!(
                parsed(&format!("в {word}"), "ru").keyword,
                Some(PhraseKeyword::Todo),
                "back to work {word}"
            );
        }
    }

    #[test]
    fn every_field_noun_names_the_field_it_empties() {
        for (word, field) in RU_FIELD_NOUNS {
            let entry = parsed(&format!("убрать {word}"), "ru");

            assert_eq!(
                entry.cleared.names(),
                [field_name(*field)],
                "field noun {word}"
            );
        }
        for (word, field) in EN_FIELD_NOUNS {
            let entry = parsed(&format!("remove the {word}"), "en");

            assert_eq!(
                entry.cleared.names(),
                [field_name(*field)],
                "field noun {word}"
            );
        }
    }

    #[test]
    fn every_word_of_removal_empties_the_field_it_names() {
        for word in RU_REMOVES {
            assert_eq!(
                parsed(&format!("{word} дату"), "ru").cleared.names(),
                ["date"],
                "removal verb {word}"
            );
        }
        for word in EN_REMOVES {
            // The article is optional after every one of these verbs.
            assert_eq!(
                parsed(&format!("{word} the date"), "en").cleared.names(),
                ["date"],
                "removal verb {word} with the article"
            );
            assert_eq!(
                parsed(&format!("{word} date"), "en").cleared.names(),
                ["date"],
                "removal verb {word}"
            );
        }
        for word in RU_WITHOUTS {
            assert_eq!(
                parsed(&format!("{word} даты"), "ru").cleared.names(),
                ["date"],
                "preposition of absence {word}"
            );
        }
        for word in EN_WITHOUTS {
            assert_eq!(
                parsed(&format!("{word} date"), "en").cleared.names(),
                ["date"],
                "preposition of absence {word}"
            );
        }
    }

    #[test]
    fn every_date_preposition_names_its_planning_line() {
        for (word, kind) in RU_DATE_PREFIXES {
            let entry = parsed(&format!("позвонить {word} завтра"), "ru");

            assert_eq!(entry.date, reference_day().checked_add_days(Days::new(1)));
            assert_eq!(entry.planning, Some(*kind), "date preposition {word}");
        }
        for (word, kind) in EN_DATE_PREFIXES {
            let entry = parsed(&format!("call {word} tomorrow"), "en");

            assert_eq!(entry.date, reference_day().checked_add_days(Days::new(1)));
            assert_eq!(entry.planning, Some(*kind), "date preposition {word}");
        }
    }

    #[test]
    fn every_negation_and_conjunction_is_read_as_one() {
        for word in RU_NEGATIONS {
            let entry = parsed(&format!("позвонить {word} завтра"), "ru");

            assert_eq!(entry.date, None, "negation {word}");
            assert_eq!(entry.heading, format!("позвонить {word} завтра"));
        }
        for word in EN_NEGATIONS {
            let entry = parsed(&format!("call {word} tomorrow"), "en");

            assert_eq!(entry.date, None, "negation {word}");
            assert_eq!(entry.heading, format!("call {word} tomorrow"));
        }
        for word in RU_CONJUNCTIONS {
            // A conjunction in front of a verb of editing joins two
            // instructions and is not part of the heading.
            let entry = parsed(&format!("позвонить {word} перенеси на завтра"), "ru");

            assert_eq!(entry.heading, "позвонить", "conjunction {word}");
            assert_eq!(entry.date, reference_day().checked_add_days(Days::new(1)));
        }
        for word in EN_CONJUNCTIONS {
            let entry = parsed(&format!("call {word} move it to tomorrow"), "en");

            assert_eq!(entry.heading, "call", "conjunction {word}");
            assert_eq!(entry.date, reference_day().checked_add_days(Days::new(1)));
        }
    }

    /// The lead time a phrase names, for a test that expects it to name one.
    fn lead_of(phrase: &str, locale: &str) -> ReminderLead {
        parsed(phrase, locale)
            .reminder
            .unwrap_or_else(|| panic!("no lead time in {phrase:?}"))
    }

    #[test]
    fn a_lead_time_is_said_with_за_in_russian() {
        assert_eq!(
            lead_of("напомни за час", "ru"),
            ReminderLead {
                value: 1,
                unit: ReminderUnit::Hour
            }
        );
        assert_eq!(
            lead_of("напомни за 15 минут", "ru"),
            ReminderLead {
                value: 15,
                unit: ReminderUnit::Minute
            }
        );
        assert_eq!(
            lead_of("за два дня", "ru"),
            ReminderLead {
                value: 2,
                unit: ReminderUnit::Day
            }
        );
        assert_eq!(
            lead_of("за неделю", "ru"),
            ReminderLead {
                value: 1,
                unit: ReminderUnit::Week
            }
        );
        assert_eq!(
            lead_of("напомни за полчаса", "ru"),
            ReminderLead {
                value: 30,
                unit: ReminderUnit::Minute
            },
            "half an hour is said as one word"
        );
    }

    #[test]
    fn a_lead_time_is_said_with_before_in_english() {
        assert_eq!(
            lead_of("remind me an hour before", "en"),
            ReminderLead {
                value: 1,
                unit: ReminderUnit::Hour
            }
        );
        assert_eq!(
            lead_of("remind 15 minutes before", "en"),
            ReminderLead {
                value: 15,
                unit: ReminderUnit::Minute
            }
        );
        assert_eq!(
            lead_of("a day before", "en"),
            ReminderLead {
                value: 1,
                unit: ReminderUnit::Day
            }
        );
        assert_eq!(
            lead_of("half an hour before", "en"),
            ReminderLead {
                value: 30,
                unit: ReminderUnit::Minute
            }
        );
    }

    #[test]
    fn a_count_and_a_unit_alone_are_not_a_lead_time() {
        // Each language marks a lead time with a word of its own, and without
        // that mark the words are ordinary ones: "an hour" is part of what the
        // entry says, and "час до созвона" is too.
        let english = parsed("call an hour", "en");

        assert_eq!(english.reminder, None);
        assert_eq!(english.heading, "call an hour");

        let russian = parsed("час до созвона", "ru");

        assert_eq!(russian.reminder, None);
    }

    #[test]
    fn the_words_of_a_lead_time_do_not_reach_the_heading() {
        let entry = parsed("напомни за час до созвона", "ru");

        assert_eq!(entry.heading, "созвона");
        assert_eq!(
            entry.reminder.map(|lead| lead.canonical()).as_deref(),
            Some("1h")
        );

        let english = parsed("remind me an hour before the call", "en");

        assert_eq!(english.heading, "the call");
    }

    #[test]
    fn a_verb_of_reminding_in_front_of_a_lead_time_stays_out_of_the_heading() {
        // The verb is eaten as a lead-in only at the head of a phrase. Said
        // where a person naturally says it -- after what the entry is -- it
        // belongs to the lead time behind it rather than to the heading.
        for (phrase, locale) in [
            ("позвонить врачу, напомни за час", "ru"),
            ("позвонить врачу, напомнить за час", "ru"),
            ("позвонить врачу, напомни мне за час", "ru"),
            ("позвонить врачу, напоминай за час", "ru"),
            ("call the doctor, remind me an hour before", "en"),
            ("call the doctor, remind an hour before", "en"),
        ] {
            let entry = parsed(phrase, locale);
            let heading = if locale == "ru" {
                "позвонить врачу"
            } else {
                "call the doctor"
            };

            assert_eq!(entry.heading, heading, "phrase {phrase:?}");
            assert_eq!(
                entry.reminder.map(|lead| lead.canonical()).as_deref(),
                Some("1h"),
                "phrase {phrase:?}"
            );
        }
    }

    #[test]
    fn a_verb_of_reminding_saying_nothing_about_a_lead_time_is_left_alone() {
        // Eaten only together with the lead time it introduces: "напомни про
        // отчёт" is what the entry is called, and a rule that ate the verb on
        // its own would leave the entry named "про отчёт".
        let entry = parsed("позвонить врачу и напомни про отчёт", "ru");

        assert_eq!(entry.heading, "позвонить врачу и напомни про отчёт");
        assert_eq!(entry.reminder, None);

        let english = parsed("call the doctor and remind the team", "en");

        assert_eq!(english.heading, "call the doctor and remind the team");
    }

    #[test]
    fn a_span_counted_ahead_is_not_a_lead_time() {
        // "через 2 дня" says when the entry is; "за 2 дня" says how long
        // before it the reminder is. One word apart, and the rules must not
        // read either as the other.
        let ahead = parsed("через 2 дня", "ru");

        assert_eq!(ahead.date, reference_day().checked_add_days(Days::new(2)));
        assert_eq!(ahead.reminder, None);

        let lead = parsed("за 2 дня", "ru");

        assert_eq!(lead.date, None);
        assert_eq!(
            lead.reminder,
            Some(ReminderLead {
                value: 2,
                unit: ReminderUnit::Day
            })
        );
    }

    #[test]
    fn a_lead_time_is_emptied_by_name() {
        for (phrase, locale) in [
            ("убрать напоминание", "ru"),
            ("без напоминания", "ru"),
            ("remove the reminder", "en"),
            ("no reminder", "en"),
        ] {
            let entry = parse_phrases(
                ["напомни за час", phrase],
                if locale == "ru" { "ru" } else { "ru,en" },
                reference_day(),
            );

            assert_eq!(entry.reminder, None, "phrase {phrase:?}");
            assert!(entry.cleared.reminder, "phrase {phrase:?}");
            assert!(
                entry.cleared.names().contains(&"reminder"),
                "phrase {phrase:?}"
            );
        }
    }

    #[test]
    fn a_lead_time_said_again_takes_back_the_emptying() {
        let entry = parse_phrases(
            ["убрать напоминание", "напомни за 10 минут"],
            "ru",
            reference_day(),
        );

        assert!(!entry.cleared.reminder);
        assert_eq!(
            entry.reminder.map(|lead| lead.canonical()).as_deref(),
            Some("10min")
        );
    }

    #[test]
    fn no_word_table_states_the_same_word_twice() {
        fn unique<T>(name: &str, table: &[(&str, T)]) {
            for (i, (word, _)) in table.iter().enumerate() {
                assert!(
                    !table[..i].iter().any(|(earlier, _)| earlier == word),
                    "{name} states {word} twice"
                );
            }
        }
        fn unique_words(name: &str, table: &[&str]) {
            for (i, word) in table.iter().enumerate() {
                assert!(!table[..i].contains(word), "{name} states {word} twice");
            }
        }

        unique("RU_SPANS", RU_SPANS);
        unique("EN_SPANS", EN_SPANS);
        unique("RU_NUMERALS", RU_NUMERALS);
        unique("RU_HOUR_WORDS", RU_HOUR_WORDS);
        unique("RU_WEEKDAYS", RU_WEEKDAYS);
        unique("EN_WEEKDAYS", EN_WEEKDAYS);
        unique("RU_MONTHS", RU_MONTHS);
        unique("EN_MONTHS", EN_MONTHS);
        unique("RU_NAMED_DAYS", RU_NAMED_DAYS);
        unique("EN_NAMED_DAYS", EN_NAMED_DAYS);
        unique("RU_SINGLE_WORD_REPEATERS", RU_SINGLE_WORD_REPEATERS);
        unique("EN_SINGLE_WORD_REPEATERS", EN_SINGLE_WORD_REPEATERS);
        unique("RU_LEAD_UNITS", RU_LEAD_UNITS);
        unique("EN_LEAD_UNITS", EN_LEAD_UNITS);
        unique("RU_FIELD_NOUNS", RU_FIELD_NOUNS);
        unique("EN_FIELD_NOUNS", EN_FIELD_NOUNS);
        unique("RU_DATE_PREFIXES", RU_DATE_PREFIXES);
        unique("EN_DATE_PREFIXES", EN_DATE_PREFIXES);
        unique("RU_PRIORITY_WORDS", RU_PRIORITY_WORDS);
        unique("EN_PRIORITY_WORDS", EN_PRIORITY_WORDS);
        unique("RU_KEYWORDS", RU_KEYWORDS);
        unique("EN_KEYWORDS", EN_KEYWORDS);
        unique("RU_HALF_DAYS", RU_HALF_DAYS);
        unique("EN_HALF_DAYS", EN_HALF_DAYS);

        unique_words("RU_REMOVES", RU_REMOVES);
        unique_words("EN_REMOVES", EN_REMOVES);
        unique_words("RU_WITHOUTS", RU_WITHOUTS);
        unique_words("EN_WITHOUTS", EN_WITHOUTS);
        unique_words("RU_REPEAT_HEADS", RU_REPEAT_HEADS);
        unique_words("EN_REPEAT_HEADS", EN_REPEAT_HEADS);
        unique_words("RU_WORKING_WORDS", RU_WORKING_WORDS);
        unique_words("EN_WORKING_WORDS", EN_WORKING_WORDS);
        unique_words("EN_WORKDAY_NOUNS", EN_WORKDAY_NOUNS);
        unique_words("RU_HOUR_UNITS", RU_HOUR_UNITS);
        unique_words("EN_HOUR_UNITS", EN_HOUR_UNITS);
        unique_words("RU_TIME_PREFIXES", RU_TIME_PREFIXES);
        unique_words("EN_TIME_PREFIXES", EN_TIME_PREFIXES);
        unique_words("RU_NEGATIONS", RU_NEGATIONS);
        unique_words("EN_NEGATIONS", EN_NEGATIONS);
        unique_words("RU_CONJUNCTIONS", RU_CONJUNCTIONS);
        unique_words("EN_CONJUNCTIONS", EN_CONJUNCTIONS);
        unique_words("RU_BACK_TO_WORK", RU_BACK_TO_WORK);
        unique_words("RU_LEAD_HEADS", RU_LEAD_HEADS);
        unique_words("EN_LEAD_TAILS", EN_LEAD_TAILS);
        unique_words("RU_REMIND_VERBS", RU_REMIND_VERBS);
        unique_words("EN_REMIND_VERBS", EN_REMIND_VERBS);
    }
}
