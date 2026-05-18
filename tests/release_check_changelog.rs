//! Integration tests for `scripts/check-changelog.sh`.
//!
//! The script is invoked from `.github/workflows/release.yml` to refuse a
//! release where the new `## [X.Y.Z]` section is missing or where the
//! `## [Unreleased]` section still carries entries that should have been
//! moved over. Tests drive it through `bash` with a temporary fixture so we
//! don't depend on the project's real `CHANGELOG.md`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;

fn script_path() -> PathBuf {
    // CARGO_MANIFEST_DIR is the crate root regardless of how the test runner
    // is invoked, so the same path works locally and in CI.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("check-changelog.sh")
}

fn run_check(changelog: &str, version: &str) -> (i32, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("CHANGELOG.md");
    fs::write(&path, changelog).unwrap();

    let output = Command::new("bash")
        .arg(script_path())
        .arg(version)
        .env("CHANGELOG", &path)
        .output()
        .expect("invoke script");

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (code, stderr)
}

const TEMPLATE_OK: &str = "\
# Changelog

## [Unreleased]

_No user-visible changes yet._

## [0.2.3] — 2026-05-18

### Added

- new flag --foo

## [0.2.2] — 2026-05-17

### Fixed

- bug
";

#[test]
fn passes_when_version_section_exists_and_unreleased_is_placeholder() {
    let (code, stderr) = run_check(TEMPLATE_OK, "0.2.3");
    assert_eq!(code, 0, "expected success; stderr: {stderr}");
}

#[test]
fn passes_when_unreleased_is_empty_no_placeholder() {
    let cl = "\
## [Unreleased]

## [0.2.3] — 2026-05-18

### Added

- new flag
";
    let (code, stderr) = run_check(cl, "0.2.3");
    assert_eq!(code, 0, "expected success; stderr: {stderr}");
}

#[test]
fn fails_when_version_section_missing() {
    // [Unreleased] is clean, but the developer forgot to add the new
    // ## [0.2.3] section — release notes extraction would be empty.
    let cl = "\
## [Unreleased]

_No user-visible changes yet._

## [0.2.2] — 2026-05-17

### Fixed

- bug
";
    let (code, stderr) = run_check(cl, "0.2.3");
    assert_ne!(code, 0, "expected failure for missing section");
    assert!(
        stderr.contains("0.2.3"),
        "stderr should mention the missing version: {stderr}"
    );
}

#[test]
fn fails_when_unreleased_still_has_entries() {
    // The new ## [0.2.3] section exists, but real changes are still in
    // [Unreleased] — they would never make it into release notes.
    let cl = "\
## [Unreleased]

### Fixed

- forgot to move this entry

## [0.2.3] — 2026-05-18

### Added

- new flag
";
    let (code, stderr) = run_check(cl, "0.2.3");
    assert_ne!(code, 0, "expected failure for non-empty Unreleased");
    assert!(
        stderr.contains("Unreleased"),
        "stderr should explain that Unreleased is dirty: {stderr}"
    );
}

#[test]
fn fails_when_changelog_missing() {
    let dir = tempdir().unwrap();
    let missing = dir.path().join("nope.md");

    let output = Command::new("bash")
        .arg(script_path())
        .arg("0.2.3")
        .env("CHANGELOG", &missing)
        .output()
        .expect("invoke script");

    assert_ne!(output.status.code().unwrap_or(-1), 0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {stderr}");
}

#[test]
fn fails_without_version_argument() {
    let output = Command::new("bash")
        .arg(script_path())
        .output()
        .expect("invoke script");

    assert_ne!(output.status.code().unwrap_or(-1), 0);
}

#[test]
fn matches_version_with_dots_literally_not_as_regex() {
    // The script must escape '.' when looking for "## [VERSION]" — otherwise
    // a section like "## [0X2X3]" would falsely match version "0.2.3".
    let cl = "\
## [Unreleased]

## [0X2X3] — placeholder

### Added

- nothing
";
    let (code, stderr) = run_check(cl, "0.2.3");
    assert_ne!(
        code, 0,
        "must not treat '.' as regex wildcard; stderr: {stderr}"
    );
}

#[test]
fn unreleased_with_blank_lines_only_counts_as_empty() {
    let cl = "\
## [Unreleased]



## [0.2.3] — 2026-05-18

- entry
";
    let (code, stderr) = run_check(cl, "0.2.3");
    assert_eq!(code, 0, "blank-only Unreleased must pass; stderr: {stderr}");
}
