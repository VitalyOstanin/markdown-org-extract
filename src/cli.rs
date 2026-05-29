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

/// `long_about` text for `--help`. Kept as a `const` so the test that pins
/// example commands has a stable string to grep.
const CLI_LONG_ABOUT: &str = "\
Extract Emacs Org-mode tasks (timestamps, SCHEDULED/DEADLINE/CLOSED, CLOCK)
from markdown files. Output is JSON by default; HTML and Markdown are also
available via --format. See <https://github.com/VitalyOstanin/markdown-org-extract>.

Examples:
  Scan the current directory as JSON (default):
    markdown-org-extract

  Today's agenda for a specific vault:
    markdown-org-extract --dir ~/notes --agenda day

  Week containing a date, as Markdown:
    markdown-org-extract --agenda week --date 2026-05-25 --format markdown

  Two-week window, anchored at today:
    markdown-org-extract --agenda week --from 2026-05-21 --to 2026-06-04

  Flat task list, absolute paths, no progress noise:
    markdown-org-extract --tasks --absolute-paths --quiet

  Public RF holidays for a year:
    markdown-org-extract --holidays 2026

  Install bash completion for the current user:
    markdown-org-extract --completions bash > ~/.local/share/bash-completion/completions/markdown-org-extract

Environment:
  RUST_LOG        Diagnostic log filter (tracing syntax). Takes precedence
                  over --verbose / --quiet (e.g. RUST_LOG=error mutes -vv).
  NO_COLOR        Any value disables ANSI colour in diagnostics.
  CLICOLOR_FORCE  Non-zero value forces colour even when stderr is not a TTY.
  CLICOLOR        CLICOLOR=0 disables colour in --color auto mode.

Exit status:
  0    success (also --holidays, --completions, and a broken output pipe)
  2    usage or input-validation error
  70   internal software error (EX_SOFTWARE: regex/serializer)
  74   IO error (EX_IOERR: unreadable input, walker, --output write)
  130  aborted by SIGINT/SIGTERM (128 + signal)
";

/// Extract org-mode tasks from a directory of markdown files
#[derive(Parser)]
#[command(name = "markdown-org-extract")]
#[command(
    about = "Extract tasks from markdown files with org-mode timestamps; emits JSON by default"
)]
#[command(long_about = CLI_LONG_ABOUT)]
#[command(version)]
pub struct Cli {
    /// Root directory to scan (recursive). `.gitignore` is respected.
    #[arg(long, default_value = ".", help_heading = "Input")]
    pub dir: PathBuf,

    /// File matching pattern. Supported: `*.ext` and exact file names.
    #[arg(long, default_value = "*.md", help_heading = "Input")]
    pub glob: String,

    /// Output format. `md` is accepted as an alias for `markdown`.
    #[arg(long, default_value = "json", value_enum, help_heading = "Output")]
    pub format: OutputFormat,

    /// Write output to file instead of stdout. The path must reside in an
    /// existing directory and must not be a symlink. Use `-` for stdout.
    #[arg(long, help_heading = "Output")]
    pub output: Option<PathBuf>,

    /// Emit absolute file paths in output. Default is paths relative to `--dir`.
    /// Note: with `-v`/`-vv`/`-vvv`, diagnostic stderr also logs file paths and
    /// timestamp content; under `--absolute-paths` these stderr entries carry
    /// absolute paths too. Pipe with `--quiet` when sharing logs externally.
    #[arg(long, help_heading = "Output")]
    pub absolute_paths: bool,

    /// Comma-separated locale list for weekday name normalization (e.g. `ru,en`).
    /// Supported values: `ru`, `en`. Empty segments are tolerated
    /// (`ru,` and `,en` both parse). An unknown locale is rejected at
    /// CLI validation time with exit code 2 — `--quiet` does not mask it.
    #[arg(long, default_value = "ru,en", value_parser = validate_locale, help_heading = "Agenda")]
    pub locale: String,

