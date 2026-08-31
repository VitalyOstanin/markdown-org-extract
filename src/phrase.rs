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
//! | 6 | heading  | everything the rules did not consume                                       |
//!
//! Two costs are accepted deliberately, both visible on the screen the fields
//! are shown on and correctable there:
//!
//! - a lead-in verb (`напомни`, `создай`, `remind me to`, `add a task`) is
//!   eaten at the start of a phrase, so an entry that really begins with one
//!   loses that word;
//! - `в N` / `at N` with nothing after the number reads as an hour, so
//!   "в 5 минутах ходьбы" sets 05:00 and leaves "минутах ходьбы" in the
//!   heading.
//!
//! Nothing here removes a field: "не завтра" sets no date and clears none —
//! the words land in the heading like any other text the rules do not know.

use chrono::{Datelike, Days, Months, NaiveDate, NaiveTime, Timelike, Weekday};
use serde::ser::{Serialize, SerializeStruct, Serializer};

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
}

/// Refine `entry` with one more `phrase`.
///
/// A field the phrase names replaces what was there; a field it does not name
/// keeps its value; text the rules do not consume is appended to the heading,
/// separated by a space. On the first phrase, where the heading is empty, that
/// is exactly "what is left over becomes the heading".
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
}

fn apply(entry: &mut PhraseEntry, effect: Effect) {
    match effect {
        Effect::Date { date, planning } => {
            entry.date = Some(date);
            entry.planning = Some(planning);
        }
        Effect::Time(time) => entry.time = Some(time),
        Effect::Repeat(repeater) => entry.repeater = Some(repeater),
        Effect::Prio(priority) => entry.priority = Some(priority),
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

fn is_negation(key: &str, langs: Languages) -> bool {
    (langs.ru && key == "не") || (langs.en && key == "not")
}

// --- lead-in verbs ---------------------------------------------------------

/// The closed list of ways a person addresses the application before saying
/// what the entry is. Longest first: the first sequence that matches wins.
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

/// How many tokens the lead-in takes. Only ever matched at the start of a
/// phrase: the same verb further along is part of what was said.
fn lead_in_len(tokens: &[Token<'_>], langs: Languages) -> usize {
    let mut lists: Vec<&&[&str]> = Vec::new();
    if langs.ru {
        lists.extend(RU_LEAD_INS.iter());
    }
    if langs.en {
        lists.extend(EN_LEAD_INS.iter());
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

/// A preposition in front of a date, and what it says about the date.
fn date_prefix(key: &str, langs: Languages) -> Option<PlanningKind> {
    if langs.ru {
        match key {
            "в" | "во" | "на" => return Some(PlanningKind::Scheduled),
            "к" | "ко" | "до" | "срок" | "дедлайн" => {
                return Some(PlanningKind::Deadline)
            }
            _ => {}
        }
    }
    if langs.en {
        match key {
            "on" | "at" => return Some(PlanningKind::Scheduled),
            "by" | "due" | "before" | "until" | "deadline" => return Some(PlanningKind::Deadline),
            _ => {}
        }
    }
    None
}

fn match_date(
    tokens: &[Token<'_>],
    i: usize,
    langs: Languages,
    today: NaiveDate,
) -> Option<(usize, NaiveDate)> {
    let key = tokens.get(i)?.key.as_str();

    if langs.ru {
        match key {
            "сегодня" => return Some((1, today)),
            "завтра" => return Some((1, today.checked_add_days(Days::new(1))?)),
            "послезавтра" => return Some((1, today.checked_add_days(Days::new(2))?)),
            _ => {}
        }
    }
    if langs.en {
        match key {
            "today" => return Some((1, today)),
            "tomorrow" => return Some((1, today.checked_add_days(Days::new(1))?)),
            "day" if word_at(tokens, i + 1) == "after" && word_at(tokens, i + 2) == "tomorrow" => {
                return Some((3, today.checked_add_days(Days::new(2))?))
            }
            _ => {}
        }
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

fn match_time(tokens: &[Token<'_>], i: usize, langs: Languages) -> Option<(usize, NaiveTime)> {
    let mut j = i;
    let mut prefixed = false;
    if let Some(token) = tokens.get(j) {
        let is_prefix = (langs.ru && matches!(token.key.as_str(), "в" | "во"))
            || (langs.en && token.key == "at");
        if is_prefix {
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
        let is_unit = (langs.ru && matches!(next.key.as_str(), "час" | "часа" | "часов"))
            || (langs.en && matches!(next.key.as_str(), "oclock" | "o'clock"));
        if is_unit {
            named_unit = true;
            k += 1;
        }
    }
    let mut qualified = false;
    if let Some(next) = tokens.get(k) {
        let qualifier = match next.key.as_str() {
            "дня" | "вечера" if langs.ru => Some(true),
            "утра" | "ночи" if langs.ru => Some(false),
            "pm" if langs.en => Some(true),
            "am" if langs.en => Some(false),
            _ => None,
        };
        if let Some(afternoon) = qualifier {
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

fn match_repeater(tokens: &[Token<'_>], i: usize, langs: Languages) -> Option<(usize, Repeater)> {
    let key = tokens.get(i)?.key.as_str();

    if let Some(unit) = single_word_repeater(key, langs) {
        return Some((1, every(1, unit)));
    }

    let heads_a_repeat = (langs.ru
        && matches!(
            key,
            "каждый" | "каждая" | "каждую" | "каждое" | "каждые" | "каждых"
        ))
        || (langs.en && key == "every");
    if !heads_a_repeat {
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

    // "каждый рабочий день" / "every working day" — the working-day repeater
    // the timestamp grammar spells `+1wd`.
    if let Some(token) = tokens.get(j) {
        let says_working = (langs.ru
            && matches!(token.key.as_str(), "рабочий" | "рабочих" | "рабочие"))
            || (langs.en && matches!(token.key.as_str(), "working" | "work"));
        if says_working {
            let unit = span_of(&tokens.get(j + 1)?.key, langs)?;
            if unit != Span::Day {
                return None;
            }
            return Some((j + 2 - i, every(count, RepeaterUnit::Workday)));
        }
        if langs.en && matches!(token.key.as_str(), "workday" | "workdays") {
            return Some((j + 1 - i, every(count, RepeaterUnit::Workday)));
        }
    }

    let span = span_of(&tokens.get(j)?.key, langs)?;
    Some((j + 1 - i, every(count, span.unit())))
}

fn single_word_repeater(key: &str, langs: Languages) -> Option<RepeaterUnit> {
    if langs.ru {
        match key {
            "ежедневно" => return Some(RepeaterUnit::Day),
            "еженедельно" => return Some(RepeaterUnit::Week),
            "ежемесячно" => return Some(RepeaterUnit::Month),
            "ежегодно" => return Some(RepeaterUnit::Year),
            _ => {}
        }
    }
    if langs.en {
        match key {
            "daily" => return Some(RepeaterUnit::Day),
            "weekly" => return Some(RepeaterUnit::Week),
            "monthly" => return Some(RepeaterUnit::Month),
            "yearly" | "annually" => return Some(RepeaterUnit::Year),
            _ => {}
        }
    }
    None
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

fn match_priority(tokens: &[Token<'_>], i: usize, langs: Languages) -> Option<(usize, Priority)> {
    let key = tokens.get(i)?.key.as_str();

    if langs.ru {
        if key == "очень" && matches!(word_at(tokens, i + 1), "важно" | "важное") {
            return Some((2, Priority::A));
        }
        match key {
            "срочно" | "срочное" | "критично" => {
                return Some((1, Priority::A))
            }
            "важно" | "важное" => return Some((1, Priority::B)),
            "приоритет" => return named_priority(tokens, i),
            _ => {}
        }
    }
    if langs.en {
        match key {
            "urgent" | "asap" | "critical" => return Some((1, Priority::A)),
            "important" => return Some((1, Priority::B)),
            "priority" => return named_priority(tokens, i),
            _ => {}
        }
    }
    None
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

fn span_of(key: &str, langs: Languages) -> Option<Span> {
    if langs.ru {
        match key {
            "день" | "дня" | "дней" | "дни" => return Some(Span::Day),
            "неделя" | "неделю" | "недели" | "недель" => {
                return Some(Span::Week)
            }
            "месяц" | "месяца" | "месяцев" => return Some(Span::Month),
            "год" | "года" | "лет" => return Some(Span::Year),
            _ => {}
        }
    }
    if langs.en {
        match key {
            "day" | "days" => return Some(Span::Day),
            "week" | "weeks" => return Some(Span::Week),
            "month" | "months" => return Some(Span::Month),
            "year" | "years" => return Some(Span::Year),
            _ => {}
        }
    }
    None
}

fn ru_numeral(key: &str) -> Option<u32> {
    Some(match key {
        "два" | "две" => 2,
        "три" => 3,
        "четыре" => 4,
        "пять" => 5,
        "шесть" => 6,
        "семь" => 7,
        "восемь" => 8,
        "девять" => 9,
        "десять" => 10,
        _ => return None,
    })
}

/// The hours as they are said: "в час дня", "в три часа".
fn ru_hour_word(key: &str) -> Option<u32> {
    Some(match key {
        "час" => 1,
        "два" | "две" => 2,
        "три" => 3,
        "четыре" => 4,
        "пять" => 5,
        "шесть" => 6,
        "семь" => 7,
        "восемь" => 8,
        "девять" => 9,
        "десять" => 10,
        "одиннадцать" => 11,
        "двенадцать" => 12,
        _ => return None,
    })
}

/// Weekday names in the cases they are said in: nominative, accusative after
/// "в", dative after "к", genitive after "до", and the abbreviations the
/// timestamp grammar already knows.
fn weekday_of(key: &str, langs: Languages) -> Option<Weekday> {
    if langs.ru {
        match key {
            "понедельник" | "понедельника" | "понедельнику" | "пн" => {
                return Some(Weekday::Mon)
            }
            "вторник" | "вторника" | "вторнику" | "вт" => {
                return Some(Weekday::Tue)
            }
            "среда" | "среду" | "среды" | "среде" | "ср" => {
                return Some(Weekday::Wed)
            }
            "четверг" | "четверга" | "четвергу" | "чт" => {
                return Some(Weekday::Thu)
            }
            "пятница" | "пятницу" | "пятницы" | "пятнице" | "пт" => {
                return Some(Weekday::Fri)
            }
            "суббота" | "субботу" | "субботы" | "субботе" | "сб" => {
                return Some(Weekday::Sat)
            }
            "воскресенье" | "воскресенья" | "воскресенью" | "вс" => {
                return Some(Weekday::Sun)
            }
            _ => {}
        }
    }
    if langs.en {
        match key {
            "monday" | "mon" => return Some(Weekday::Mon),
            "tuesday" | "tue" | "tues" => return Some(Weekday::Tue),
            "wednesday" | "wed" => return Some(Weekday::Wed),
            "thursday" | "thu" | "thurs" => return Some(Weekday::Thu),
            "friday" | "fri" => return Some(Weekday::Fri),
            "saturday" | "sat" => return Some(Weekday::Sat),
            "sunday" | "sun" => return Some(Weekday::Sun),
            _ => {}
        }
    }
    None
}

fn month_of(key: &str, langs: Languages) -> Option<(u32, Lang)> {
    if langs.ru {
        let month = match key {
            "январь" | "января" => 1,
            "февраль" | "февраля" => 2,
            "март" | "марта" => 3,
            "апрель" | "апреля" => 4,
            "май" | "мая" => 5,
            "июнь" | "июня" => 6,
            "июль" | "июля" => 7,
            "август" | "августа" => 8,
            "сентябрь" | "сентября" => 9,
            "октябрь" | "октября" => 10,
            "ноябрь" | "ноября" => 11,
            "декабрь" | "декабря" => 12,
            _ => 0,
        };
        if month != 0 {
            return Some((month, Lang::Ru));
        }
    }
    if langs.en {
        let month = match key {
            "january" | "jan" => 1,
            "february" | "feb" => 2,
            "march" | "mar" => 3,
            "april" | "apr" => 4,
            "may" => 5,
            "june" | "jun" => 6,
            "july" | "jul" => 7,
            "august" | "aug" => 8,
            "september" | "sep" | "sept" => 9,
            "october" | "oct" => 10,
            "november" | "nov" => 11,
            "december" | "dec" => 12,
            _ => 0,
        };
        if month != 0 {
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
/// heading; dates are `YYYY-MM-DD`, times are `HH:MM`, and the repeater is
/// the org-mode spelling (`+1w`). Per ADR-0015 a consumer ignores keys it
/// does not know.
impl Serialize for PhraseEntry {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        let mut state = ser.serialize_struct("PhraseEntry", 6)?;
        state.serialize_field("heading", &self.heading)?;
        state.serialize_field("priority", &self.priority)?;
        state.serialize_field("planning", &self.planning.map(PlanningKind::as_str))?;
        state.serialize_field("date", &self.date.map(|date| date.to_string()))?;
        state.serialize_field(
            "time",
            &self.time.map(|time| time.format("%H:%M").to_string()),
        )?;
        state.serialize_field("repeater", &self.repeater.as_ref().map(Repeater::canonical))?;
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
}
