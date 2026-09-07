//! How far ahead of an occurrence a reminder is due (ADR-0041).
//!
//! A client decides when it reminds; an entry can say how long before its
//! occurrence it wants to be reminded, and says it with the `REMINDER` key of
//! the `org-properties` block of ADR-0020. The value is a count and a unit —
//! `30min`, `1h`, `1m` — and this module is the one place that reads it.
//!
//! The unit letters are the repeater's, because a file must not spell one
//! letter two ways: `m` is a calendar month there and here, which is why
//! minutes are written `min`. Nothing is converted on the way in. A month has
//! no fixed length, and the subtraction belongs to the client that knows the
//! occurrence it is counting back from.

use serde::{Deserialize, Serialize};

/// Property key holding an entry's own reminder lead time (ADR-0041).
pub const REMINDER_KEY: &str = "REMINDER";

/// How far ahead of an occurrence a reminder is due, as the file writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReminderLead {
    /// How many units ahead. Zero is a reminder at the occurrence itself.
    pub value: u32,
    /// The unit those are counted in.
    pub unit: ReminderUnit,
}

/// The unit a lead time is counted in (ADR-0041).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReminderUnit {
    /// `min` — minutes. Spelled with three letters because `m` is a month.
    #[serde(rename = "min")]
    Minute,
    /// `h` — hours.
    #[serde(rename = "h")]
    Hour,
    /// `d` — days.
    #[serde(rename = "d")]
    Day,
    /// `w` — weeks.
    #[serde(rename = "w")]
    Week,
    /// `m` — calendar months.
    #[serde(rename = "m")]
    Month,
    /// `y` — calendar years.
    #[serde(rename = "y")]
    Year,
}

impl ReminderUnit {
    /// The suffix the file writes this unit with (`min`, `h`, `d`, `w`, `m`,
    /// `y`). Round-trips with [`parse_reminder_lead`].
    pub fn suffix(&self) -> &'static str {
        match self {
            ReminderUnit::Minute => "min",
            ReminderUnit::Hour => "h",
            ReminderUnit::Day => "d",
            ReminderUnit::Week => "w",
            ReminderUnit::Month => "m",
            ReminderUnit::Year => "y",
        }
    }
}

impl ReminderLead {
    /// The value as a file writes it: the count and the unit's suffix
    /// (`30min`, `1m`).
    pub fn canonical(&self) -> String {
        format!("{}{}", self.value, self.unit.suffix())
    }
}

/// Read a `REMINDER` value, or refuse it.
///
/// The value is one count and one unit, in that order, with at most spaces
/// between them: `30min`, `1 h`, `1m`. Anything else is refused rather than
/// guessed at, and the caller reports it — a lead time that was almost
/// understood is worse than none, because the entry would be reminded about
/// at a time nobody wrote.
///
/// The units are spelled the way the repeater spells them, in lower case
/// only. `30M` is refused rather than read as thirty months: it is what
/// someone means minutes by, and a file that writes it should hear about it.
pub fn parse_reminder_lead(raw: &str) -> Option<ReminderLead> {
    let text = raw.trim();
    let digits_end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let digits = &text[..digits_end];
    if digits.is_empty() {
        return None;
    }
    let unit = match text[digits_end..].trim_start() {
        "min" => ReminderUnit::Minute,
        "h" => ReminderUnit::Hour,
        "d" => ReminderUnit::Day,
        "w" => ReminderUnit::Week,
        "m" => ReminderUnit::Month,
        "y" => ReminderUnit::Year,
        _ => return None,
    };
    // A count larger than the type holds is refused rather than clamped: a
    // clamped one would remind at a time the file does not say.
    let value = digits.parse::<u32>().ok()?;
    Some(ReminderLead { value, unit })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lead time a value reads as, for a test that expects it to read.
    fn lead(raw: &str) -> ReminderLead {
        parse_reminder_lead(raw).unwrap_or_else(|| panic!("value did not read: {raw:?}"))
    }

    #[test]
    fn a_lead_time_is_a_count_and_a_unit() {
        assert_eq!(
            lead("30min"),
            ReminderLead {
                value: 30,
                unit: ReminderUnit::Minute
            }
        );
        assert_eq!(lead("2h").unit, ReminderUnit::Hour);
        assert_eq!(lead("3d").unit, ReminderUnit::Day);
        assert_eq!(lead("1w").unit, ReminderUnit::Week);
        assert_eq!(lead("1m").unit, ReminderUnit::Month);
        assert_eq!(lead("1y").unit, ReminderUnit::Year);
    }

    #[test]
    fn a_month_is_m_and_a_minute_is_min() {
        // The one thing this format cannot afford to get wrong: `m` here says
        // the same as `+1m` in a repeater does.
        assert_eq!(lead("1m").unit, ReminderUnit::Month);
        assert_eq!(lead("1min").unit, ReminderUnit::Minute);
    }

    #[test]
    fn spaces_around_the_value_and_before_the_unit_are_read() {
        assert_eq!(lead("  30 min  "), lead("30min"));
        assert_eq!(lead("1 m"), lead("1m"));
    }

    #[test]
    fn a_reminder_at_the_occurrence_is_zero_of_a_unit() {
        assert_eq!(lead("0min").value, 0);
    }

    #[test]
    fn a_value_that_is_not_one_count_and_one_unit_is_refused() {
        for raw in [
            "",           // nothing written
            "30",         // no unit
            "min",        // no count
            "-30min",     // a direction the key does not have
            "+30min",     // the repeater's sign, which says nothing here
            "1h30min",    // two counts, which this value does not carry
            "30 minutes", // a unit spelled out
            "30min soon", // a unit followed by anything else
            "30MIN",      // upper case, which someone means minutes by
            "1M",         // upper case, which someone means minutes by
            "30wd",       // the repeater's working days, which do not count ahead
        ] {
            assert!(
                parse_reminder_lead(raw).is_none(),
                "value should have been refused: {raw:?}"
            );
        }
    }

    #[test]
    fn a_count_too_large_to_hold_is_refused() {
        assert!(parse_reminder_lead("99999999999min").is_none());
    }

    #[test]
    fn a_lead_time_is_written_back_the_way_it_was_read() {
        for raw in ["30min", "2h", "3d", "1w", "1m", "1y", "0min"] {
            assert_eq!(lead(raw).canonical(), raw);
        }
    }

    #[test]
    fn the_json_unit_is_the_suffix_the_file_writes() {
        let json = serde_json::to_string(&lead("30min")).unwrap();
        assert_eq!(json, r#"{"value":30,"unit":"min"}"#);
        let month = serde_json::to_string(&lead("1m")).unwrap();
        assert_eq!(month, r#"{"value":1,"unit":"m"}"#);
    }
}