    /// Agenda time scope: day / week / month. Mutually exclusive with `--tasks`.
    #[arg(
        long,
        default_value = "day",
        value_enum,
        conflicts_with = "tasks",
        help_heading = "Agenda"
    )]
    pub agenda: AgendaMode,

    /// Show flat task list instead of agenda. Mutually exclusive with `--agenda`.
    #[arg(long, help_heading = "Agenda")]
    pub tasks: bool,

    /// Also include DONE tasks in the flat list (`--tasks` / `--agenda tasks`).
    /// Off by default: the flat list is TODO-only. Has no effect in
    /// `--agenda day/week/month`, which keep their Org-faithful DONE handling.
    /// Intended for consumers that need completed tasks surfaced — e.g. a
    /// calendar sync that deletes an event once its task is marked DONE.
    #[arg(long, help_heading = "Agenda")]
    pub tasks_include_done: bool,

    /// Also include CANCELLED tasks in the flat list (`--tasks` /
    /// `--agenda tasks`). Off by default: the flat list is TODO-only.
    /// Independent of `--tasks-include-done`. Has no effect in
    /// `--agenda day/week/month`. Intended for consumers that need cancelled
    /// tasks surfaced — e.g. a calendar sync that deletes an event once its
    /// task is marked CANCELLED.
    #[arg(long, help_heading = "Agenda")]
    pub tasks_include_cancelled: bool,

    /// Window anchor for `--agenda day/week/month` (YYYY-MM-DD).
    /// In day mode the window is exactly this date; in week/month it is the
    /// week / month containing this date. Overridden by `--from`/`--to` when
    /// either is given. Not allowed in `--agenda tasks`.
    #[arg(long, value_parser = validate_date, help_heading = "Agenda")]
    pub date: Option<String>,

    /// Window start for `--agenda day/week/month` (YYYY-MM-DD). Together with
    /// `--to` forms an explicit range that overrides `--date`. If `--to` is
    /// omitted, the window ends at `--current-date` (or today).
    #[arg(long, value_parser = validate_date, conflicts_with = "tasks", help_heading = "Agenda")]
    pub from: Option<String>,

    /// Window end for `--agenda day/week/month` (YYYY-MM-DD). Together with
    /// `--from` forms an explicit range that overrides `--date`. If `--from`
    /// is omitted, the window starts at `--current-date` (or today).
    #[arg(long, value_parser = validate_date, conflicts_with = "tasks", help_heading = "Agenda")]
    pub to: Option<String>,

    /// IANA timezone for "today" determination (e.g. `Europe/Moscow`, `UTC`)
    #[arg(long, default_value = "Europe/Moscow", value_parser = validate_timezone, help_heading = "Agenda")]
    pub tz: String,

    /// Override "today" (YYYY-MM-DD). Used as the reference point for overdue
    /// and upcoming markers, and as the default for a missing `--from`/`--to`
    /// edge. Not allowed in `--agenda tasks`.
    #[arg(long, value_parser = validate_date, help_heading = "Agenda")]
    pub current_date: Option<String>,

    /// Maximum number of tasks to extract before stopping (1..=10_000_000).
    /// Acts as a global cap on extracted tasks; the same value is reused as a
    /// per-file cap so a single hostile file cannot exhaust the global budget
    /// on its own. The scan stops as soon as either cap is hit. A separate
    /// hard limit of 10 MiB per file is built in; oversized files are counted
    /// under `files_skipped_size` in the processing summary.
    #[arg(long, default_value_t = crate::types::DEFAULT_MAX_TASKS, value_parser = validate_max_tasks, help_heading = "Limits")]
    pub max_tasks: usize,

    /// Increase logging verbosity. Repeat for more (-v = info, -vv = debug, -vvv = trace).
    /// `-vvv` is the maximum; extra `-v` are ignored and trigger a one-off
    /// saturation warning rather than unlocking a deeper level.
    /// Overridden by the `RUST_LOG` environment variable when set, regardless
    /// of `--verbose` / `--quiet` (e.g. `RUST_LOG=error` mutes `-vv`).
    #[arg(long, short = 'v', action = clap::ArgAction::Count, conflicts_with = "quiet", help_heading = "Diagnostics")]
    pub verbose: u8,

    /// Suppress all diagnostics (warnings and the per-run processing summary
    /// of skipped/failed files). Hard errors still go to stderr.
    #[arg(
        long,
        short = 'q',
        conflicts_with = "verbose",
        help_heading = "Diagnostics"
    )]
    pub quiet: bool,

    /// Disable ANSI color codes in diagnostic output. The `NO_COLOR` env var
    /// has the same effect (see <https://no-color.org>). Shortcut for
    /// `--color never`; cannot be combined with `--color`.
    #[arg(long, conflicts_with = "color", help_heading = "Diagnostics")]
    pub no_color: bool,

    /// Color output mode for diagnostics: `auto` (default), `always`, `never`.
    /// In `auto` mode color is enabled only when stderr is a TTY. The env vars
    /// `NO_COLOR`, `CLICOLOR=0`, and `CLICOLOR_FORCE` (non-zero) are also
    /// honored — see the `--no-color` flag and <https://bixense.com/clicolors/>.
    #[arg(long, value_enum, default_value = "auto", help_heading = "Diagnostics")]
    pub color: ColorMode,

    /// Print holidays for the given year (1900..=2100) and exit.
    /// Short-circuits scanning; cannot be combined with scan/agenda flags.
    #[arg(
        long,
        value_parser = validate_year,
        conflicts_with_all = ["dir", "glob", "format", "output", "tasks", "agenda", "date", "from", "to", "absolute_paths", "max_tasks", "completions"],
        help_heading = "Actions"
    )]
    pub holidays: Option<i32>,

    /// Print the shell completion script for the given shell and exit.
    /// Short-circuits scanning; cannot be combined with scan/agenda flags.
    /// Usage: `markdown-org-extract --completions bash > ~/.local/share/bash-completion/completions/markdown-org-extract`.
    #[arg(
        long,
        value_enum,
        value_name = "SHELL",
        conflicts_with_all = ["dir", "glob", "format", "output", "tasks", "agenda", "date", "from", "to", "absolute_paths", "max_tasks", "holidays"],
        help_heading = "Actions"
    )]
    pub completions: Option<clap_complete::Shell>,
}

