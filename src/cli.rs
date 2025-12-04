use chrono::NaiveDate;
use clap::Parser;
use std::path::PathBuf;

/// CLI arguments for markdown-extract
#[derive(Parser)]
#[command(name = "markdown-extract")]
#[command(about = "Extract tasks from markdown files with org-mode timestamps")]
#[command(version)]
pub struct Cli {
    /// Directory to search for markdown files
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,

    /// Glob pattern for filtering files
    #[arg(long, default_value = "*.md")]
    pub glob: String,

    /// Output format: json, md, html
    #[arg(long, default_value = "json", value_parser = ["json", "md", "html"])]
    pub format: String,

    /// Output file path (stdout if not specified)
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Comma-separated locales for weekday names (e.g., "ru,en")
    #[arg(long, default_value = "ru,en")]
    pub locale: String,

    /// Agenda mode: day, week, tasks
    #[arg(long, default_value = "day", value_parser = ["day", "week", "tasks"])]
    pub agenda: String,

    /// Date for 'day' mode (YYYY-MM-DD format)
    #[arg(long, value_parser = validate_date)]
    pub date: Option<String>,

    /// Start date for 'week' mode (YYYY-MM-DD format)
    #[arg(long, value_parser = validate_date)]
    pub from: Option<String>,

    /// End date for 'week' mode (YYYY-MM-DD format)
    #[arg(long, value_parser = validate_date)]
    pub to: Option<String>,

    /// Timezone for date calculations (IANA timezone, e.g., "Europe/Moscow")
    #[arg(long, default_value = "Europe/Moscow")]
    pub tz: String,
}

/// Validate date format (YYYY-MM-DD)
fn validate_date(s: &str) -> Result<String, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|_| s.to_string())
        .map_err(|e| format!("Invalid date '{s}': {e}. Use YYYY-MM-DD format"))
}

/// Get weekday name mappings for the specified locales
///
/// # Arguments
/// * `locale` - Comma-separated locale codes (e.g., "ru,en")
///
/// # Returns
/// Vector of (localized_name, english_name) tuples
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
