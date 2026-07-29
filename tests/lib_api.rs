//! Tests for the library API.
//!
//! The CLI is covered by the other integration tests; these exercise the
//! Rust surface that embedders use instead of spawning the binary. The
//! motivating consumer is an Android build, where processes cannot be
//! spawned at all and the same extraction has to run in-process.

use std::fs;
use std::sync::atomic::AtomicBool;

use markdown_org_extract::{
    filter_agenda, parse_heading_line, parse_timestamp_parts, scan_directory, AgendaDates,
    AgendaOutput, AgendaScope, AppError, Priority, ScanOptions, TaskType,
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
fn scan_directory_counts_a_file_that_is_not_utf8_apart_from_a_read_failure() {
    // A note written in a Windows editor and committed to the same
    // repository arrives as CP1251. An embedder showing "N files could not be
    // read" cannot tell the user to convert it unless the reason is reported
    // separately from a genuine IO failure.
    let vault = write_vault(&[("ok.md", ONE_TASK)]);
    // `# TODO Отчёт` with the title in CP1251, and a timestamp so the file
    // reaches the UTF-8 check rather than being filtered out by the search.
    let mut cp1251 = b"# TODO ".to_vec();
    cp1251.extend_from_slice(&[0xCE, 0xF2, 0xF7, 0xB8, 0xF2, b'\n']);
    cp1251.extend_from_slice("`SCHEDULED: <2026-03-02 Mon>`\n".as_bytes());
    fs::write(vault.path().join("cp1251.md"), cp1251).expect("write file");

    let outcome = scan_directory(vault.path(), &ScanOptions::default(), None).expect("scan");

    assert_eq!(outcome.tasks.len(), 1, "only the UTF-8 file has tasks");
    assert_eq!(outcome.stats.files_not_utf8, 1);
    assert_eq!(
        outcome.stats.files_failed_read, 0,
        "an unreadable file and a file in another encoding need different answers"
    );
    assert!(outcome.stats.has_warnings());
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

// `parse_heading_line` exists for editors: it reports where the keyword and
// the priority cookie sit so a caller can replace one token and leave the rest
// of the line byte-for-byte alone. The assertions below are therefore about
// the offsets, not only about the values.

#[test]
fn heading_line_reports_where_the_keyword_and_the_cookie_sit() {
    let line = "## TODO [#A] Write the report";

    let heading = parse_heading_line(line).expect("a heading");

    assert_eq!(heading.level, 2);
    let status = heading.status.expect("a keyword");
    assert_eq!(status.value, TaskType::Todo);
    assert_eq!(&line[status.range], "TODO");
    let priority = heading.priority.expect("a priority");
    assert_eq!(priority.value, Priority::A);
    assert_eq!(&line[priority.range], "[#A]");
    assert_eq!(&line[heading.title_start..], "Write the report");
}

#[test]
fn heading_line_reads_a_heading_without_tokens() {
    let line = "### Just a title";

    let heading = parse_heading_line(line).expect("a heading");

    assert_eq!(heading.level, 3);
    assert!(heading.status.is_none());
    assert!(heading.priority.is_none());
    // A caller inserting a keyword writes at `title_start`, so it has to point
    // past the gap after the hashes rather than at the hashes themselves.
    assert_eq!(&line[heading.title_start..], "Just a title");
}

#[test]
fn heading_line_keeps_the_text_between_the_keyword_and_the_cookie_addressable() {
    // The parser that feeds the agenda drops this text (emacs
    // `org-element--headline-parse-title` does the same). An editor must not:
    // rewriting the line from parts would delete what the user typed.
    let line = "# TODO leftover [#B] Title";

    let heading = parse_heading_line(line).expect("a heading");

    let status = heading.status.expect("a keyword");
    let priority = heading.priority.expect("a priority");
    assert_eq!(&line[status.range.end..priority.range.start], " leftover ");
    assert_eq!(&line[heading.title_start..], "Title");
}

#[test]
fn heading_line_preserves_the_single_l_cancelled_spelling() {
    let line = "# CANCELED Old plan";

    let heading = parse_heading_line(line).expect("a heading");

    let status = heading.status.expect("a keyword");
    assert_eq!(&line[status.range], "CANCELED");
    assert!(matches!(status.value, TaskType::Cancelled(_)));
}

#[test]
fn heading_line_reads_a_numeric_priority() {
    let line = "# TODO [#12] Task";

    let heading = parse_heading_line(line).expect("a heading");

    let priority = heading.priority.expect("a priority");
    assert_eq!(priority.value, Priority::Numeric(12));
    assert_eq!(&line[priority.range], "[#12]");
}

#[test]
fn heading_line_rejects_what_is_not_a_heading() {
    assert!(parse_heading_line("plain text").is_none());
    // No gap after the hashes: markdown does not treat this as a heading, and
    // neither may an editor about to rewrite the line.
    assert!(parse_heading_line("#no gap").is_none());
    assert!(parse_heading_line("").is_none());
}

#[test]
fn heading_line_offsets_survive_multibyte_text() {
    let line = "# TODO [#A] Отчёт";

    let heading = parse_heading_line(line).expect("a heading");

    // Byte offsets, not character counts: slicing has to land on a boundary.
    assert_eq!(&line[heading.title_start..], "Отчёт");
}

// `parse_timestamp_parts` is the timestamp counterpart of
// `parse_heading_line`: an editor moving a date has to leave the weekday, the
// time, the repeater and the warning cookie exactly where they are.

#[test]
fn timestamp_parts_report_where_the_date_and_the_weekday_sit() {
    let line = "`SCHEDULED: <2026-07-28 Tue 10:00 +1w>`";

    let parts = parse_timestamp_parts(line).expect("a timestamp");

    assert_eq!(&line[parts.date.clone()], "2026-07-28");
    assert_eq!(&line[parts.weekday.clone().expect("a weekday")], "Tue");
    assert_eq!(&line[parts.whole.clone()], "<2026-07-28 Tue 10:00 +1w>");
    assert!(parts.active);
    let repeater = parts.repeater.expect("a repeater");
    assert_eq!(repeater.canonical(), "+1w");
}

#[test]
fn timestamp_parts_handle_a_timestamp_without_a_weekday() {
    let parts = parse_timestamp_parts("<2026-07-28>").expect("a timestamp");

    assert!(parts.weekday.is_none());
    assert!(parts.repeater.is_none());
}

#[test]
fn timestamp_parts_do_not_mistake_a_time_for_a_weekday() {
    let line = "<2026-07-28 10:00>";

    let parts = parse_timestamp_parts(line).expect("a timestamp");

    assert!(
        parts.weekday.is_none(),
        "the token after the date is a time, not a weekday"
    );
}

#[test]
fn timestamp_parts_read_a_localised_weekday() {
    let line = "`DEADLINE: <2026-07-28 Вт>`";

    let parts = parse_timestamp_parts(line).expect("a timestamp");

    assert_eq!(&line[parts.weekday.clone().expect("a weekday")], "Вт");
    // Byte offsets past a Cyrillic token still land on a boundary.
    assert_eq!(&line[parts.whole.clone()], "<2026-07-28 Вт>");
}

#[test]
fn timestamp_parts_report_the_inactive_form() {
    let parts = parse_timestamp_parts("`CLOSED: [2026-07-28 Tue]`").expect("a timestamp");

    assert!(!parts.active);
}

#[test]
fn timestamp_parts_reject_a_line_without_one() {
    assert!(parse_timestamp_parts("# TODO Water the plants").is_none());
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