/// Snapshot of the color-related environment, taken once per invocation so the
/// decision logic stays pure and unit-testable.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ColorEnv {
    /// `NO_COLOR` is set to any value (including empty).
    /// Per <https://no-color.org>, the mere presence of the variable is the
    /// signal; the value is irrelevant.
    pub no_color: bool,
    /// `CLICOLOR_FORCE` is set to a value other than `0` or empty.
    /// Per <https://bixense.com/clicolors/>, a non-zero value forces color
    /// even when stderr is not a TTY.
    pub clicolor_force: bool,
    /// `CLICOLOR` is explicitly `0`. Other values (including unset) leave the
    /// auto-detection in place — `CLICOLOR=1` is documented as "use color
    /// when output is a terminal", which is already our default.
    pub clicolor_zero: bool,
}

impl ColorEnv {
    /// Read the color-related env vars from the current process. Called once
    /// from `Cli::use_color`; tests build a `ColorEnv` literal instead so they
    /// don't have to manipulate shared process state.
    fn from_process_env() -> Self {
        Self {
            no_color: std::env::var_os("NO_COLOR").is_some(),
            clicolor_force: clicolor_force_active(std::env::var("CLICOLOR_FORCE").ok().as_deref()),
            clicolor_zero: matches!(std::env::var("CLICOLOR").ok().as_deref(), Some("0")),
        }
    }
}

/// `CLICOLOR_FORCE` is "active" when set to a value that is neither empty
/// nor `0`. The spec at <https://bixense.com/clicolors/> says "set to a
/// value not equal to 0"; we treat the empty string as "no clear value" and
/// therefore not active, matching how `git` and `ls --color=auto` behave.
fn clicolor_force_active(value: Option<&str>) -> bool {
    match value {
        Some(v) => !v.is_empty() && v != "0",
        None => false,
    }
}

