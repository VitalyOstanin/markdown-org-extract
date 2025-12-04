mod agenda;
mod cli;
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
use crate::parser::extract_tasks;
use crate::render::{render_html, render_markdown};
use crate::types::{ProcessingStats, MAX_FILE_SIZE};

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

/// Main application logic with proper error handling
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mappings = get_weekday_mappings(&cli.locale);

    // Validate directory exists
    if !cli.dir.exists() {
        return Err(format!("Directory does not exist: {}", cli.dir.display()).into());
    }
    if !cli.dir.is_dir() {
        return Err(format!("Path is not a directory: {}", cli.dir.display()).into());
    }

    let mut tasks = Vec::new();
    let mut stats = ProcessingStats::default();
    let matcher = RegexMatcher::new(r"(?m)^[#*]+\s+(TODO|DONE)\s")
        .map_err(|e| format!("Failed to create regex matcher: {e}"))?;

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

        // Check file size before processing
        if let Ok(metadata) = fs::metadata(path) {
            if metadata.len() > MAX_FILE_SIZE {
                eprintln!(
                    "Warning: Skipping large file {} ({} bytes, max {} bytes)",
                    path.display(),
                    metadata.len(),
                    MAX_FILE_SIZE
                );
                stats.files_skipped_size += 1;
                continue;
            }
        }

        let mut found = false;
        let mut searcher = Searcher::new();
        let search_result = searcher.search_path(&matcher, path, FoundSink { found: &mut found });

        if let Err(e) = search_result {
            eprintln!("Warning: Failed to search {}: {}", path.display(), e);
            stats.files_failed_search += 1;
            continue;
        }

        if found {
            match fs::read_to_string(path) {
                Ok(content) => {
                    tasks.extend(extract_tasks(path, &content, &mappings));
                    stats.files_processed += 1;
                }
                Err(e) => {
                    eprintln!("Warning: Failed to read {}: {}", path.display(), e);
                    stats.files_failed_read += 1;
                }
            }
        }
    }

    stats.print_summary();

    tasks = filter_agenda(
        tasks,
        &cli.agenda,
        cli.date.as_deref(),
        cli.from.as_deref(),
        cli.to.as_deref(),
        &cli.tz,
    )?;

    let output = match cli.format.as_str() {
        "json" => serde_json::to_string_pretty(&tasks)
            .map_err(|e| format!("Failed to serialize to JSON: {e}"))?,
        "md" => render_markdown(&tasks),
        "html" => render_html(&tasks),
        _ => return Err(format!("Invalid format: {}", cli.format).into()),
    };

    if let Some(out_path) = cli.output {
        fs::write(&out_path, output)
            .map_err(|e| format!("Failed to write to {}: {}", out_path.display(), e))?;
    } else {
        io::stdout()
            .write_all(output.as_bytes())
            .map_err(|e| format!("Failed to write to stdout: {e}"))?;
    }

    Ok(())
}

/// Sink for grep-searcher to detect if pattern was found
struct FoundSink<'a> {
    found: &'a mut bool,
}

impl<'a> Sink for FoundSink<'a> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, _mat: &SinkMatch) -> Result<bool, Self::Error> {
        *self.found = true;
        Ok(false) // Stop after first match
    }
}

/// Check if file path matches glob pattern
///
/// # Errors
/// Returns error if pattern is invalid
fn matches_glob(path: &Path, pattern: &str) -> Result<bool, Box<dyn std::error::Error>> {
    if let Some(ext) = pattern.strip_prefix("*.") {
        if ext.is_empty() {
            return Err("Invalid glob pattern: extension cannot be empty".into());
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
