//! Tests for the library API.
//!
//! The CLI is covered by the other integration tests; these exercise the
//! Rust surface that embedders use instead of spawning the binary. The
//! motivating consumer is an Android build, where processes cannot be
//! spawned at all and the same extraction has to run in-process.

use std::fs;
use std::sync::atomic::AtomicBool;

use markdown_org_extract::{
    filter_agenda, scan_directory, AgendaDates, AgendaOutput, AgendaScope, AppError, ScanOptions,
};

fn write_vault(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (name, body) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&path, body).expect("write file");
    }
    dir
}

const ONE_TASK: &str = "# TODO Write the report\n`SCHEDULED: <2026-03-02 Mon>`\n";

#[test]
fn scan_directory_extracts_tasks_without_spawning_a_process() {
    let vault = write_vault(&[("notes.md", ONE_TASK)]);

    let outcome = scan_directory(vault.path(), &ScanOptions::default(), None).expect("scan");

    assert_eq!(outcome.tasks.len(), 1, "expected exactly one task");
    let task = &outcome.tasks[0];
    assert_eq!(task.heading, "Write the report");
    assert_eq!(task.timestamp_date.as_deref(), Some("2026-03-02"));
    assert_eq!(outcome.stats.files_processed, 1);
}

#[test]
fn scan_directory_reports_paths_relative_to_the_scanned_root_by_default() {
    let vault = write_vault(&[("inbox/notes.md", ONE_TASK)]);

    let outcome = scan_directory(vault.path(), &ScanOptions::default(), None).expect("scan");

    let file = &outcome.tasks[0].file;
    assert!(
        !std::path::Path::new(file).is_absolute(),
        "expected a relative path, got {file}"
    );
    assert!(file.ends_with("notes.md"), "unexpected path: {file}");
}

#[test]
fn scan_directory_honours_absolute_paths() {
    let vault = write_vault(&[("notes.md", ONE_TASK)]);
    let options = ScanOptions {
        absolute_paths: true,
        ..ScanOptions::default()
    };

    let outcome = scan_directory(vault.path(), &options, None).expect("scan");

    assert!(
        std::path::Path::new(&outcome.tasks[0].file).is_absolute(),
        "expected an absolute path, got {}",
        outcome.tasks[0].file
    );
}

#[test]
fn scan_directory_applies_the_glob_filter() {
    let vault = write_vault(&[("keep.md", ONE_TASK), ("skip.txt", ONE_TASK)]);

    let outcome = scan_directory(vault.path(), &ScanOptions::default(), None).expect("scan");

    assert_eq!(outcome.tasks.len(), 1, "only the .md file must be scanned");
    assert!(outcome.tasks[0].file.ends_with("keep.md"));
}

#[test]
fn scan_directory_caps_the_task_count() {
    let many = (0..10)
        .map(|i| format!("# TODO Task {i}\n"))
        .collect::<String>();
    let vault = write_vault(&[("notes.md", many.as_str())]);
    let options = ScanOptions {
        max_tasks: 3,
        ..ScanOptions::default()
    };

    let outcome = scan_directory(vault.path(), &options, None).expect("scan");

    assert_eq!(outcome.tasks.len(), 3);
    assert!(outcome.stats.max_tasks_reached, "cap must be reported");
}

#[test]
fn scan_directory_rejects_a_missing_directory() {
    let vault = write_vault(&[]);
    let missing = vault.path().join("does-not-exist");

    let err = scan_directory(&missing, &ScanOptions::default(), None).expect_err("must fail");

    assert!(matches!(err, AppError::InvalidDirectory(_)), "got {err:?}");
}

#[test]
fn scan_directory_stops_when_the_interrupt_flag_is_already_set() {
    let vault = write_vault(&[("notes.md", ONE_TASK)]);
    let interrupt = AtomicBool::new(true);

    let outcome =
        scan_directory(vault.path(), &ScanOptions::default(), Some(&interrupt)).expect("scan");

    assert!(outcome.stats.interrupted, "interruption must be reported");
    assert!(outcome.tasks.is_empty(), "no file may be read after a stop");
}

#[test]
fn scan_and_filter_compose_into_an_agenda() {
    let vault = write_vault(&[("notes.md", ONE_TASK)]);
    let outcome = scan_directory(vault.path(), &ScanOptions::default(), None).expect("scan");

    let agenda = filter_agenda(
        outcome.tasks,
        AgendaScope::Day,
        AgendaDates {
            date: Some("2026-03-02"),
            current_date: Some("2026-03-02"),
            ..AgendaDates::default()
        },
        "Europe/Moscow",
        false,
        false,
        true,
    )
    .expect("agenda");

    match agenda {
        AgendaOutput::Days(days) => {
            assert_eq!(days.len(), 1, "day scope must yield exactly one day");
            assert_eq!(days[0].date, "2026-03-02");
        }
        AgendaOutput::Tasks(_) => panic!("day scope must not produce a flat task list"),
    }
}

#[test]
fn holiday_calendar_is_reachable_from_the_library() {
    let calendar = markdown_org_extract::HolidayCalendar::global();

    // 1 January is a public holiday in the bundled Russian calendar; the
    // point of the assertion is that the calendar is reachable and loaded,
    // not the specific policy, which ADR-0008 fixes elsewhere.
    let holidays = calendar.get_holidays_for_year(2026);
    assert!(!holidays.is_empty(), "bundled calendar must not be empty");
}

#[test]
fn renderers_are_reachable_from_the_library() {
    let vault = write_vault(&[("notes.md", ONE_TASK)]);
    let outcome = scan_directory(vault.path(), &ScanOptions::default(), None).expect("scan");

    let markdown = markdown_org_extract::render_markdown(&outcome.tasks);

    assert!(
        markdown.contains("Write the report"),
        "rendered output missing the heading: {markdown}"
    );
}
