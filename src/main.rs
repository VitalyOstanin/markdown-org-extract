#![warn(missing_docs)]
//! CLI utility for extracting tasks from markdown files with Emacs
//! Org-mode support. See [`README.md`] at the repository root for the
//! user-facing description.
//!
//! This binary is a thin shell over the [`markdown_org_extract`] library:
//! it parses arguments, installs signal handlers, and writes bytes. The
//! extraction itself lives in the library so other consumers — notably an
//! Android build, which cannot spawn a process — run the same code.
//!
//! [`README.md`]: https://github.com/VitalyOstanin/markdown-org-extract

mod cli;
mod format;

use clap::Parser;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use markdown_org_extract::agenda::{self, filter_agenda, AgendaDates};
use markdown_org_extract::scan::{scan_directories, scan_directory, validate_dir, ScanOptions};
use markdown_org_extract::{render, AppError, HolidayCalendar};

use crate::cli::Cli;
use crate::format::OutputFormat;

/// Exit code for a scan aborted by SIGINT/SIGTERM. Follows the shell
/// convention `128 + signum` so `$?` after Ctrl-C is the familiar `130`.
const EXIT_INTERRUPTED: i32 = 130;

fn main() {
    // Install signal handlers before anything heavy happens so a Ctrl-C
    // during startup still triggers a clean exit. The flag is shared with
    // the scan, which polls it between walker iterations.
    let interrupt = Arc::new(AtomicBool::new(false));
    if let Err(e) = install_signal_handlers(&interrupt) {
        eprintln!("error: failed to install signal handlers: {e}");
        std::process::exit(74);
    }

    if let Err(e) = run(&interrupt) {
        // A broken pipe is the normal way a downstream consumer (e.g.
        // `… | head -n 1`) signals it has read enough. Surfacing it as
        // `error: io: <stdout>: Broken pipe (os error 32)` would train users
        // to expect spurious failures in well-formed pipelines, and other
        // Unix tools (cat, grep, jq) all stay quiet in the same situation.
        // Exit 0 silently — by the time we reach this branch we have already
        // produced the bytes the consumer kept.
        if is_broken_pipe(&e) {
            std::process::exit(0);
        }
        // Use eprintln directly: tracing may not be initialized if argument parsing failed,
        // and a hard error should always reach the user regardless of `--quiet`.
        eprintln!("error: {e}");
        std::process::exit(e.exit_code());
    }
}

/// Register a handler that flips `interrupt` to `true` on SIGINT (and SIGTERM
/// on Unix). The flag is polled by the scan so a long run can stop between
/// files and still print the per-run summary. SIGTERM is Unix-only: Windows
/// does not deliver it through the C runtime, and signal-hook would reject the
/// registration.
fn install_signal_handlers(interrupt: &Arc<AtomicBool>) -> io::Result<()> {
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(interrupt))?;
    #[cfg(unix)]
    signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(interrupt))?;
    Ok(())
}

/// True when `e` is an `AppError::Io` whose underlying `io::Error` is a
/// `BrokenPipe`. Centralised so the catch can stay precise — every other
/// IO error is still reported normally.
fn is_broken_pipe(e: &AppError) -> bool {
    if let AppError::Io { source, .. } = e {
        return source.kind() == io::ErrorKind::BrokenPipe;
    }
    false
}

