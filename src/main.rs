mod agenda;
mod cli;
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
use std::path::Path;

use crate::agenda::filter_agenda;
use crate::cli::{get_weekday_mappings, Cli};
use crate::error::AppError;
use crate::format::OutputFormat;
use crate::parser::extract_tasks;
use crate::render::{render_html, render_markdown};
use crate::types::{ProcessingStats, MAX_FILE_SIZE};

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), AppError> {
    let cli = Cli::parse();

    if let Some(year) = cli.holidays {
        let calendar = holidays::HolidayCalendar::load()
            .map_err(|e| AppError::Io(io::Error::new(io::ErrorKind::Other, e.to_string())))?;
        let holidays = calendar.get_holidays_for_year(year);
        let dates: Vec<String> = holidays.iter()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .collect();
        let output = serde_json::to_string_pretty(&dates)?;
        io::stdout().write_all(output.as_bytes())?;
        return Ok(());
    }

    let mappings = get_weekday_mappings(&cli.locale);

    if !cli.dir.exists() {
        return Err(AppError::InvalidDirectory(format!("Directory does not exist: {}", cli.dir.display())));
    }
    if !cli.dir.is_dir() {
        return Err(AppError::InvalidDirectory(format!("Path is not a directory: {}", cli.dir.display())));
    }

    let mut tasks = Vec::new();
    let mut stats = ProcessingStats::default();
    let matcher = RegexMatcher::new(r"(?m)(^[#*]+\s+(TODO|DONE)\s|DEADLINE:|SCHEDULED:|CREATED:|CLOSED:)")
        .map_err(|e| AppError::Regex(e.to_string()))?;

    let walker = WalkBuilder::new(&cli.dir).standard_filters(true).build();

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
                continue;
            }
        }

        let mut found = false;
        let mut searcher = Searcher::new();
        if searcher.search_path(&matcher, path, FoundSink { found: &mut found }).is_err() {
            stats.files_failed_search += 1;
            continue;
        }

        if found {
            match fs::read_to_string(path) {
                Ok(content) => {
                    tasks.extend(extract_tasks(path, &content, &mappings));
                    stats.files_processed += 1;
                }
                Err(_) => {
                    stats.files_failed_read += 1;
                }
            }
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
            return Err(AppError::InvalidGlob("extension cannot be empty".to_string()));
        }
        return Ok(path.extension().and_then(|e| e.to_str()) == Some(ext));
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
    }
}
