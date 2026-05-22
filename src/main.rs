#![warn(missing_docs)]
//! CLI utility for extracting tasks from markdown files with Emacs
//! Org-mode support. See [`README.md`] at the repository root for the
//! user-facing description; this binary's entry point lives in
//! [`main`] and the public surface used by integration tests is the
//! CLI itself, not a Rust API.
//!
//! [`README.md`]: https://github.com/VitalyOstanin/markdown-org-extract

mod agenda;
mod cli;
mod clock;
mod error;
mod format;
mod holidays;
mod parser;
mod regex_limits;
mod render;
mod timestamp;
mod types;

use clap::Parser;
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, Sink, SinkMatch};
use ignore::WalkBuilder;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::agenda::filter_agenda;
use crate::cli::{get_weekday_mappings, Cli};
use crate::error::AppError;
use crate::format::OutputFormat;
use crate::parser::extract_tasks;
use crate::render::{render_html, render_markdown};
use crate::types::{ProcessingStats, MAX_FILE_SIZE};

fn main() {
    if let Err(e) = run() {
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

/// True when `e` is an `AppError::Io` whose underlying `io::Error` is a
/// `BrokenPipe`. Centralised so the catch can stay precise — every other
/// IO error is still reported normally.
fn is_broken_pipe(e: &AppError) -> bool {
    if let AppError::Io { source, .. } = e {
        return source.kind() == io::ErrorKind::BrokenPipe;
    }
    false
}

fn run() -> Result<(), AppError> {
    let cli = Cli::parse();
    cli.init_tracing();

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

    let dir_canonical = validate_dir(&cli.dir)?;
    let mappings = get_weekday_mappings(&cli.locale);

    let (tasks, stats) = scan_files(&cli, &dir_canonical, &mappings)?;

    tracing::info!(
        files = stats.files_processed,
        tasks = tasks.len(),
        "scan finished"
    );

    if stats.has_warnings() {
        stats.print_summary();
    }

    let agenda_output = filter_agenda(
        tasks,
        cli.agenda_scope(),
        cli.date.as_deref(),
        cli.from.as_deref(),
        cli.to.as_deref(),
        &cli.tz,
        cli.current_date.as_deref(),
    )?;

    render_output(&cli, agenda_output)
}

/// Handle the `--holidays YEAR` short-circuit: emit a JSON array of
/// `YYYY-MM-DD` dates and exit before any file scanning happens.
fn handle_holidays(year: i32) -> Result<(), AppError> {
    let calendar = holidays::HolidayCalendar::global();
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

/// Validate that `--dir` points to an existing directory and canonicalize it.
fn validate_dir(dir: &Path) -> Result<PathBuf, AppError> {
    if !dir.exists() {
        return Err(AppError::InvalidDirectory(format!(
            "directory does not exist: {}",
            dir.display()
        )));
    }
    if !dir.is_dir() {
        return Err(AppError::InvalidDirectory(format!(
            "path is not a directory: {}",
            dir.display()
        )));
    }
    fs::canonicalize(dir).map_err(|e| {
        AppError::InvalidDirectory(format!("cannot canonicalize {}: {e}", dir.display()))
    })
}

/// Walk `dir_canonical`, apply the `--glob` filter and a keyword pre-filter,
/// then parse matching files into `Task`s. Returns the accumulated tasks and
/// a `ProcessingStats` recording skipped/failed files.
fn scan_files(
    cli: &Cli,
    dir_canonical: &Path,
    mappings: &[(&'static str, &'static str)],
) -> Result<(Vec<types::Task>, ProcessingStats), AppError> {
    let glob_matcher = compile_glob(&cli.glob)?;

    let mut tasks = Vec::new();
    let mut stats = ProcessingStats {
        max_tasks_limit: cli.max_tasks,
        ..ProcessingStats::default()
    };
    let matcher = RegexMatcher::new(
        r"(?m)(^[#*]+\s+(TODO|DONE)\s|DEADLINE:|SCHEDULED:|CREATED:|CLOSED:|CLOCK:)",
    )
    .map_err(|e| AppError::Regex(e.to_string()))?;

    // Defense-in-depth: refuse to follow symlinks and stay within the chosen
    // filesystem. Pass `dir_canonical` (absolute) so every emitted path is an
    // absolute descendant of the root, which lets `strip_prefix(dir_canonical)`
    // succeed downstream for both glob matching and display-path computation.
    // Using `&cli.dir` (often relative) would silently break multi-segment
    // glob patterns like `notes/*.md` against a relative `--dir`.
    let walker = WalkBuilder::new(dir_canonical)
        .standard_filters(true)
        .follow_links(false)
        .same_file_system(true)
        .build();

    // Reuse one Searcher and one read buffer across the entire walk. Both are
    // designed to be cleared and reused; allocating them per file added a
    // monotonic cost that scaled with tree size for no gain.
    let mut searcher = Searcher::new();
    let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);

    for result in walker {
        // A walker error on one entry (permission denied on a subdir, broken
        // metadata, etc.) must not abort the whole scan: the rest of the
        // tree may still contain usable files. Record it in the summary so
        // the user knows their output is partial. The Display impl of
        // ignore::Error already includes the failing path, so we forward the
        // whole message into `failed_paths` for the listing in print_summary.
        let entry = match result {
            Ok(entry) => entry,
            Err(err) => {
                stats.walk_errors += 1;
                let msg = err.to_string();
                stats.record_failed_path(&msg);
                tracing::warn!(error = %msg, "walker entry failed; skipping");
                continue;
            }
        };
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let path = entry.path();

        if !glob_match(&glob_matcher, path, dir_canonical) {
            continue;
        }

        // Read once with a hard cap into the reusable buffer. Avoids the
        // TOCTOU window where a separate metadata() check might say a file is
        // small but the subsequent read() pulls in a file that has since
        // grown — read_capped_into probes one byte past the cap and refuses
        // anything larger.
        match read_capped_into(path, MAX_FILE_SIZE, &mut buf) {
            Ok(true) => {}
            Ok(false) => {
                stats.files_skipped_size += 1;
                continue;
            }
            Err(_) => {
                stats.files_failed_read += 1;
                stats.record_failed_path(&path.display().to_string());
                continue;
            }
        }

        let mut found = false;
        if searcher
            .search_slice(&matcher, &buf, FoundSink { found: &mut found })
            .is_err()
        {
            stats.files_failed_search += 1;
            stats.record_failed_path(&path.display().to_string());
            continue;
        }

        if !found {
            continue;
        }

        let content = match std::str::from_utf8(&buf) {
            Ok(s) => s,
            Err(_) => {
                stats.files_failed_read += 1;
                stats.record_failed_path(&path.display().to_string());
                continue;
            }
        };

        let display_path = if cli.absolute_paths {
            path.display().to_string()
        } else {
            // WalkBuilder now traverses `dir_canonical`, so every emitted path
            // is an absolute descendant of it; strip_prefix cannot fail unless
            // canonicalize and the walker disagree (a TOCTOU we cannot fix
            // here). The absolute path is the safest fallback for that case.
            match path.strip_prefix(dir_canonical) {
                Ok(rel) => rel.display().to_string(),
                Err(_) => path.display().to_string(),
            }
        };

        // Wrap parsing in a span so every debug!/trace! emitted by the parser,
        // timestamp extractor, and clock extractor inherits `path` automatically.
        // Without this, multi-file runs at `-vv` produce a soup of messages
        // without any way to tie a warning back to the file it came from.
        let span = tracing::debug_span!("file", path = %display_path);
        let extracted = span.in_scope(|| {
            extract_tasks(Path::new(&display_path), content, mappings, cli.max_tasks)
        });
        tasks.extend(extracted);
        stats.files_processed += 1;

        if tasks.len() >= cli.max_tasks {
            tasks.truncate(cli.max_tasks);
            stats.max_tasks_reached = true;
            break;
        }
    }

    Ok((tasks, stats))
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
            agenda::AgendaOutput::Tasks(tasks) => render_markdown(&tasks),
        },
        OutputFormat::Html => match agenda_output {
            agenda::AgendaOutput::Days(days) => render::render_days_html(&days),
            agenda::AgendaOutput::Tasks(tasks) => render_html(&tasks),
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

/// Read a file with a hard size cap, returning `Ok(None)` if the file exceeds
/// the cap. Defense-in-depth against TOCTOU: we cannot trust a prior
/// `fs::metadata` call because the file may have grown (or been swapped out
/// for a symlink target on a different filesystem) between the metadata read
/// and the content read. Reading `cap + 1` bytes lets us detect overruns
/// without first asking the filesystem how large the file claims to be.
/// Read up to `cap` bytes from `path` into `buf`, clearing `buf` first.
///
/// Returns:
///
/// - `Ok(true)` -- file content fully read (length <= `cap`).
/// - `Ok(false)` -- file exceeds `cap`; `buf` holds the first `cap + 1` bytes
///   (caller should treat as over-cap and discard).
/// - `Err(_)` -- IO error (open / read failure).
///
/// Reusing one buffer across the scan loop lets a tight walker avoid one
/// allocation per file. The buffer's capacity grows monotonically to the
/// largest file seen, which is bounded by `MAX_FILE_SIZE` plus the probe byte.
fn read_capped_into(path: &Path, cap: u64, buf: &mut Vec<u8>) -> io::Result<bool> {
    buf.clear();
    let file = File::open(path)?;
    let probe = cap.saturating_add(1);
    file.take(probe).read_to_end(buf)?;
    Ok((buf.len() as u64) <= cap)
}

struct FoundSink<'a> {
    found: &'a mut bool,
}

impl<'a> Sink for FoundSink<'a> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, _mat: &SinkMatch) -> Result<bool, Self::Error> {
        *self.found = true;
        Ok(false)
    }
}