fn run(interrupt: &AtomicBool) -> Result<(), AppError> {
    let cli = Cli::parse();
    cli.init_tracing();

    // Warn once when the user piles on more -v's than the level mapping
    // uses (`-vvvv`+). Without the signal, a user expecting "even more
    // detail than trace" sees no acknowledgement that the count is capped.
    // tracing::warn! is the right channel: it inherits the colour/format
    // settings and is suppressed by RUST_LOG=error if the user has
    // explicitly silenced warnings.
    if cli.verbose_saturated() {
        tracing::warn!(
            verbose = cli.verbose,
            "--verbose saturated at -vvv (TRACE); additional v's have no effect"
        );
    }

    if let Some(shell) = cli.completions {
        return handle_completions(shell);
    }

    if let Some(year) = cli.holidays {
        return handle_holidays(year);
    }

    if let Some(ref out_path) = cli.output {
        if !is_stdout_sigil(out_path) {
            validate_output_path(out_path)?;
        }
    }

    // Validate before the run span opens so a bad `--dir` is reported without
    // a log line that already claims to be scanning it. Every root, not the
    // first: a run over three collections must not walk two of them before
    // saying the third is not there.
    let roots = cli
        .dir
        .iter()
        .map(|dir| validate_dir(dir))
        .collect::<Result<Vec<_>, _>>()?;

    // Root span for the whole run, carrying the scanned directory. Every
    // event from here on — the per-file spans, `scan finished`, the summary,
    // and the agenda events that otherwise have no span — inherits
    // `run{dir=...}` so a multi-run log can be attributed (2026-05-25 review,
    // O4). It is an `info_span`, so at the default `warn` level it is inactive
    // and adds nothing to the default output; the context appears from `-v`
    // upward, the same threshold at which `scan finished` becomes visible.
    let run_span = tracing::info_span!("run", dir = %joined(&roots));
    let _run = run_span.enter();

    let options = ScanOptions {
        glob: &cli.glob,
        max_tasks: cli.max_tasks,
        absolute_paths: cli.absolute_paths,
        locale: &cli.locale,
    };
    // One root goes through `scan_directory` so its output is unchanged: the
    // tasks of a single-directory run carry no `root` field, which is what
    // every consumer written against it reads.
    let outcome = match roots.as_slice() {
        [only] => scan_directory(only, &options, Some(interrupt))?,
        several => scan_directories(several, &options, Some(interrupt))?,
    };
    let stats = outcome.stats;

    tracing::info!(
        files = stats.files_processed,
        tasks = outcome.tasks.len(),
        interrupted = stats.interrupted,
        "scan finished"
    );

    // A SIGINT/SIGTERM during the walk short-circuits the rest of the
    // pipeline: emit the partial summary so the user sees what was
    // processed, then exit with the conventional `128 + SIGINT` code so
    // shell pipelines can distinguish "aborted" from "ok" and from real
    // errors. Skipping `render_output` is intentional — a half-formed
    // agenda is worse than no agenda.
    if stats.interrupted {
        stats.print_summary();
        std::process::exit(EXIT_INTERRUPTED);
    }

    if stats.has_warnings() {
        stats.print_summary();
    }

    let agenda_output = filter_agenda(
        outcome.tasks,
        cli.agenda_scope(),
        AgendaDates {
            date: cli.date.as_deref(),
            from: cli.from.as_deref(),
            to: cli.to.as_deref(),
            current_date: cli.current_date.as_deref(),
            week_start: cli.week_start.as_deref(),
        },
        &cli.tz,
        cli.tasks_include_done,
        cli.tasks_include_cancelled,
        // `timestamp_next` only exists in the JSON wire format; the Markdown
        // and HTML renderers never print it, so there is nothing to compute.
        matches!(cli.format, OutputFormat::Json),
    )?;

    render_output(&cli, agenda_output)
}

/// The scanned roots as one span field, comma-separated.
///
/// The span carries what the run was over, and with several roots that is all
/// of them: a log attributed to the first of three says nothing about where a
/// warning from the third came from.
fn joined(roots: &[std::path::PathBuf]) -> String {
    roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Handle the `--holidays YEAR` short-circuit: emit a JSON array of
/// `YYYY-MM-DD` dates and exit before any file scanning happens.
fn handle_holidays(year: i32) -> Result<(), AppError> {
    let calendar = HolidayCalendar::global();
    let holidays = calendar.get_holidays_for_year(year);
    let dates: Vec<String> = holidays
        .iter()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .collect();
    let mut output = serde_json::to_string_pretty(&dates)?;
    ensure_trailing_newline(&mut output);
    io::stdout()
        .write_all(output.as_bytes())
        .map_err(|e| AppError::io("<stdout>", e))?;
    Ok(())
}

/// Ensure `s` ends with exactly one `\n`. Renderers vary: `serde_json` and
/// the HTML/JSON-array formatters return a string with no trailing newline,
/// while the Markdown formatter already adds one. Calling this before every
/// write keeps the contract uniform (POSIX text file shape, prompt on the
/// next line) without producing `\n\n` for formatters that already emitted
/// the newline.
fn ensure_trailing_newline(s: &mut String) {
    if !s.ends_with('\n') {
        s.push('\n');
    }
}

/// Handle the `--completions <SHELL>` short-circuit: emit the completion
/// script for `shell` on stdout and exit. Used to register shell completions
/// at install time (e.g. via the user's shell config).
fn handle_completions(shell: clap_complete::Shell) -> Result<(), AppError> {
    let mut cmd = <Cli as clap::CommandFactory>::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut io::stdout());
    Ok(())
}

