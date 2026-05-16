mod agenda;
mod cli;
mod clock;
mod error;
mod format;
mod holidays;
mod parser;
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
use crate::types::{ProcessingStats, MAX_FILE_SIZE, MAX_TASKS};

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), AppError> {
    let cli = Cli::parse();

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

    let mut tasks = Vec::new();
    let mut stats = ProcessingStats::default();
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

        if !matches_glob(path, &cli.glob)? {
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

        let extracted = extract_tasks(Path::new(&display_path), &content, &mappings);
        tasks.extend(extracted);
        stats.files_processed += 1;

        if tasks.len() >= MAX_TASKS {
            tasks.truncate(MAX_TASKS);
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

fn matches_glob(path: &Path, pattern: &str) -> Result<bool, AppError> {
    if let Some(ext) = pattern.strip_prefix("*.") {
        if ext.is_empty() {
            return Err(AppError::InvalidGlob(
                "pattern '*.': extension cannot be empty".to_string(),
            ));
        }
        if ext.contains('*') || ext.contains('?') || ext.contains('/') {
            return Err(AppError::InvalidGlob(format!(
                "unsupported pattern '{pattern}': only '*.ext' and exact file names are supported"
            )));
        }
        return Ok(path.extension().and_then(|e| e.to_str()) == Some(ext));
    }

    if pattern.contains('*') || pattern.contains('?') {
        return Err(AppError::InvalidGlob(format!(
            "unsupported pattern '{pattern}': only '*.ext' and exact file names are supported"
        )));
    }

    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        return Ok(file_name == pattern);
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_matches_glob_md() {
        let path = PathBuf::from("test.md");
        assert!(matches_glob(&path, "*.md").unwrap());
    }

    #[test]
    fn test_matches_glob_other_extension() {
        let path = PathBuf::from("test.txt");
        assert!(!matches_glob(&path, "*.md").unwrap());
        assert!(matches_glob(&path, "*.txt").unwrap());
    }

    #[test]
    fn test_matches_glob_exact_name() {
        let path = PathBuf::from("README.md");
        assert!(matches_glob(&path, "README.md").unwrap());
        assert!(!matches_glob(&path, "OTHER.md").unwrap());
    }

    #[test]
    fn test_matches_glob_invalid() {
        let path = PathBuf::from("test.md");
        assert!(matches_glob(&path, "*.").is_err());
        assert!(matches_glob(&path, "**/*.md").is_err());
        assert!(matches_glob(&path, "src/*.md").is_err());
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
