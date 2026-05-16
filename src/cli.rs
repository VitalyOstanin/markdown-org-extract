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

    /// Maximum number of tasks to extract before stopping (1..=10_000_000).
    /// Acts as both a per-file cap and a global cap during scanning.
    #[arg(long, default_value_t = crate::types::DEFAULT_MAX_TASKS, value_parser = validate_max_tasks)]
    pub max_tasks: usize,

    /// Increase logging verbosity. Repeat for more (-v = info, -vv = debug, -vvv = trace).
    #[arg(long, short = 'v', action = clap::ArgAction::Count, conflicts_with = "quiet")]
    pub verbose: u8,

    /// Suppress all diagnostic output except hard errors.
    #[arg(long, short = 'q', conflicts_with = "verbose")]
    pub quiet: bool,

    /// Disable ANSI color codes in diagnostic output. Honors `NO_COLOR` env var as well.
    #[arg(long)]
    pub no_color: bool,
}

impl Cli {
    /// Resolve the diagnostic log level from `--verbose` / `--quiet`.
    /// Default is `warn`: warnings and errors are visible, info-level chatter is not.
    pub fn log_level(&self) -> tracing::Level {
        if self.quiet {
            tracing::Level::ERROR
        } else {
            match self.verbose {
                0 => tracing::Level::WARN,
                1 => tracing::Level::INFO,
                2 => tracing::Level::DEBUG,
                _ => tracing::Level::TRACE,
            }
        }
    }

    /// Whether ANSI color should be used in the log output.
    ///
    /// Disabled when `--no-color` is set, when `NO_COLOR` is present in the
    /// environment (per <https://no-color.org>), or when stderr is not a TTY.
    pub fn use_color(&self) -> bool {
        if self.no_color {
            return false;
        }
        if std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        // Conservative default: assume no TTY unless we can prove otherwise.
        // Keeps tests deterministic (assert_cmd pipes stderr).
        use std::io::IsTerminal;
        std::io::stderr().is_terminal()
    }

    pub fn get_agenda_mode(&self) -> &str {
        if self.tasks {
            "tasks"
        } else {
            self.agenda.as_str()
        }
    }

    /// Initialize the global tracing subscriber from CLI flags.
    ///
    /// Idempotent in practice: callers invoke this once at startup. If
    /// `try_init` finds an existing global subscriber (e.g. in tests), the
    /// error is swallowed — tests don't need diagnostic output.
    pub fn init_tracing(&self) {
        use tracing_subscriber::EnvFilter;
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new(self.log_level().to_string().to_lowercase()));
        let _ = tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_target(false)
            .without_time()
            .with_ansi(self.use_color())
            .with_env_filter(env_filter)
            .try_init();
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

fn validate_max_tasks(s: &str) -> Result<usize, String> {
    let n: usize = s
        .parse()
        .map_err(|_| format!("Invalid --max-tasks '{s}': must be a positive integer"))?;
    const MAX_ALLOWED: usize = 10_000_000;
    if n == 0 {
        return Err("Invalid --max-tasks 0: must be at least 1".to_string());
    }
    if n > MAX_ALLOWED {
        return Err(format!(
            "Invalid --max-tasks '{s}': must be at most {MAX_ALLOWED}"
        ));
    }
    Ok(n)
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
