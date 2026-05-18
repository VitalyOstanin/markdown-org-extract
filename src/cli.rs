use chrono::NaiveDate;
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

use crate::format::OutputFormat;

/// Color output mode for diagnostics. Mirrors the `--color auto|always|never`
/// convention used by `cargo`, `rg`, and other Rust-ecosystem CLIs.
#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum ColorMode {
    /// Enable color when stderr is a TTY and `NO_COLOR` is not set
    Auto,
    /// Force-enable color even when stderr is piped
    Always,
    /// Never emit color
    Never,
}

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
    /// Flat task list (no date windowing). Equivalent to the legacy `--tasks` flag.
    Tasks,
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

    /// Output format. `md` is accepted as an alias for `markdown`.
    #[arg(long, default_value = "json", value_enum)]
    pub format: OutputFormat,

    /// Write output to file instead of stdout. The path must reside in an
    /// existing directory and must not be a symlink. Use `-` for stdout.
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
    #[arg(long, value_parser = validate_date, conflicts_with = "tasks")]
    pub from: Option<String>,

    /// Range end for `--agenda week/month` (YYYY-MM-DD)
    #[arg(long, value_parser = validate_date, conflicts_with = "tasks")]
    pub to: Option<String>,

    /// IANA timezone for "today" determination (e.g. `Europe/Moscow`, `UTC`)
    #[arg(long, default_value = "Europe/Moscow", value_parser = validate_timezone)]
    pub tz: String,

    /// Override "today" for reproducible runs / tests (YYYY-MM-DD)
    #[arg(long, value_parser = validate_date)]
    pub current_date: Option<String>,

    /// Print holidays for the given year (1900..=2100) and exit.
    /// Short-circuits scanning; cannot be combined with scan/agenda flags.
    #[arg(
        long,
        value_parser = validate_year,
        conflicts_with_all = ["dir", "glob", "format", "output", "tasks", "agenda", "date", "from", "to", "absolute_paths", "max_tasks"]
    )]
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

    /// Suppress all diagnostics (warnings and the per-run processing summary
    /// of skipped/failed files). Hard errors still go to stderr.
    #[arg(long, short = 'q', conflicts_with = "verbose")]
    pub quiet: bool,

    /// Disable ANSI color codes in diagnostic output. The `NO_COLOR` env var
    /// has the same effect (see <https://no-color.org>). Shortcut for
    /// `--color never`; cannot be combined with `--color`.
    #[arg(long, conflicts_with = "color")]
    pub no_color: bool,

    /// Color output mode for diagnostics: `auto` (default), `always`, `never`.
    /// `auto` enables color only when stderr is a TTY and `NO_COLOR` is not set.
    #[arg(long, value_enum, default_value = "auto")]
    pub color: ColorMode,
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
    /// Precedence (highest first):
    /// 1. `--color always` -> always on (overrides NO_COLOR and TTY check)
    /// 2. `--color never` or `--no-color` -> always off
    /// 3. `NO_COLOR` env var present (per <https://no-color.org>) -> off
    /// 4. `--color auto` (default) -> on only when stderr is a TTY
    pub fn use_color(&self) -> bool {
        match self.color {
            ColorMode::Always => return true,
            ColorMode::Never => return false,
            ColorMode::Auto => {}
        }
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

    pub fn agenda_scope(&self) -> crate::agenda::AgendaScope {
        use crate::agenda::AgendaScope;
        // `--tasks` and `--agenda tasks` both produce a flat task list. The
        // legacy bool flag wins when present so existing scripts keep working;
        // otherwise the AgendaMode value decides.
        if self.tasks {
            return AgendaScope::Tasks;
        }
        match self.agenda {
            AgendaMode::Day => AgendaScope::Day,
            AgendaMode::Week => AgendaScope::Week,
            AgendaMode::Month => AgendaScope::Month,
            AgendaMode::Tasks => AgendaScope::Tasks,
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

/// Lower/upper bounds for any user-supplied date in this CLI. Matches the
/// `--holidays` year range so an extreme year cannot, for example, push a
/// repeater into thousands of `+1y` iterations.
const DATE_YEAR_MIN: i32 = 1900;
const DATE_YEAR_MAX: i32 = 2100;

// Validator messages start lowercase and do not re-echo the argument name or
// value. clap already prints `error: invalid value '<v>' for '--<arg> ...':`
// before our text, so re-prefixing with "Invalid date '<v>':" produces a
// stuttering message ("invalid value ...: Invalid date ...: ..."). Match clap's
// own style: lowercase, no leading capital, no duplicated value.

fn validate_date(s: &str) -> Result<String, String> {
    use chrono::Datelike;
    let parsed = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| format!("{e}; use YYYY-MM-DD format"))?;
    let year = parsed.year();
    if !(DATE_YEAR_MIN..=DATE_YEAR_MAX).contains(&year) {
        return Err(format!(
            "year must be between {DATE_YEAR_MIN} and {DATE_YEAR_MAX}"
        ));
    }
    Ok(s.to_string())
}

fn validate_year(s: &str) -> Result<i32, String> {
    let year: i32 = s.parse().map_err(|_| "must be a number".to_string())?;

    if !(DATE_YEAR_MIN..=DATE_YEAR_MAX).contains(&year) {
        return Err(format!(
            "must be between {DATE_YEAR_MIN} and {DATE_YEAR_MAX}"
        ));
    }

    Ok(year)
}

const MAX_TASKS_ALLOWED: usize = 10_000_000;

fn validate_max_tasks(s: &str) -> Result<usize, String> {
    use std::num::IntErrorKind;
    let n: usize = match s.parse() {
        Ok(n) => n,
        Err(e) => {
            // Distinguish "number is too big to fit in usize" (PosOverflow) from
            // "not a number at all" (InvalidDigit/Empty/etc). On 32-bit usize
            // overflows at 4_294_967_295, which is still above MAX_TASKS_ALLOWED,
            // so a parse-time overflow is just an over-the-cap value — say so.
            return Err(match e.kind() {
                IntErrorKind::PosOverflow => {
                    format!("out of range, must be at most {MAX_TASKS_ALLOWED}")
                }
                _ => format!("must be a positive integer up to {MAX_TASKS_ALLOWED}"),
            });
        }
    };
    if n == 0 {
        return Err("must be at least 1".to_string());
    }
    if n > MAX_TASKS_ALLOWED {
        return Err(format!("must be at most {MAX_TASKS_ALLOWED}"));
    }
    Ok(n)
}

fn validate_timezone(s: &str) -> Result<String, String> {
    // Preserve the chrono-tz error text — it usually pinpoints the failure
    // (e.g. trailing whitespace, unknown zone name). Without it the user only
    // sees the generic IANA-hint and has to guess what went wrong.
    s.parse::<chrono_tz::Tz>()
        .map(|_| s.to_string())
        .map_err(|e| format!("{e}; use IANA timezone names (e.g. 'Europe/Moscow', 'UTC')"))
}

/// Russian weekday names mapped to their English equivalents. The same table
/// drives both `--locale ru` weekday normalization in the CLI and the
/// integration-style parser test that reproduces the production pipeline,
/// so adding or renaming an entry only requires editing it here.
pub(crate) const RU_WEEKDAY_MAPPINGS: &[(&str, &str)] = &[
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
];

/// Locales for which `get_weekday_mappings` ships a translation table.
/// `en` is recognised as a no-op (English weekday names need no mapping) so
/// the default `--locale ru,en` works without warnings.
pub(crate) const SUPPORTED_LOCALES: &[&str] = &["ru", "en"];

pub fn get_weekday_mappings(locale: &str) -> Vec<(&'static str, &'static str)> {
    let locales: Vec<&str> = locale.split(',').map(|s| s.trim()).collect();
    let mut mappings = Vec::new();

    for loc in locales {
        match loc {
            "ru" => mappings.extend_from_slice(RU_WEEKDAY_MAPPINGS),
            "en" => {} // English: no translation table needed, recognised silently.
            "" => {}   // tolerate `--locale ru,` / leading commas without warning
            other => {
                tracing::warn!(
                    locale = %other,
                    supported = ?SUPPORTED_LOCALES,
                    "unknown --locale entry ignored; no weekday mappings will be added for it"
                );
            }
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
    fn get_weekday_mappings_ru_matches_static_table() {
        // The `--locale ru` output must be exactly the static table — there
        // is no other source of truth. Catches a future regression where
        // someone edits the table but forgets to update consumers, or vice
        // versa (the parser test imports the same constant, so a missing
        // entry would fail in both places at once).
        let mappings = get_weekday_mappings("ru");
        assert_eq!(mappings.as_slice(), RU_WEEKDAY_MAPPINGS);
    }

    #[test]
    fn test_get_weekday_mappings_empty() {
        let mappings = get_weekday_mappings("en");
        assert!(mappings.is_empty());
    }

    #[test]
    fn validate_max_tasks_accepts_valid() {
        assert_eq!(validate_max_tasks("1"), Ok(1));
        assert_eq!(validate_max_tasks("10000000"), Ok(10_000_000));
    }

    #[test]
    fn validate_max_tasks_rejects_zero() {
        let err = validate_max_tasks("0").unwrap_err();
        assert!(err.contains("at least 1"), "got: {err}");
    }

    #[test]
    fn validate_max_tasks_rejects_above_cap_with_cap_message() {
        // In-range usize but above MAX_TASKS_ALLOWED.
        let err = validate_max_tasks("20000000").unwrap_err();
        assert!(err.contains("at most 10000000"), "got: {err}");
    }

    #[test]
    fn validate_max_tasks_rejects_non_number_with_explicit_cap_hint() {
        // Non-numeric input must still mention the cap so the user knows what
        // range is acceptable, not just "must be a positive integer".
        let err = validate_max_tasks("abc").unwrap_err();
        assert!(err.contains("positive integer"), "got: {err}");
        assert!(
            err.contains("10000000"),
            "expected cap in message, got: {err}"
        );
    }

    #[test]
    fn validate_date_accepts_year_at_lower_bound() {
        assert!(validate_date("1900-01-01").is_ok());
    }

    #[test]
    fn validate_date_accepts_year_at_upper_bound() {
        assert!(validate_date("2100-12-31").is_ok());
    }

    #[test]
    fn validate_date_rejects_year_below_lower_bound() {
        let err = validate_date("1899-12-31").unwrap_err();
        assert!(err.contains("1900"), "got: {err}");
        assert!(err.contains("2100"), "got: {err}");
    }

    #[test]
    fn validate_date_rejects_year_above_upper_bound() {
        let err = validate_date("2101-01-01").unwrap_err();
        assert!(err.contains("1900"), "got: {err}");
        assert!(err.contains("2100"), "got: {err}");
    }

    #[test]
    fn validate_timezone_accepts_iana() {
        assert!(validate_timezone("Europe/Moscow").is_ok());
        assert!(validate_timezone("UTC").is_ok());
    }

    #[test]
    fn validate_timezone_propagates_underlying_error_and_hint() {
        // The chrono-tz Display ("failed to parse timezone") must come through
        // verbatim and the IANA hint must follow it. We deliberately do NOT
        // echo the input -- clap prefixes its own `invalid value '<v>' for ...`
        // before our text, so echoing here would stutter.
        let err = validate_timezone("Not/A_Zone").unwrap_err();
        assert!(
            err.contains("failed to parse timezone"),
            "expected chrono-tz reason, got: {err}"
        );
        assert!(err.contains("IANA"), "expected IANA hint, got: {err}");
    }

    #[test]
    fn validate_date_still_rejects_malformed() {
        // The bounds check must not mask the format check — non-YYYY-MM-DD
        // input keeps the existing "Use YYYY-MM-DD format" hint.
        let err = validate_date("not-a-date").unwrap_err();
        assert!(err.contains("YYYY-MM-DD"), "got: {err}");
    }

    #[test]
    fn validate_max_tasks_distinguishes_overflow_from_garbage() {
        // A number that overflows usize even on 64-bit must produce a different,
        // more informative message than non-numeric garbage. This proves the
        // IntErrorKind::PosOverflow branch is reachable from CLI input.
        let huge = "99999999999999999999999999999999999";
        let err = validate_max_tasks(huge).unwrap_err();
        assert!(
            err.contains("out of range"),
            "expected 'out of range' wording for overflow, got: {err}"
        );
        assert!(
            err.contains("10000000"),
            "expected cap in message, got: {err}"
        );
    }
}
