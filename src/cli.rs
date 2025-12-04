use chrono::NaiveDate;
use clap::Parser;
use std::path::PathBuf;

use crate::format::OutputFormat;

#[derive(Parser)]
#[command(name = "markdown-extract")]
#[command(about = "Extract tasks from markdown files with org-mode timestamps")]
#[command(version)]
pub struct Cli {
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,

    #[arg(long, default_value = "*.md")]
    pub glob: String,

    #[arg(long, default_value = "json", value_parser = parse_format)]
    pub format: OutputFormat,

    #[arg(long)]
    pub output: Option<PathBuf>,

    #[arg(long, default_value = "ru,en")]
    pub locale: String,

    #[arg(long, default_value = "day", value_parser = ["day", "week"], conflicts_with = "tasks")]
    pub agenda: String,

    #[arg(long)]
    pub tasks: bool,

    #[arg(long, value_parser = validate_date)]
    pub date: Option<String>,

    #[arg(long, value_parser = validate_date)]
    pub from: Option<String>,

    #[arg(long, value_parser = validate_date)]
    pub to: Option<String>,

    #[arg(long, default_value = "Europe/Moscow", value_parser = validate_timezone)]
    pub tz: String,

    #[arg(long, value_parser = validate_date)]
    pub current_date: Option<String>,

    #[arg(long, default_value = "14")]
    pub deadline_warning_days: i64,
}

impl Cli {
    pub fn get_agenda_mode(&self) -> &str {
        if self.tasks {
            "tasks"
        } else {
            &self.agenda
        }
    }
}

fn parse_format(s: &str) -> Result<OutputFormat, String> {
    s.parse()
}

fn validate_date(s: &str) -> Result<String, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|_| s.to_string())
        .map_err(|e| format!("Invalid date '{s}': {e}. Use YYYY-MM-DD format"))
}

fn validate_timezone(s: &str) -> Result<String, String> {
    s.parse::<chrono_tz::Tz>()
        .map(|_| s.to_string())
        .map_err(|_| format!("Invalid timezone '{s}'. Use IANA timezone names (e.g., 'Europe/Moscow', 'UTC')"))
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
}
