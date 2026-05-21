//! Integration tests for `scripts/check-changelog.sh`.
//!
//! The script is invoked from `.github/workflows/release.yml` to refuse a
//! release where the new `## [X.Y.Z]` section is missing or where the
//! `## [Unreleased]` section still carries entries that should have been
//! moved over. Tests drive it through `bash` with a temporary fixture so we
//! don't depend on the project's real `CHANGELOG.md`.
//!
//! Unix-only: the script is a POSIX bash script that runs only on the
//! ubuntu-24.04 release runner. On Windows CI, `Command::new("bash")` is
//! unreliable (PATH/EOL issues with Git for Windows), and the script is
//! never executed there in production, so the tests have no Windows
//! behaviour to defend. Gating the entire file with `cfg(unix)` keeps the
//! Windows CI matrix green without weakening Linux/macOS coverage.

#![cfg(unix)]

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
    // a section like "## [0X2X3]" would falsely match version "0.2.3". The
    // date is well-formed so this test isolates the regex-escape contract;
    // a missing-date test lives separately below.
    let cl = "\
## [Unreleased]

## [0X2X3] — 2026-05-18

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

#[test]
fn fails_when_release_section_has_no_date() {
    // Releasing with `## [0.2.3]` but no `— YYYY-MM-DD` suffix must be
    // refused: the gh-release notes extractor and the README's changelog
    // links both rely on the date being present, and a missing date is a
    // sign that the version section was hand-edited but not finished.
    let cl = "\
## [Unreleased]

## [0.2.3]

### Added

- new flag
";
    let (code, stderr) = run_check(cl, "0.2.3");
    assert_ne!(code, 0, "expected failure for missing date");
    assert!(
        stderr.contains("date") || stderr.contains("YYYY-MM-DD"),
        "stderr should explain that a date is required: {stderr}"
    );
}

#[test]
fn fails_when_release_section_uses_ascii_hyphen_not_em_dash() {
    // CHANGELOG.md uses an em-dash (U+2014) between version and date in
    // every existing entry. An ASCII hyphen on the released version is
    // either a typo or a forked tool that does not match the project's
    // convention — either way, refuse to publish so the heading style
    // stays uniform across the history.
    let cl = "\
## [Unreleased]

## [0.2.3] - 2026-05-18

### Added

- new flag
";
    let (code, stderr) = run_check(cl, "0.2.3");
    assert_ne!(code, 0, "ASCII hyphen must be rejected; em-dash required");
    assert!(
        stderr.contains("em-dash") || stderr.contains("—") || stderr.contains("date"),
        "stderr should hint at the missing em-dash: {stderr}"
    );
}

#[test]
fn fails_when_version_is_not_monotonic() {
    // The new ## [<version>] section must be the FIRST version heading
    // after ## [Unreleased]. If a stale section (e.g. an aborted release
    // for 0.2.4) sits between Unreleased and 0.2.3, that signals the
    // CHANGELOG is out of order and cargo-publish would ship release
    // notes for the wrong version. Refuse.
    let cl = "\
## [Unreleased]

_No user-visible changes yet._

## [0.2.4] — 2026-05-19

### Added

- a later flag

## [0.2.3] — 2026-05-18

### Added

- the flag we are releasing now
";
    let (code, stderr) = run_check(cl, "0.2.3");
    assert_ne!(code, 0, "non-monotonic CHANGELOG must be refused");
    assert!(
        stderr.contains("monotonic")
            || stderr.contains("immediately")
            || stderr.contains("0.2.4"),
        "stderr should name the offending section or the ordering rule: {stderr}"
    );
}

#[test]
fn monotonicity_only_inspects_the_first_section_after_unreleased() {
    // 0.2.4 below is older but appears AFTER 0.2.3 — that's the normal
    // chronological layout and must pass even though versions are
    // non-decreasing further down. The monotonicity rule only requires
    // that the FIRST section after Unreleased matches the released
    // version.
    let cl = "\
## [Unreleased]

_No user-visible changes yet._

## [0.2.3] — 2026-05-18

### Added

- the released change

## [0.2.4] — 2026-05-19

### Note

- bogus older section kept for the test
";
    let (code, stderr) = run_check(cl, "0.2.3");
    assert_eq!(
        code, 0,
        "first-section-after-Unreleased rule must pass; stderr: {stderr}"
    );
}