/// Compile a `--glob` pattern into a `globset::GlobMatcher`. Empty patterns
/// and `*.` (extension-less) are rejected for parity with previous behaviour.
fn compile_glob(pattern: &str) -> Result<globset::GlobMatcher, AppError> {
    if pattern.is_empty() {
        return Err(AppError::InvalidGlob("empty pattern".to_string()));
    }
    if pattern == "*." {
        return Err(AppError::InvalidGlob(
            "pattern '*.': extension cannot be empty".to_string(),
        ));
    }
    globset::Glob::new(pattern)
        .map(|g| g.compile_matcher())
        .map_err(|e| AppError::InvalidGlob(format_error_chain(pattern, &e)))
}

/// Flatten a `globset::Error` (or any `std::error::Error`) into a single line
/// that preserves its `source()` chain. Without this the user only sees the
/// top-level `Display`, which sometimes elides the underlying reason (e.g. the
/// specific syntax error inside a brace alternative).
fn format_error_chain(pattern: &str, err: &dyn std::error::Error) -> String {
    let mut msg = format!("invalid pattern '{pattern}': {err}");
    let mut source = err.source();
    while let Some(cause) = source {
        msg.push_str(&format!(" (caused by: {cause})"));
        source = cause.source();
    }
    msg
}

/// Match a path against the compiled glob. The matcher is tried against:
/// (1) the path relative to `dir_root` — supports patterns like `**/*.md`,
/// (2) the file name — supports patterns like `*.md` regardless of depth.
fn glob_match(matcher: &globset::GlobMatcher, path: &Path, dir_root: &Path) -> bool {
    if let Ok(rel) = path.strip_prefix(dir_root) {
        if matcher.is_match(rel) {
            return true;
        }
    }
    if let Some(name) = path.file_name() {
        return matcher.is_match(Path::new(name));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn m(pattern: &str, file: &str) -> bool {
        let matcher = compile_glob(pattern).unwrap();
        glob_match(&matcher, &PathBuf::from(file), Path::new(""))
    }

    #[test]
    fn glob_simple_extension_matches_at_any_depth() {
        assert!(m("*.md", "test.md"));
        assert!(m("*.md", "src/notes/test.md"));
        assert!(!m("*.md", "test.txt"));
    }

    #[test]
    fn glob_exact_name_matches() {
        assert!(m("README.md", "README.md"));
        assert!(!m("README.md", "OTHER.md"));
    }

    #[test]
    fn glob_double_star_matches_full_path() {
        assert!(m("**/*.md", "src/notes/test.md"));
        assert!(m("src/*.md", "src/test.md"));
        assert!(!m("src/*.md", "other/test.md"));
    }

    #[test]
    fn glob_invalid_patterns_rejected() {
        assert!(compile_glob("").is_err());
        assert!(compile_glob("*.").is_err());
        // unbalanced brace — globset rejects it
        assert!(compile_glob("{md,").is_err());
    }

    #[test]
    fn compile_glob_message_echoes_offending_pattern() {
        // The user-facing message must mention the pattern so the user does
        // not have to guess which invocation produced the error.
        let err = compile_glob("{md,").unwrap_err();
        let s = err.to_string();
        assert!(s.contains("{md,"), "pattern missing in message: {s}");
        assert!(s.contains("invalid pattern"), "expected prefix, got: {s}");
    }

    #[test]
    fn format_error_chain_walks_source() {
        use std::error::Error;
        use std::fmt;
        // Two-link chain: Outer ── source ──> Inner.
        #[derive(Debug)]
        struct Inner;
        impl fmt::Display for Inner {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "inner reason")
            }
        }
        impl Error for Inner {}

        #[derive(Debug)]
        struct Outer(Inner);
        impl fmt::Display for Outer {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "outer failure")
            }
        }
        impl Error for Outer {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                Some(&self.0)
            }
        }

        let msg = format_error_chain("pat", &Outer(Inner));
        assert!(msg.contains("invalid pattern 'pat'"), "got: {msg}");
        assert!(msg.contains("outer failure"), "top-level missing: {msg}");
        assert!(
            msg.contains("caused by: inner reason"),
            "source missing: {msg}"
        );
    }

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
    fn read_capped_into_returns_true_when_file_within_limit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("small.md");
        fs::write(&path, b"hello world").unwrap();
        let mut buf = Vec::new();
        assert!(read_capped_into(&path, 1024, &mut buf).unwrap());
        assert_eq!(buf, b"hello world");
    }

    #[test]
    fn read_capped_into_returns_true_at_exact_limit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("exact.md");
        let payload = vec![b'x'; 64];
        fs::write(&path, &payload).unwrap();
        let mut buf = Vec::new();
        assert!(read_capped_into(&path, 64, &mut buf).unwrap());
        assert_eq!(buf, payload);
    }

    #[test]
    fn read_capped_into_returns_false_when_file_over_limit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("big.md");
        let payload = vec![b'x'; 65];
        fs::write(&path, &payload).unwrap();
        // cap is 64, file is 65 bytes — must be rejected (false), not truncated.
        let mut buf = Vec::new();
        let ok = read_capped_into(&path, 64, &mut buf).unwrap();
        assert!(!ok, "expected false for file exceeding cap (read {} bytes)", buf.len());
    }

    #[test]
    fn read_capped_into_returns_err_for_missing_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.md");
        let mut buf = Vec::new();
        assert!(read_capped_into(&path, 64, &mut buf).is_err());
    }

    #[test]
    fn read_capped_into_clears_previous_contents() {
        // Buffer reuse contract: any leftover content from a previous read
        // must not bleed into the next file.
        let dir = tempdir().unwrap();
        let path1 = dir.path().join("first.md");
        let path2 = dir.path().join("second.md");
        fs::write(&path1, b"longer content here").unwrap();
        fs::write(&path2, b"short").unwrap();

        let mut buf = Vec::new();
        read_capped_into(&path1, 1024, &mut buf).unwrap();
        assert_eq!(buf, b"longer content here");
        read_capped_into(&path2, 1024, &mut buf).unwrap();
        assert_eq!(buf, b"short", "buffer must be cleared on each read");
    }
}
