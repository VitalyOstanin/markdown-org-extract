//! The cost of reading an entry's `MOVED` lines has to stay linear in their
//! number.
//!
//! Nothing bounds how many an entry may carry: `MAX_DIAGNOSTIC_ITEMS` bounds
//! the warnings, not the lines, and a 10 MB file leaves room for hundreds of
//! thousands of them at ~36 bytes each. A day already held used to be looked
//! for by walking the days kept so far, which is quadratic and was measured
//! at 271 s for 250 000 lines against 0.87 s when the walk stopped on the
//! first comparison.
//!
//! The guard compares two inputs of the same size rather than an input
//! against a stopwatch: one names a different day on every line, the other
//! names one day over and over. They parse the same number of lines and
//! differ only in how the day already held is looked for, so a slow machine
//! slows both and the comparison says the same thing everywhere.

use markdown_org_extract::extract_tasks;
use std::path::Path;
use std::time::{Duration, Instant};

const LINES: usize = 32_000;
/// Reading distinct days measured 0.85--0.97 of reading one day over and over
/// once the day held is looked up rather than walked to; the walk that was
/// there before measured 3.5, and the further the size goes the higher it is.
const MOST_THE_RATIO_MAY_BE: f64 = 2.0;

/// The series repeats daily from the first day the generated moves name, so
/// that every one of them is an occurrence the entry has and the line is read
/// rather than refused for naming a day off the series. What is being measured
/// is the cost of the lines that are kept.
fn an_entry_moving(lines: usize, day_of: fn(usize) -> String) -> String {
    let mut content = String::from("### TODO T\n`SCHEDULED: <1900-01-01 Mon +1d>`\n");
    for i in 0..lines {
        let day = day_of(i);
        content.push_str(&format!("`MOVED: {day} -> <2026-08-22 Sat 18:00>`\n"));
    }
    content
}

/// A day of its own for every line, so that none is refused as a repeat --
/// the repeat is the case a walk leaves early on, and it is the case that
/// hides the cost.
fn a_day_of_its_own(i: usize) -> String {
    let day = 1 + (i % 28);
    let month = 1 + (i / 28) % 12;
    let year = 1900 + (i / (28 * 12));
    format!("{year:04}-{month:02}-{day:02}")
}

fn one_day_over_and_over(_: usize) -> String {
    String::from("1900-01-01")
}

fn read_and_time(content: &str) -> (usize, Duration) {
    let started = Instant::now();
    let tasks = extract_tasks(Path::new("scale.md"), content, &[], 100);
    let took = started.elapsed();
    let kept = tasks[0].moved_occurrences.as_deref().map_or(0, <[_]>::len);
    (kept, took)
}

#[test]
fn reading_distinct_days_costs_about_what_reading_one_day_costs() {
    let distinct = an_entry_moving(LINES, a_day_of_its_own);
    let repeated = an_entry_moving(LINES, one_day_over_and_over);

    let (repeated_kept, repeated_took) = read_and_time(&repeated);
    let (distinct_kept, distinct_took) = read_and_time(&distinct);

    assert_eq!(distinct_kept, LINES, "every day named once is kept");
    assert_eq!(repeated_kept, 1, "one day named many times is held once");

    let ratio = distinct_took.as_secs_f64() / repeated_took.as_secs_f64().max(f64::MIN_POSITIVE);
    assert!(
        ratio < MOST_THE_RATIO_MAY_BE,
        "reading {LINES} moved lines naming {LINES} days took {ratio:.1} times as long as \
         reading {LINES} lines naming one day ({distinct_took:?} against {repeated_took:?}); \
         the two parse the same lines, so the difference is the day already held being \
         looked for by walking the days kept so far"
    );
}

#[test]
fn a_day_named_twice_is_still_refused_among_many() {
    let mut content = an_entry_moving(4_000, a_day_of_its_own);
    content.push_str("`MOVED: 1900-01-01 -> <2026-09-01 Tue 09:00>`\n");

    let tasks = extract_tasks(Path::new("scale.md"), &content, &[], 100);
    let moved = tasks[0]
        .moved_occurrences
        .as_deref()
        .expect("the entry moves occurrences");

    assert_eq!(moved.len(), 4_000, "the repeat is dropped, not kept");
    assert_eq!(
        moved[0].to, "2026-08-22",
        "the first move of the day stands"
    );
}