/// Pure decision: given the resolved flags and env snapshot, should color be
/// emitted? Extracted from `Cli::use_color` so the precedence is exhaustively
/// covered by unit tests without touching the process environment.
///
/// Precedence (highest first):
/// 1. `--color always` -> on
/// 2. `--color never` or `--no-color` -> off
/// 3. `NO_COLOR` env present -> off (per <https://no-color.org>)
/// 4. `CLICOLOR_FORCE` non-zero -> on (overrides TTY check)
/// 5. `CLICOLOR=0` -> off
/// 6. `--color auto` -> follow `is_tty`
pub(crate) fn decide_use_color(
    mode: ColorMode,
    no_color_flag: bool,
    env: ColorEnv,
    is_tty: bool,
) -> bool {
    match mode {
        ColorMode::Always => return true,
        ColorMode::Never => return false,
        ColorMode::Auto => {}
    }
    if no_color_flag {
        return false;
    }
    if env.no_color {
        return false;
    }
    if env.clicolor_force {
        return true;
    }
    if env.clicolor_zero {
        return false;
    }
    is_tty
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

    /// Whether the user passed more `-v`s than the level mapping uses.
    /// `-vvv` already lands on TRACE — anything beyond is silently equal,
    /// which historically misled users into thinking "-vvvv" meant a
    /// deeper level. Returns `true` once the count exceeds the documented
    /// `-vvv` so callers can emit a one-off saturation warning.
    pub fn verbose_saturated(&self) -> bool {
        self.verbose > 3
    }

    /// Whether ANSI color should be used in the log output.
    ///
    /// See `decide_use_color` for the full precedence table; this wrapper
    /// only takes the env snapshot and TTY probe.
    pub fn use_color(&self) -> bool {
        use std::io::IsTerminal;
        // Conservative default: assume no TTY unless we can prove otherwise.
        // Keeps tests deterministic (assert_cmd pipes stderr).
        let is_tty = std::io::stderr().is_terminal();
        decide_use_color(
            self.color,
            self.no_color,
            ColorEnv::from_process_env(),
            is_tty,
        )
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

/// Human-readable form of [`MAX_TASKS_ALLOWED`] for validator messages, grouped
/// with underscores so a seven-digit cap reads clearly (`10_000_000` rather
/// than `10000000`). Kept in sync with the numeric constant by the
/// `max_tasks_allowed_display_matches_value` unit test (CLI-UX info 8 in the
/// 2026-05-25 review).
const MAX_TASKS_ALLOWED_DISPLAY: &str = "10_000_000";

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
                    format!("out of range, must be at most {MAX_TASKS_ALLOWED_DISPLAY}")
                }
                _ => format!("must be a positive integer up to {MAX_TASKS_ALLOWED_DISPLAY}"),
            });
        }
    };
    if n == 0 {
        return Err("must be at least 1".to_string());
    }
    if n > MAX_TASKS_ALLOWED {
        return Err(format!("must be at most {MAX_TASKS_ALLOWED_DISPLAY}"));
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

fn validate_locale(s: &str) -> Result<String, String> {
    // Reject unknown entries at clap parse time (exit 2) rather than letting
    // them dissolve into a tracing::warn! that --quiet swallows. The empty
    // segment is still tolerated so `--locale ru,` (trailing comma) and
    // `--locale ,en` (leading comma) keep working as before.
    for seg in s.split(',') {
        let entry = seg.trim();
        if entry.is_empty() {
            continue;
        }
        if !SUPPORTED_LOCALES.contains(&entry) {
            return Err(format!(
                "unknown locale '{entry}'; supported: {SUPPORTED_LOCALES:?}"
            ));
        }
    }
    Ok(s.to_string())
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

/// Return the (foreign, English) weekday-name pairs for the requested
/// `locale` string. `locale` is the comma-separated value of `--locale`
/// (e.g. `"ru,en"`); each segment is looked up independently, and
/// `"en"` / empty / whitespace segments contribute nothing because
/// English weekday names need no translation.
///
/// The returned mappings are fed to the timestamp parser as a Russian-
/// to-English alias table so org-mode timestamps written with Cyrillic
/// weekday abbreviations (`<2026-01-12 Пн>`) are parsed identically to
/// their English equivalents.
///
/// Callers are expected to have run the value through `validate_locale`
/// already, so unknown locales never reach this function — see the
/// `--locale` CLI validator in this module for the single source of
/// truth.
pub fn get_weekday_mappings(locale: &str) -> Vec<(&'static str, &'static str)> {
    // The CLI surface validates locale entries against SUPPORTED_LOCALES via
    // `validate_locale`, so reaching this function with anything outside
    // {"ru", "en", ""} means a programmer bypassed the value_parser. Unknown
    // entries are silently dropped here rather than warned about — the
    // single source of truth for "unknown locale" is the CLI validator.
    let mut mappings = Vec::new();
    for loc in locale.split(',') {
        // "en" / empty / anything else: nothing to translate. The CLI
        // validator already rejected unrecognised entries, so this
        // catch-all should only hit "en" or whitespace in practice.
        if loc.trim() == "ru" {
            mappings.extend_from_slice(RU_WEEKDAY_MAPPINGS);
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
        // In-range usize but above MAX_TASKS_ALLOWED. The cap is rendered with
        // digit-group underscores (CLI-UX info 8, 2026-05-25 review).
        let err = validate_max_tasks("20000000").unwrap_err();
        assert!(err.contains("at most 10_000_000"), "got: {err}");
    }

    #[test]
    fn validate_max_tasks_rejects_non_number_with_explicit_cap_hint() {
        // Non-numeric input must still mention the cap so the user knows what
        // range is acceptable, not just "must be a positive integer".
        let err = validate_max_tasks("abc").unwrap_err();
        assert!(err.contains("positive integer"), "got: {err}");
        assert!(
            err.contains("10_000_000"),
            "expected cap in message, got: {err}"
        );
    }

    #[test]
    fn max_tasks_allowed_display_matches_value() {
        // The human-readable cap string is a separate const for readability;
        // pin that it still denotes the numeric cap so the two cannot drift.
        let parsed: usize = MAX_TASKS_ALLOWED_DISPLAY
            .replace('_', "")
            .parse()
            .expect("display must be a number once underscores are stripped");
        assert_eq!(
            parsed, MAX_TASKS_ALLOWED,
            "MAX_TASKS_ALLOWED_DISPLAY must match MAX_TASKS_ALLOWED"
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
            err.contains("10_000_000"),
            "expected cap in message, got: {err}"
        );
    }

    // --- decide_use_color: precedence matrix -------------------------------

    fn env_with(no_color: bool, clicolor_force: bool, clicolor_zero: bool) -> ColorEnv {
        ColorEnv {
            no_color,
            clicolor_force,
            clicolor_zero,
        }
    }

    #[test]
    fn color_always_overrides_everything() {
        // `--color always` is the strongest override: NO_COLOR / CLICOLOR=0 /
        // no-TTY must all lose to it. The user explicitly asked for color.
        assert!(decide_use_color(
            ColorMode::Always,
            true,
            env_with(true, false, true),
            false,
        ));
    }

    #[test]
    fn color_never_overrides_everything() {
        // Symmetric to Always: explicit `never` beats CLICOLOR_FORCE and TTY.
        assert!(!decide_use_color(
            ColorMode::Never,
            false,
            env_with(false, true, false),
            true,
        ));
    }

    #[test]
    fn no_color_flag_beats_clicolor_force() {
        // `--no-color` is an explicit user override and must beat the env-only
        // CLICOLOR_FORCE signal. The flag is on the same precedence tier as
        // `--color never`.
        assert!(!decide_use_color(
            ColorMode::Auto,
            true,
            env_with(false, true, false),
            true,
        ));
    }

    #[test]
    fn no_color_env_beats_clicolor_force() {
        // Per no-color.org, NO_COLOR is "explicit user opt-out". We give it
        // priority over CLICOLOR_FORCE because a user setting NO_COLOR has
        // typed it themselves; CLICOLOR_FORCE is more often inherited from
        // shell config or tooling and should not silently revive color.
        assert!(!decide_use_color(
            ColorMode::Auto,
            false,
            env_with(true, true, false),
            true,
        ));
    }

    #[test]
    fn clicolor_force_overrides_no_tty() {
        // The point of CLICOLOR_FORCE: emit color even when piped. Without
        // this branch, piping into `less -R` could not get colored output.
        assert!(decide_use_color(
            ColorMode::Auto,
            false,
            env_with(false, true, false),
            false,
        ));
    }

    #[test]
    fn clicolor_force_beats_clicolor_zero() {
        // The two CLICOLOR vars can be set inconsistently; the spec says
        // FORCE wins, and we follow it. This protects scripts that set FORCE
        // explicitly from a globally-exported CLICOLOR=0.
        assert!(decide_use_color(
            ColorMode::Auto,
            false,
            env_with(false, true, true),
            false,
        ));
    }

    #[test]
    fn clicolor_zero_disables_in_auto() {
        // CLICOLOR=0 with no FORCE and no NO_COLOR forces off in auto mode,
        // even when stderr is a TTY. Distinguishes from "unset" — only the
        // literal "0" counts.
        assert!(!decide_use_color(
            ColorMode::Auto,
            false,
            env_with(false, false, true),
            true,
        ));
    }

    #[test]
    fn auto_with_no_env_follows_tty() {
        // With no relevant env vars, auto must mirror the TTY probe exactly.
        // This is the legacy behaviour and the most common case.
        assert!(decide_use_color(
            ColorMode::Auto,
            false,
            env_with(false, false, false),
            true,
        ));
        assert!(!decide_use_color(
            ColorMode::Auto,
            false,
            env_with(false, false, false),
            false,
        ));
    }

    #[test]
    fn validate_locale_accepts_supported() {
        assert!(validate_locale("ru").is_ok());
        assert!(validate_locale("en").is_ok());
        assert!(validate_locale("ru,en").is_ok());
    }

    #[test]
    fn validate_locale_tolerates_empty_segments() {
        // Trailing / leading / doubled commas must keep working — they were
        // tolerated by the previous warn-based path and removing that
        // tolerance would silently break existing scripts.
        assert!(validate_locale("ru,").is_ok());
        assert!(validate_locale(",en").is_ok());
        assert!(validate_locale("ru,,en").is_ok());
        assert!(validate_locale("").is_ok());
    }

    #[test]
    fn validate_locale_rejects_unknown_entry() {
        // The whole point of the validator: surface unknown locales to the
        // user instead of dissolving them into a tracing::warn! that
        // --quiet would swallow.
        let err = validate_locale("ru,de").unwrap_err();
        assert!(err.contains("unknown locale 'de'"), "got: {err}");
        assert!(err.contains("ru"), "expected supported list, got: {err}");
        assert!(err.contains("en"), "expected supported list, got: {err}");
    }

    #[test]
    fn validate_locale_rejects_unknown_with_whitespace_padding() {
        // Whitespace around entries must not let an unknown locale slip past
        // the check ("--locale ru, de" should still fail on `de`).
        let err = validate_locale("ru, de").unwrap_err();
        assert!(err.contains("unknown locale 'de'"), "got: {err}");
    }

    #[test]
    fn clicolor_force_active_treats_zero_and_empty_as_inactive() {
        // The bixense spec says "value not equal to 0" enables force. Empty
        // string is ambiguous (a `CLICOLOR_FORCE=` line in dotenv is a common
        // way to "clear" the var) so we treat it as not-active.
        assert!(!clicolor_force_active(None));
        assert!(!clicolor_force_active(Some("0")));
        assert!(!clicolor_force_active(Some("")));
        assert!(clicolor_force_active(Some("1")));
        assert!(clicolor_force_active(Some("yes")));
        // Any non-zero, non-empty value enables, including whitespace — the
        // user clearly wrote something, so honour it.
        assert!(clicolor_force_active(Some(" ")));
    }
}
