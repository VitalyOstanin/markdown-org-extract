//! Org-mode timestamp parsing and repeater logic.
//!
//! Submodule layout:
//! - `extract` — pull timestamp / CREATED strings out of free-form text.
//! - `parser`  — parse a single org-style timestamp into [`ParsedTimestamp`].
//! - `repeater` — repeater grammar and occurrence math (`+1d`, `++2w`, `.+1m`, `+1wd`).
//! - `weekdays` — localized weekday name normalization (RU → EN).

mod extract;
mod parser;
mod repeater;
mod weekdays;

pub use extract::{
    extract_created_normalized, extract_timestamp_normalized, parse_timestamp_fields,
};
pub use parser::{parse_org_timestamp, ParsedTimestamp};
pub use repeater::{closest_date, DatePreference, Repeater, RepeaterUnit};
pub(crate) use weekdays::normalize_weekdays;
