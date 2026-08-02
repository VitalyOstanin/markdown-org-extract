//! Directory scanning: walk a tree, pre-filter by glob and by content, then
//! parse matching files into [`Task`]s.
//!
//! This is the half of the old `main.rs` that has nothing to do with being a
//! command-line program. It takes [`ScanOptions`] rather than the parsed CLI
//! so embedders — notably an Android build, where no process can be spawned —
//! drive the same code path the binary does.

use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, Sink, SinkMatch};
use ignore::WalkBuilder;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::AppError;
use crate::locale::get_weekday_mappings;
use crate::parser::extract_tasks_with_counter;
use crate::types::{ProcessingStats, Task, DEFAULT_MAX_TASKS, MAX_FILE_SIZE};

/// Initial capacity of the read buffer reused across the walk. Sized at 64 KiB
/// to cover most source files in a single allocation while still amortising to
/// one buffer for the whole tree; the buffer grows on demand for larger files.
const READ_BUF_INITIAL_CAP: usize = 64 * 1024;

/// Inputs to [`scan_directory`]. The defaults match the CLI's own defaults, so
/// an embedder that only wants "scan this directory the way the tool does"
/// writes `ScanOptions::default()`.
#[derive(Debug, Clone, Copy)]
pub struct ScanOptions<'a> {
    /// File-name / relative-path glob, as `--glob`. Defaults to `*.md`.
    pub glob: &'a str,
    /// Upper bound on collected tasks, as `--max-tasks`. Reaching it stops the
    /// walk and sets `ProcessingStats::max_tasks_reached`.
    pub max_tasks: usize,
    /// Emit absolute paths in `Task::file` instead of paths relative to the
    /// scanned root, as `--absolute-paths`.
    pub absolute_paths: bool,
    /// Comma-separated locale list for weekday-name normalization, as
    /// `--locale`. Defaults to `ru,en`; see [`crate::locale`].
    pub locale: &'a str,
}

impl Default for ScanOptions<'_> {
    fn default() -> Self {
        Self {
            glob: "*.md",
            max_tasks: DEFAULT_MAX_TASKS,
            absolute_paths: false,
            locale: "ru,en",
        }
    }
}

/// What a scan produced: the tasks themselves plus the diagnostics the CLI
/// prints as its per-run summary. Embedders can ignore `stats`, but it is the
/// only channel that reports skipped and failed files, so dropping it silently
/// would hide a partial result.
#[derive(Debug)]
pub struct ScanOutcome {
    /// Tasks extracted from every matching file, in walk order.
    pub tasks: Vec<Task>,
    /// Per-run counters: processed / skipped / failed files, warnings, and
    /// whether the walk was cut short by the task cap or by an interrupt.
    pub stats: ProcessingStats,
}

/// Scan `dir` and return the tasks found in it.
///
/// `interrupt`, when supplied, is polled between walker iterations: flipping it
/// to `true` stops the walk at the next file boundary and reports
/// `stats.interrupted`. The binary wires it to its SIGINT/SIGTERM handler;
/// in-process callers usually pass `None`.
///
/// Errors:
/// - `AppError::InvalidDirectory` — `dir` is missing, is not a directory, or
///   cannot be canonicalized.
/// - `AppError::InvalidGlob` — `options.glob` is empty, is `*.`, or fails to
///   compile.
/// - `AppError::Regex` — the built-in keyword pre-filter failed to compile,
///   which is a bug rather than a user error.
pub fn scan_directory(
    dir: &Path,
    options: &ScanOptions<'_>,
    interrupt: Option<&AtomicBool>,
) -> Result<ScanOutcome, AppError> {
    let dir_canonical = validate_dir(dir)?;
    let mappings = get_weekday_mappings(options.locale);
    let mut run = Run::new(options);
    // No root on the tasks: the caller named the one directory and `file` is
    // relative to it, so repeating it on every task would say nothing new.
    scan_files(
        options,
        &dir_canonical,
        &mappings,
        interrupt,
        None,
        &mut run,
    )?;

    Ok(run.finish())
}

