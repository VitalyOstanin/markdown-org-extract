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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let mappings = get_weekday_mappings(&cli.locale);

    let mut tasks = Vec::new();
    let matcher = RegexMatcher::new(r"(?m)^[#*]+\s+(TODO|DONE)\s")?;
    
    let walker = WalkBuilder::new(&cli.dir)
        .standard_filters(true)
        .build();
    
    for result in walker {
        let entry = result?;
        if !entry.file_type().map_or(false, |ft| ft.is_file()) {
            continue;
        }
        
        let path = entry.path();
        
        if !matches_glob(path, &cli.glob) {
            continue;
        }
        
        let mut found = false;
        let mut searcher = Searcher::new();
        let _ = searcher.search_path(
            &matcher,
            path,
            FoundSink { found: &mut found }
        );
        
        if found {
            if let Ok(content) = fs::read_to_string(path) {
                tasks.extend(extract_tasks(&path.to_path_buf(), &content, &mappings));
            }
        }
    }

    tasks = filter_agenda(
        tasks,
        &cli.agenda,
        cli.date.as_deref(),
        cli.from.as_deref(),
        cli.to.as_deref(),
        &cli.tz,
    )?;

    let output = match cli.format.as_str() {
        "json" => serde_json::to_string_pretty(&tasks)?,
        "md" => render_markdown(&tasks),
        "html" => render_html(&tasks),
        _ => return Err("Invalid format".into()),
    };

    if let Some(out_path) = cli.output {
        fs::write(out_path, output)?;
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

fn matches_glob(path: &Path, pattern: &str) -> bool {
    if pattern == "*.md" {
        return path.extension().map_or(false, |ext| ext == "md");
    }
    
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if pattern.starts_with("*.") {
            let ext = &pattern[2..];
            return file_name.ends_with(ext);
        }
        return file_name == pattern;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use crate::timestamp::*;

    #[test]
    fn test_parse_heading_with_priority() {
        // Tests moved to parser module
    }

    #[test]
    fn test_parse_deadline_with_warning() {
        let ts = "DEADLINE: <2024-12-15 Sun -7d>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.timestamp_type, TimestampType::Deadline);
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 15).unwrap());
        assert_eq!(parsed.warning.unwrap().days, 7);
    }

    #[test]
    fn test_parse_scheduled_with_repeater() {
        let ts = "SCHEDULED: <2024-12-01 Mon +1w>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.timestamp_type, TimestampType::Scheduled);
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 1).unwrap());
        assert!(parsed.repeater.is_some());
        let repeater = parsed.repeater.unwrap();
        assert_eq!(repeater.interval, 1);
    }

    #[test]
    fn test_parse_date_range() {
        let ts = "<2024-12-20 Fri>--<2024-12-22 Sun>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 20).unwrap());
        assert_eq!(parsed.end_date, Some(NaiveDate::from_ymd_opt(2024, 12, 22).unwrap()));
    }

    #[test]
    fn test_parse_timestamp_with_time() {
        let ts = "<2024-12-05 Wed 10:00-12:00>";
        let parsed = parse_org_timestamp(ts, None).unwrap();
        assert_eq!(parsed.date, NaiveDate::from_ymd_opt(2024, 12, 5).unwrap());
        assert_eq!(parsed.time, Some("10:00".to_string()));
        assert_eq!(parsed.end_time, Some("12:00".to_string()));
    }

    #[test]
    fn test_deadline_shows_before_date() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Deadline,
            date: NaiveDate::from_ymd_opt(2024, 12, 15).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: None,
            warning: Some(Warning { days: 7 }),
        };
        
        let target = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap();
        assert!(timestamp_matches_date(&parsed, &target));
        
        let target = NaiveDate::from_ymd_opt(2024, 12, 15).unwrap();
        assert!(timestamp_matches_date(&parsed, &target));
        
        let target = NaiveDate::from_ymd_opt(2024, 12, 7).unwrap();
        assert!(!timestamp_matches_date(&parsed, &target));
        
        let target = NaiveDate::from_ymd_opt(2024, 12, 16).unwrap();
        assert!(!timestamp_matches_date(&parsed, &target));
    }

    #[test]
    fn test_deadline_default_warning() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Deadline,
            date: NaiveDate::from_ymd_opt(2024, 12, 15).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: None,
            warning: None,
        };
        
        let target = NaiveDate::from_ymd_opt(2024, 12, 5).unwrap();
        assert!(timestamp_matches_date(&parsed, &target));
    }

    #[test]
    fn test_scheduled_shows_from_date() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Scheduled,
            date: NaiveDate::from_ymd_opt(2024, 12, 10).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: None,
            warning: None,
        };
        
        let target = NaiveDate::from_ymd_opt(2024, 12, 9).unwrap();
        assert!(!timestamp_matches_date(&parsed, &target));
        
        let target = NaiveDate::from_ymd_opt(2024, 12, 10).unwrap();
        assert!(timestamp_matches_date(&parsed, &target));
        
        let target = NaiveDate::from_ymd_opt(2024, 12, 15).unwrap();
        assert!(timestamp_matches_date(&parsed, &target));
    }

    #[test]
    fn test_repeater_daily() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Scheduled,
            date: NaiveDate::from_ymd_opt(2024, 12, 1).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: Some(Repeater { interval: 2, unit: RepeatUnit::Day }),
            warning: None,
        };
        
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 1).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 3).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 5).unwrap()));
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 2).unwrap()));
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 4).unwrap()));
    }

    #[test]
    fn test_repeater_weekly() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Scheduled,
            date: NaiveDate::from_ymd_opt(2024, 12, 1).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: Some(Repeater { interval: 1, unit: RepeatUnit::Week }),
            warning: None,
        };
        
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 1).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 8).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 15).unwrap()));
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 2).unwrap()));
    }

    #[test]
    fn test_date_range_coverage() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Plain,
            date: NaiveDate::from_ymd_opt(2024, 12, 20).unwrap(),
            time: None,
            end_date: Some(NaiveDate::from_ymd_opt(2024, 12, 22).unwrap()),
            end_time: None,
            repeater: None,
            warning: None,
        };
        
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 19).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 20).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 21).unwrap()));
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 22).unwrap()));
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 23).unwrap()));
    }

    #[test]
    fn test_closed_exact_date_only() {
        let parsed = ParsedTimestamp {
            timestamp_type: TimestampType::Closed,
            date: NaiveDate::from_ymd_opt(2024, 12, 10).unwrap(),
            time: None,
            end_date: None,
            end_time: None,
            repeater: None,
            warning: None,
        };
        
        assert!(timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 10).unwrap()));
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 9).unwrap()));
        assert!(!timestamp_matches_date(&parsed, &NaiveDate::from_ymd_opt(2024, 12, 11).unwrap()));
    }

    #[test]
    fn test_parse_timestamp_fields_scheduled() {
        let mappings = vec![];
        let (ts_type, ts_date, ts_time, ts_end_time) = parse_timestamp_fields("SCHEDULED: <2024-12-10 Tue 14:30>", &mappings);
        assert_eq!(ts_type, Some("SCHEDULED".to_string()));
        assert_eq!(ts_date, Some("2024-12-10".to_string()));
        assert_eq!(ts_time, Some("14:30".to_string()));
        assert_eq!(ts_end_time, None);
    }

    #[test]
    fn test_parse_timestamp_fields_with_time_range() {
        let mappings = vec![];
        let (ts_type, ts_date, ts_time, ts_end_time) = parse_timestamp_fields("<2024-12-04 Mon 10:00-11:00>", &mappings);
        assert_eq!(ts_type, Some("PLAIN".to_string()));
        assert_eq!(ts_date, Some("2024-12-04".to_string()));
        assert_eq!(ts_time, Some("10:00".to_string()));
        assert_eq!(ts_end_time, Some("11:00".to_string()));
    }

    #[test]
    fn test_parse_timestamp_fields_deadline() {
        let mappings = vec![];
        let (ts_type, ts_date, ts_time, ts_end_time) = parse_timestamp_fields("DEADLINE: <2025-12-10 Wed -3d>", &mappings);
        assert_eq!(ts_type, Some("DEADLINE".to_string()));
        assert_eq!(ts_date, Some("2025-12-10".to_string()));
        assert_eq!(ts_time, None);
        assert_eq!(ts_end_time, None);
    }
}