/// Serialize the agenda result into the requested format and either write it
/// to `--output` or to stdout.
fn render_output(cli: &Cli, agenda_output: agenda::AgendaOutput) -> Result<(), AppError> {
    let mut output = match cli.format {
        OutputFormat::Json => match agenda_output {
            agenda::AgendaOutput::Days(days) => serde_json::to_string_pretty(&days)?,
            agenda::AgendaOutput::Tasks(tasks) => serde_json::to_string_pretty(&tasks)?,
        },
        OutputFormat::Markdown => match agenda_output {
            agenda::AgendaOutput::Days(days) => render::render_days_markdown(&days),
            agenda::AgendaOutput::Tasks(tasks) => render::render_markdown(&tasks),
        },
        OutputFormat::Html => match agenda_output {
            agenda::AgendaOutput::Days(days) => render::render_days_html(&days),
            agenda::AgendaOutput::Tasks(tasks) => render::render_html(&tasks),
        },
    };
    ensure_trailing_newline(&mut output);

    match cli.output.as_deref() {
        Some(p) if !is_stdout_sigil(p) => {
            fs::write(p, output).map_err(|e| AppError::io(p.display().to_string(), e))?
        }
        // None or `--output -` both mean stdout. The explicit `-` form is the
        // standard unix sigil for stdout and lets shell pipelines target it
        // unambiguously when stdout is otherwise reserved (e.g. tee chains).
        _ => io::stdout()
            .write_all(output.as_bytes())
            .map_err(|e| AppError::io("<stdout>", e))?,
    }

    Ok(())
}

/// Returns true when the path is the standard unix sigil `-` meaning stdout.
fn is_stdout_sigil(path: &Path) -> bool {
    path.as_os_str() == "-"
}

/// Validate that the `--output` target is safe to write:
/// - the parent directory exists and is a directory;
/// - the target itself is not an existing symlink (refuse symlink overwrite).
///
/// There is a TOCTOU window between this check and the subsequent
/// `fs::write` in `render_output`: an attacker who already controls the
/// parent directory could replace the target with a symlink between the two
/// calls. For a non-setuid CLI run by an ordinary user this is
/// acceptable — the attacker already has full access to the same
/// directory. Closing the window completely needs `O_NOFOLLOW` on the
/// open path (Unix-only) and is left for a future change if the threat
/// model shifts.
fn validate_output_path(path: &Path) -> Result<(), AppError> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    if !parent.exists() {
        return Err(AppError::InvalidOutput(format!(
            "parent directory does not exist: {}",
            parent.display()
        )));
    }
    if !parent.is_dir() {
        return Err(AppError::InvalidOutput(format!(
            "parent is not a directory: {}",
            parent.display()
        )));
    }

    // NotFound is the expected case when --output names a fresh file. Any other
    // error (PermissionDenied on the path itself, EIO, etc.) means we cannot
    // confirm symlink safety — fail loudly here instead of letting fs::write
    // produce a confusing error message later.
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(AppError::InvalidOutput(format!(
                "refusing to overwrite symlink: {}",
                path.display()
            )));
        }
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(AppError::InvalidOutput(format!(
                "cannot inspect output path {}: {e}",
                path.display()
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn validate_output_rejects_missing_parent() {
        let p = PathBuf::from("/nonexistent_definitely_xyz/out.json");
        assert!(matches!(
            validate_output_path(&p),
            Err(AppError::InvalidOutput(_))
        ));
    }

    #[test]
    fn validate_output_accepts_missing_target_in_existing_dir() {
        // NotFound on the target itself is the normal "write to a fresh file" case.
        let dir = tempdir().unwrap();
        let target = dir.path().join("fresh.json");
        validate_output_path(&target).expect("missing target in existing dir must be OK");
    }

    #[test]
    fn validate_output_accepts_existing_regular_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("regular.json");
        fs::write(&target, b"existing").unwrap();
        validate_output_path(&target).expect("existing regular file must be OK");
    }

    #[test]
    #[cfg(unix)]
    fn validate_output_rejects_existing_symlink_target() {
        use std::os::unix::fs::symlink;
        let dir = tempdir().unwrap();
        let real = dir.path().join("real.json");
        fs::write(&real, b"data").unwrap();
        let link = dir.path().join("link.json");
        symlink(&real, &link).unwrap();
        let err = validate_output_path(&link).expect_err("symlink must be rejected");
        assert!(matches!(err, AppError::InvalidOutput(ref m) if m.contains("symlink")));
    }

    #[test]
    fn ensure_trailing_newline_adds_one_when_missing() {
        let mut s = String::from("payload");
        ensure_trailing_newline(&mut s);
        assert_eq!(s, "payload\n");
    }

    #[test]
    fn ensure_trailing_newline_leaves_an_existing_one_alone() {
        // Formatters that already terminate their output must not gain a
        // second newline.
        let mut s = String::from("payload\n");
        ensure_trailing_newline(&mut s);
        assert_eq!(s, "payload\n");
    }

    #[test]
    fn stdout_sigil_is_only_the_bare_dash() {
        assert!(is_stdout_sigil(Path::new("-")));
        assert!(!is_stdout_sigil(Path::new("./-")));
        assert!(!is_stdout_sigil(Path::new("out.json")));
    }
}
