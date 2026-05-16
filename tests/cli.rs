//! End-to-end CLI tests. The binary is invoked through `assert_cmd` against
//! the markdown fixtures in `examples/`, with `--current-date` pinned so the
//! output is deterministic.

use assert_cmd::Command;
use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

fn bin() -> Command {
    Command::cargo_bin("markdown-org-extract").expect("binary should build")
}

#[test]
fn shows_help_with_usage_section() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Usage:"))
        .stdout(contains("--dir"))
        .stdout(contains("--format"));
}

#[test]
fn rejects_nonexistent_dir() {
    bin()
        .args([
            "--dir",
            "/this/path/should/never/exist_xyz",
            "--current-date",
            "2025-12-05",
        ])
        .assert()
        .failure()
        .stderr(contains("Invalid directory"));
}

#[test]
fn examples_directory_emits_json_with_relative_paths() {
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--format",
            "json",
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Output is JSON
    assert!(stdout.starts_with("[") || stdout.starts_with("{"));
    // Relative paths by default — no host filesystem prefix
    assert!(
        !stdout.contains("/home/"),
        "default output must not contain absolute paths: {stdout:.200}"
    );
}

#[test]
fn absolute_paths_flag_emits_full_paths() {
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--absolute-paths",
            "--format",
            "json",
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // With --absolute-paths we should see the path containing "examples/"
    // (regardless of cwd, the prefix is the real working directory).
    assert!(stdout.contains("examples/"));
}

#[test]
fn output_flag_writes_to_file() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("out.json");

    bin()
        .args([
            "--dir",
            "examples",
            "--format",
            "json",
            "--current-date",
            "2025-12-05",
            "--output",
        ])
        .arg(&target)
        .assert()
        .success();

    let content = fs::read_to_string(&target).unwrap();
    assert!(!content.is_empty());
    assert!(content.contains("\"date\""));
}

#[test]
fn output_flag_rejects_symlink() {
    let dir = tempdir().unwrap();
    let real = dir.path().join("real.json");
    let link = dir.path().join("link.json");
    fs::write(&real, "existing").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &link).unwrap();

    #[cfg(unix)]
    {
        bin()
            .args([
                "--dir",
                "examples",
                "--format",
                "json",
                "--current-date",
                "2025-12-05",
                "--output",
            ])
            .arg(&link)
            .assert()
            .failure()
            .stderr(contains("symlink"));
    }
}

#[test]
fn holidays_year_returns_json_array() {
    let out = bin().args(["--holidays", "2026"]).output().expect("run");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.trim().starts_with('['));
    assert!(stdout.contains("2026-01-01"));
}

#[test]
fn invalid_year_rejected() {
    bin().args(["--holidays", "1800"]).assert().failure();
}

#[test]
fn double_star_glob_is_accepted() {
    // Regression: with globset we now support real glob patterns; `**/*.md`
    // is valid and should match recursively.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--glob",
            "**/*.md",
            "--format",
            "json",
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn rejects_malformed_glob() {
    bin()
        .args([
            "--dir",
            "examples",
            "--glob",
            "{md,",
            "--current-date",
            "2025-12-05",
        ])
        .assert()
        .failure()
        .stderr(contains("invalid pattern"));
}