/// Scan several roots in one run and return the tasks of all of them.
///
/// Notes are kept in more than one place — a work repository and a private
/// one, a shared vault and a personal one — and the agenda over them is the
/// agenda of all of them together. The merge is here rather than in the caller
/// because the parts that make it a merge belong to a scan: the task cap is a
/// budget for the run, the statistics are one report over it, and
/// [`filter_agenda`](crate::filter_agenda) already takes a flat list of tasks
/// whatever they were read from.
///
/// Every task carries [`Task::root`], the canonical path of the directory its
/// `file` is relative to: the same relative path in two roots is two different
/// files. The roots are walked in the order they are given, and one named
/// twice is walked once — the same directory configured as two sources would
/// otherwise show every task in it twice.
///
/// A root nested inside another one is not detected, and the notes under it
/// are read by both walks. Nesting roots is a choice the caller makes, and
/// refusing it would rule out a collection that deliberately holds a smaller
/// one.
///
/// Errors:
/// - `AppError::InvalidDirectory` — the list is empty, or any root is missing,
///   is not a directory, or cannot be canonicalized. Refused rather than
///   skipped: a directory that has been unmounted or renamed would otherwise
///   read as a collection with nothing in it.
/// - `AppError::InvalidGlob` / `AppError::Regex` — as for [`scan_directory`].
pub fn scan_directories(
    dirs: &[PathBuf],
    options: &ScanOptions<'_>,
    interrupt: Option<&AtomicBool>,
) -> Result<ScanOutcome, AppError> {
    if dirs.is_empty() {
        return Err(AppError::InvalidDirectory(
            "no directory to scan".to_string(),
        ));
    }

    // Every root is validated before any of them is walked, so a mistyped path
    // is reported as such rather than after a walk that already spent seconds
    // on the roots before it.
    let mut roots: Vec<PathBuf> = Vec::with_capacity(dirs.len());
    for dir in dirs {
        let canonical = validate_dir(dir)?;
        if !roots.contains(&canonical) {
            roots.push(canonical);
        }
    }

    let mappings = get_weekday_mappings(options.locale);
    let mut run = Run::new(options);

    for root in &roots {
        // Both stops belong to the run rather than to one root: a cap that has
        // been reached is not going to un-reach itself, and a signal that
        // stopped the first walk must not start the second.
        if run.stats.interrupted || run.stats.max_tasks_reached {
            break;
        }

        let label = root.display().to_string();
        scan_files(
            options,
            root,
            &mappings,
            interrupt,
            Some(label.as_str()),
            &mut run,
        )?;
    }

    Ok(run.finish())
}

/// What a scan accumulates across the roots it walks.
///
/// One vector and one `ProcessingStats` for the whole run, so the task cap is
/// a budget over all the roots and the summary is a single report.
struct Run {
    tasks: Vec<Task>,
    stats: ProcessingStats,
}

impl Run {
    fn new(options: &ScanOptions<'_>) -> Self {
        Self {
            tasks: Vec::new(),
            stats: ProcessingStats {
                max_tasks_limit: options.max_tasks,
                ..ProcessingStats::default()
            },
        }
    }

    fn finish(self) -> ScanOutcome {
        ScanOutcome {
            tasks: self.tasks,
            stats: self.stats,
        }
    }
}

