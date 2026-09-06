//! Conventions the sources themselves have to keep.
//!
//! The one guarded here is the doc comment: when a table is lifted out of a
//! function into a constant, the description that belonged to the rule tends
//! to stay where it was and get copied onto the new item, leaving two
//! verbatim copies that drift apart at the first edit of either. That is what
//! happened to the weekday description in `src/phrase.rs`, where the same
//! three lines stood on `RU_WEEKDAYS` and on `weekday_of` at once.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sources() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![project_root().join("src")];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("src is readable") {
            let path = entry.expect("entry is readable").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                found.push(path);
            }
        }
    }
    found.sort();
    assert!(!found.is_empty(), "no sources found under src/");
    found
}

/// Every run of `///` lines of the given file, keyed by its text and carrying
/// the line each copy starts on. Single-line runs are left out: a one-line
/// description of a table ("The Russian months.") is said the same way in
/// several places on purpose.
fn doc_blocks(text: &str) -> HashMap<String, Vec<usize>> {
    let mut blocks: HashMap<String, Vec<usize>> = HashMap::new();
    let mut current: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut close = |current: &mut Vec<&str>, start: usize| {
        if current.len() > 1 {
            blocks.entry(current.join("\n")).or_default().push(start);
        }
        current.clear();
    };
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("///") {
            if current.is_empty() {
                start = n + 1;
            }
            current.push(trimmed);
        } else {
            close(&mut current, start);
        }
    }
    close(&mut current, start);
    blocks
}

#[test]
fn no_doc_comment_is_written_twice_in_the_same_file() {
    let mut copies = Vec::new();
    for path in sources() {
        let text = fs::read_to_string(&path).expect("source is readable");
        for (block, lines) in doc_blocks(&text) {
            if lines.len() > 1 {
                let name = path
                    .strip_prefix(project_root())
                    .unwrap_or(Path::new("src"))
                    .display()
                    .to_string();
                let first = block.lines().next().unwrap_or_default().to_string();
                copies.push(format!("{name}: lines {lines:?} say {first}"));
            }
        }
    }
    copies.sort();
    assert!(
        copies.is_empty(),
        "the same doc comment is written more than once; describe each item in \
         its own words, or leave the description on the one item it is about:\n{}",
        copies.join("\n")
    );
}

#[test]
fn the_guard_sees_a_doc_comment_written_twice() {
    let text = "\
/// One line.
/// A second line.
const A: u8 = 1;

/// One line.
/// A second line.
fn b() {}
";
    let blocks = doc_blocks(text);
    let repeated: Vec<_> = blocks.values().filter(|lines| lines.len() > 1).collect();
    assert_eq!(repeated, vec![&vec![1, 5]]);
}
