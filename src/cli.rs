use chrono::NaiveDate;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

use crate::format::OutputFormat;

/// Agenda time scope
#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum AgendaMode {
    /// Single-day agenda for `--date` (default: today)
    Day,
    /// Week (Mon-Sun) containing `--date`, or `--from`..`--to` range
    Week,
    /// Whole month containing `--date`, or `--from`..`--to` range
    Month,
}

impl AgendaMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AgendaMode::Day => "day",
            AgendaMode::Week => "week",
            AgendaMode::Month => "month",
        }
    }
}

/// Extract org-mode tasks from a directory of markdown files
#[derive(Parser)]
#[command(name = "markdown-org-extract")]
#[command(about = "Extract tasks from markdown files with org-mode timestamps", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Root directory to scan (recursive). `.gitignore` is respected.
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,

    /// File matching pattern. Supported: `*.ext` and exact file names.
    #[arg(long, default_value = "*.md")]
    pub glob: String,

    /// Output format
    #[arg(long, default_value = "json", value_enum)]
    pub format: OutputFormat,

    /// Write output to file instead of stdout. The path must reside in an
    /// existing directory and must not be a symlink.
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Comma-separated locale list for weekday name normalization (e.g. `ru,en`).
    #[arg(long, default_value = "ru,en")]
    pub locale: String,

    /// Agenda time scope: day / week / month. Mutually exclusive with `--tasks`.
    #[arg(long, default_value = "day", value_enum, conflicts_with = "tasks")]
    pub agenda: AgendaMode,

    /// Show flat task list instead of agenda. Mutually exclusive with `--agenda`.
    #[arg(long)]
    pub tasks: bool,

    /// Target date for `--agenda day/week/month` (YYYY-MM-DD)
    #[arg(long, value_parser = validate_date)]
    pub date: Option<String>,

    /// Range start for `--agenda week/month` (YYYY-MM-DD)
    #[arg(long, value_parser = validate_date)]
    pub from: Option<String>,

    /// Range end for `--agenda week/month` (YYYY-MM-DD)
    #[arg(long, value_parser = validate_date)]
    pub to: Option<String>,

    /// IANA timezone for "today" determination (e.g. `Europe/Moscow`, `UTC`)
    #[arg(long, default_value = "Europe/Moscow", value_parser = validate_timezone)]
    pub tz: String,

    /// Override "today" for reproducible runs / tests (YYYY-MM-DD)
    #[arg(long, value_parser = validate_date)]
    pub current_date: Option<String>,

    /// Print holidays for the given year (1900..=2100) and exit
    #[arg(long, value_parser = validate_year)]
    pub holidays: Option<i32>,

    /// Emit absolute file paths in output. Default is paths relative to `--dir`.
    #[arg(long)]
    pub absolute_paths: bool,
}

impl Cli {
    pub fn get_agenda_mode(&self) -> &str {
        if self.tasks {
            "tasks"
        } else {
            self.agenda.as_str()
        }
    }
}

fn validate_date(s: &str) -> Result<String, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|_| s.to_string())
        .map_err(|e| format!("Invalid date '{s}': {e}. Use YYYY-MM-DD format"))
}

fn validate_year(s: &str) -> Result<i32, String> {
    let year: i32 = s
        .parse()
        .map_err(|_| format!("Invalid year '{s}': must be a number"))?;

    if !(1900..=2100).contains(&year) {
        return Err(format!("Invalid year '{s}': must be between 1900 and 2100"));
    }

    Ok(year)
}

fn validate_timezone(s: &str) -> Result<String, String> {
    s.parse::<chrono_tz::Tz>()
        .map(|_| s.to_string())
        .map_err(|_| {
            format!(
                "Invalid timezone '{s}'. Use IANA timezone names (e.g., 'Europe/Moscow', 'UTC')"
            )
        })
}

pub fn get_weekday_mappings(locale: &str) -> Vec<(&'static str, &'static str)> {
    let locales: Vec<&str> = locale.split(',').map(|s| s.trim()).collect();
    let mut mappings = Vec::new();

    for loc in locales {
        if loc == "ru" {
            mappings.extend_from_slice(&[
                ("Понедельник", "Monday"),
                ("Вторник", "Tuesday"),
                ("Среда", "Wednesday"),
                ("Четверг", "Thursday"),
                ("Пятница", "Friday"),
                ("Суббота", "Saturday"),
                ("Воскресенье", "Sunday"),
                ("Пн", "Mon"),
                ("Вт", "Tue"),
                ("Ср", "Wed"),
                ("Чт", "Thu"),
                ("Пт", "Fri"),
                ("Сб", "Sat"),
                ("Вс", "Sun"),
            ]);
        }
    }
    mappings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_weekday_mappings_ru() {
        let mappings = get_weekday_mappings("ru");
        assert!(mappings.contains(&("Понедельник", "Monday")));
        assert!(mappings.contains(&("Пн", "Mon")));
    }

    #[test]
    fn test_get_weekday_mappings_multiple() {
        let mappings = get_weekday_mappings("ru,en");
        assert!(mappings.contains(&("Понедельник", "Monday")));
    }

    #[test]
    fn test_get_weekday_mappings_empty() {
        let mappings = get_weekday_mappings("en");
        assert!(mappings.is_empty());
    }

    #[test]
    fn test_agenda_mode_as_str() {
        assert_eq!(AgendaMode::Day.as_str(), "day");
        assert_eq!(AgendaMode::Week.as_str(), "week");
        assert_eq!(AgendaMode::Month.as_str(), "month");
    }
}
