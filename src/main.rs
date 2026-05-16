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
use std::fs;
use std::io::{self, Write};
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
        // Use eprintln directly: tracing may not be initialized if argument parsing failed,
        // and a hard error should always reach the user regardless of `--quiet`.
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), AppError> {
    let cli = Cli::parse();
    cli.init_tracing();

    if let Some(year) = cli.holidays {
        let calendar = holidays::HolidayCalendar::global();
        let holidays = calendar.get_holidays_for_year(year);
        let dates: Vec<String> = holidays
            .iter()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .collect();
        let output = serde_json::to_string_pretty(&dates)?;
        io::stdout().write_all(output.as_bytes())?;
        return Ok(());
    }

    if let Some(ref out_path) = cli.output {
        validate_output_path(out_path)?;
    }

    let mappings = get_weekday_mappings(&cli.locale);

    if !cli.dir.exists() {
        return Err(AppError::InvalidDirectory(format!(
            "Directory does not exist: {}",
            cli.dir.display()
        )));
    }
    if !cli.dir.is_dir() {
        return Err(AppError::InvalidDirectory(format!(
            "Path is not a directory: {}",
            cli.dir.display()
        )));
    }

    let dir_canonical = fs::canonicalize(&cli.dir)
        .map_err(|e| AppError::InvalidDirectory(format!("{}: {e}", cli.dir.display())))?;

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

    // Defense-in-depth: refuse to follow symlinks and stay within the chosen filesystem.
    let walker = WalkBuilder::new(&cli.dir)
        .standard_filters(true)
        .follow_links(false)
        .same_file_system(true)
        .build();

    for result in walker {
        let entry = result?;
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let path = entry.path();

        if !glob_match(&glob_matcher, path, &dir_canonical) {
            continue;
        }

        match fs::metadata(path) {
            Ok(metadata) => {
                if metadata.len() > MAX_FILE_SIZE {
                    stats.files_skipped_size += 1;
                    continue;
                }
            }
            Err(_) => {
                stats.files_failed_read += 1;
                stats.record_failed_path(&path.display().to_string());
                continue;
            }
        }

        // Read file once and reuse the buffer for both the keyword pre-filter and the parser.
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(_) => {
                stats.files_failed_read += 1;
                stats.record_failed_path(&path.display().to_string());
                continue;
            }
        };

        let mut found = false;
        let mut searcher = Searcher::new();
        if searcher
            .search_slice(&matcher, &bytes, FoundSink { found: &mut found })
            .is_err()
        {
            stats.files_failed_search += 1;
            stats.record_failed_path(&path.display().to_string());
            continue;
        }

        if !found {
            continue;
        }

        let content = match String::from_utf8(bytes) {
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
            match path
                .strip_prefix(&dir_canonical)
                .or_else(|_| path.strip_prefix(&cli.dir))
            {
                Ok(rel) => rel.display().to_string(),
                Err(_) => path.display().to_string(),
            }
        };

        let extracted = extract_tasks(Path::new(&display_path), &content, &mappings, cli.max_tasks);
        tasks.extend(extracted);
        stats.files_processed += 1;

        if tasks.len() >= cli.max_tasks {
            tasks.truncate(cli.max_tasks);
            stats.max_tasks_reached = true;
            break;
        }
    }

    if stats.has_warnings() {
        stats.print_summary();
    }

    let agenda_output = filter_agenda(
        tasks,
        cli.get_agenda_mode(),
        cli.date.as_deref(),
        cli.from.as_deref(),
        cli.to.as_deref(),
        &cli.tz,
        cli.current_date.as_deref(),
    )?;

    let output = match cli.format {
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

    if let Some(out_path) = cli.output {
        fs::write(&out_path, output)?;
    } else {
        io::stdout().write_all(output.as_bytes())?;
    }

    Ok(())
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

    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return Err(AppError::InvalidOutput(format!(
                "refusing to overwrite symlink: {}",
                path.display()
            )));
        }
    }

    Ok(())
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
        .map_err(|e| AppError::InvalidGlob(format!("invalid pattern '{pattern}': {e}")))
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
    fn validate_output_rejects_missing_parent() {
        let p = PathBuf::from("/nonexistent_definitely_xyz/out.json");
        assert!(matches!(
            validate_output_path(&p),
            Err(AppError::InvalidOutput(_))
        ));
    }
}