/// Validate that a scan root points to an existing directory and canonicalize
/// it. Exposed because the binary validates `--dir` before it opens the run
/// span, so the error surfaces before any log line mentions the directory.
pub fn validate_dir(dir: &Path) -> Result<PathBuf, AppError> {
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

/// Walk `dir_canonical`, apply the glob filter and a keyword pre-filter, then
/// parse matching files into `Task`s, appending them to `run`.
///
/// `root` is what every task found here reports as [`Task::root`]; `None`
/// leaves the field unset, which is the single-directory case. The tasks and
/// the statistics are accumulated in `run` rather than returned, so a scan of
/// several roots shares one task budget and one summary.
fn scan_files(
    options: &ScanOptions<'_>,
    dir_canonical: &Path,
    mappings: &[(&'static str, &'static str)],
    interrupt: Option<&AtomicBool>,
    root: Option<&str>,
    run: &mut Run,
) -> Result<(), AppError> {
    let glob_matcher = compile_glob(options.glob)?;

    let Run { tasks, stats } = run;
    let matcher = RegexMatcher::new(
        r"(?m)(^[#*]+\s+(TODO|DONE)\s|DEADLINE:|SCHEDULED:|CREATED:|CLOSED:|CLOCK:)",
    )
    .map_err(|e| AppError::Regex(e.to_string()))?;

    // Defense-in-depth: refuse to follow symlinks and stay within the chosen
    // filesystem. Pass `dir_canonical` (absolute) so every emitted path is an
    // absolute descendant of the root, which lets `strip_prefix(dir_canonical)`
    // succeed downstream for both glob matching and display-path computation.
    // Using the caller's (often relative) path would silently break
    // multi-segment glob patterns like `notes/*.md`.
    let walker = WalkBuilder::new(dir_canonical)
        .standard_filters(true)
        .follow_links(false)
        .same_file_system(true)
        .build();

    // Reuse one Searcher and one read buffer across the entire walk. Both are
    // designed to be cleared and reused; allocating them per file added a
    // monotonic cost that scaled with tree size for no gain.
    let mut searcher = Searcher::new();
    let mut buf: Vec<u8> = Vec::with_capacity(READ_BUF_INITIAL_CAP);

    for result in walker {
        // A SIGINT/SIGTERM trips the flag; bail out *before* opening the next
        // file so the partial summary is consistent with what was actually
        // processed. `Relaxed` is sufficient — the only writer is the signal
        // handler, and we re-check on every iteration, so there is no need
        // for ordering with respect to other reads/writes here.
        if interrupt.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            stats.interrupted = true;
            break;
        }
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
            Err(e) => {
                stats.files_failed_read += 1;
                stats.record_failed_path(&path.display().to_string());
                // The path is surfaced in the aggregated summary warn
                // (see ProcessingStats::print_summary). Keep the
                // underlying cause at debug level so `-vv` can explain
                // *why* a path failed without re-flooding the default
                // warn stream that the O5 aggregation deliberately
                // quietened (2026-05-25 review, m3 / error-handling).
                tracing::debug!(file = %path.display(), error = %e, "file read failed; skipping");
                continue;
            }
        }

        let mut found = false;
        if let Err(e) = searcher.search_slice(&matcher, &buf, FoundSink { found: &mut found }) {
            stats.files_failed_search += 1;
            stats.record_failed_path(&path.display().to_string());
            tracing::debug!(file = %path.display(), error = %e, "content search failed; skipping");
            continue;
        }

        if !found {
            continue;
        }

        let content = match std::str::from_utf8(&buf) {
            Ok(s) => s,
            Err(e) => {
                stats.files_not_utf8 += 1;
                stats.record_failed_path(&path.display().to_string());
                tracing::debug!(file = %path.display(), error = %e, "file is not valid UTF-8; skipping");
                continue;
            }
        };

        let display_path = if options.absolute_paths {
            path.display().to_string()
        } else {
            // WalkBuilder traverses `dir_canonical`, so every emitted path is
            // an absolute descendant of it; strip_prefix cannot fail unless
            // canonicalize and the walker disagree (a TOCTOU we cannot fix
            // here). The absolute path is the safest fallback for that case.
            match path.strip_prefix(dir_canonical) {
                Ok(rel) => rel.display().to_string(),
                Err(_) => path.display().to_string(),
            }
        };

        // A path that is not valid UTF-8 (arbitrary bytes on Linux, unpaired
        // surrogates on Windows) was just rendered lossily into `display_path`
        // via `Path::display`, which substitutes U+FFFD for the invalid bytes.
        // The file is still processed, but the `file` field cannot round-trip,
        // so warn once per run and count it (ADR-0019). `to_str().is_none()` is
        // the precise signal: it distinguishes a genuinely non-UTF-8 path from
        // a valid path that merely happens to contain a literal U+FFFD.
        if path.to_str().is_none() {
            stats.note_nonutf8_path(&display_path);
        }

        // Wrap parsing in a span so every debug!/trace! emitted by the parser,
        // timestamp extractor, and clock extractor inherits `file` automatically.
        // Without this, multi-file runs at `-vv` produce a soup of messages
        // without any way to tie a warning back to the file it came from. The
        // key is `file` (not `path`) so the span agrees with the parser events
        // and the `Task.file` output field — one path, one key (2026-05-25
        // review, O3).
        let span = tracing::debug_span!("file", file = %display_path);
        let extracted = span.in_scope(|| {
            extract_tasks_with_counter(
                Path::new(&display_path),
                content,
                mappings,
                options.max_tasks,
                &mut stats.ts_warnings_emitted,
                &mut stats.prop_warnings_emitted,
            )
        });
        tasks.extend(extracted.into_iter().map(|mut task| {
            task.root = root.map(str::to_string);
            task
        }));
        stats.files_processed += 1;

        if tasks.len() >= options.max_tasks {
            tasks.truncate(options.max_tasks);
            stats.max_tasks_reached = true;
            break;
        }
    }

    Ok(())
}

/// Read up to `cap` bytes from `path` into `buf`, clearing `buf` first.
///
/// Defense-in-depth against TOCTOU: we cannot trust a prior `fs::metadata`
/// call because the file may have grown (or been swapped out for a symlink
/// target on a different filesystem) between the metadata read and the content
/// read. Reading `cap + 1` bytes lets us detect overruns without first asking
/// the filesystem how large the file claims to be.
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

impl Sink for FoundSink<'_> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, _mat: &SinkMatch) -> Result<bool, Self::Error> {
        *self.found = true;
        Ok(false)
    }
}

/// Compile a glob pattern into a `globset::GlobMatcher`. Empty patterns and
/// `*.` (extension-less) are rejected for parity with previous behaviour.
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
        assert!(
            !ok,
            "expected false for file exceeding cap (read {} bytes)",
            buf.len()
        );
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
