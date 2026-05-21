//! Integration tests for the developer helper scripts:
//!   * `scripts/check.sh`        — local equivalent of CI (fmt + clippy + test).
//!   * `scripts/install-hooks.sh` — installs a git `pre-commit` hook that
//!     delegates to `scripts/check.sh`.
//!
//! Both scripts are POSIX bash. Tests drive them through `bash` with a
//! tempdir-isolated environment so the project's real `.git/hooks/` and
//! `cargo` are never touched.
//!
//! Unix-only: same reasoning as `release_check_changelog.rs` —
//! `Command::new("bash")` on Windows CI is unreliable, and these scripts
//! are documented as Linux/macOS developer tooling.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn script(name: &str) -> PathBuf {
    project_root().join("scripts").join(name)
}

/// Write a fake `cargo` to `bin_dir` that:
///   * appends its CLI args (one invocation per line, space-separated) to `log`;
///   * exits with code 1 if its first arg equals `fail_on`, otherwise 0.
fn write_fake_cargo(bin_dir: &Path, log: &Path, fail_on: Option<&str>) {
    let fail_marker = fail_on.unwrap_or("__none__");
    let script = format!(
        r#"#!/usr/bin/env bash
echo "$@" >> "{log}"
if [ "${{1:-}}" = "{fail}" ]; then
    exit 1
fi
exit 0
"#,
        log = log.display(),
        fail = fail_marker,
    );
    let bin = bin_dir.join("cargo");
    fs::write(&bin, script).unwrap();
    let mut perms = fs::metadata(&bin).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&bin, perms).unwrap();
}

/// Run `scripts/check.sh` with PATH pinned to `bin_dir` (where the fake cargo
/// lives) followed by the real PATH. Returns (exit code, stdout, stderr,
/// invocation log lines).
fn run_check(fail_on: Option<&str>) -> (i32, String, String, Vec<String>) {
    let dir = tempdir().unwrap();
    let bin_dir = dir.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let log = dir.path().join("cargo-invocations.log");
    write_fake_cargo(&bin_dir, &log, fail_on);

    let path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), path);

    let output = Command::new("bash")
        .arg(script("check.sh"))
        .env("PATH", new_path)
        .output()
        .expect("invoke check.sh");

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let log_contents = fs::read_to_string(&log).unwrap_or_default();
    let invocations: Vec<String> = log_contents
        .lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    (code, stdout, stderr, invocations)
}

#[test]
fn check_runs_fmt_clippy_doc_test_in_order_on_success() {
    let (code, _stdout, stderr, invocations) = run_check(None);
    assert_eq!(code, 0, "expected success; stderr: {stderr}");
    assert_eq!(
        invocations.len(),
        4,
        "expected exactly 4 cargo invocations, got {invocations:?}"
    );
    // fmt --check, clippy with -D warnings, doc with -D warnings, then test.
    assert!(
        invocations[0].starts_with("fmt"),
        "first invocation should be fmt: {:?}",
        invocations[0]
    );
    assert!(
        invocations[0].contains("--check"),
        "fmt must run in --check mode: {:?}",
        invocations[0]
    );
    assert!(
        invocations[1].starts_with("clippy"),
        "second invocation should be clippy: {:?}",
        invocations[1]
    );
    assert!(
        invocations[1].contains("-D warnings"),
        "clippy must deny warnings: {:?}",
        invocations[1]
    );
    assert!(
        invocations[2].starts_with("doc"),
        "third invocation should be doc: {:?}",
        invocations[2]
    );
    assert!(
        invocations[2].contains("--no-deps"),
        "doc must skip deps to keep the step fast: {:?}",
        invocations[2]
    );
    assert!(
        invocations[3].starts_with("test"),
        "fourth invocation should be test: {:?}",
        invocations[3]
    );
}

#[test]
fn check_fails_fast_when_fmt_fails() {
    let (code, _stdout, stderr, invocations) = run_check(Some("fmt"));
    assert_ne!(code, 0, "expected failure when fmt fails");
    assert_eq!(
        invocations.len(),
        1,
        "fail-fast: clippy/doc/test must not run after fmt failure; got {invocations:?}"
    );
    assert!(
        invocations[0].starts_with("fmt"),
        "the single failed invocation must be fmt: {:?}",
        invocations[0]
    );
    assert!(
        stderr.contains("fmt") || stderr.contains("format"),
        "stderr should mention which step failed: {stderr}"
    );
}

#[test]
fn check_fails_fast_when_clippy_fails() {
    let (code, _stdout, _stderr, invocations) = run_check(Some("clippy"));
    assert_ne!(code, 0, "expected failure when clippy fails");
    assert_eq!(
        invocations.len(),
        2,
        "fail-fast: doc/test must not run after clippy failure; got {invocations:?}"
    );
    assert!(invocations[1].starts_with("clippy"));
}

#[test]
fn check_fails_fast_when_doc_fails() {
    let (code, _stdout, _stderr, invocations) = run_check(Some("doc"));
    assert_ne!(code, 0, "expected failure when doc fails");
    assert_eq!(
        invocations.len(),
        3,
        "fail-fast: test must not run after doc failure; got {invocations:?}"
    );
    assert!(invocations[2].starts_with("doc"));
}

#[test]
fn check_fails_when_test_fails() {
    let (code, _stdout, _stderr, invocations) = run_check(Some("test"));
    assert_ne!(code, 0, "expected failure when test fails");
    assert_eq!(
        invocations.len(),
        4,
        "all four steps should run: {invocations:?}"
    );
    assert!(invocations[3].starts_with("test"));
}

/// Initialise a minimal git repo in `dir`. We don't need any commits — only
/// `.git/hooks/` and `git rev-parse --show-toplevel` must work.
fn init_git_repo(dir: &Path) {
    let status = Command::new("git")
        .arg("-c")
        .arg("init.defaultBranch=main")
        .arg("init")
        .arg("--quiet")
        .current_dir(dir)
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed in {}", dir.display());
}

fn run_install_hooks(repo: &Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new("bash")
        .arg(script("install-hooks.sh"))
        .args(args)
        .current_dir(repo)
        .output()
        .expect("invoke install-hooks.sh");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn install_hooks_creates_pre_commit_hook() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());

    let (code, _stdout, stderr) = run_install_hooks(dir.path(), &[]);
    assert_eq!(code, 0, "install-hooks must succeed; stderr: {stderr}");

    let hook = dir.path().join(".git").join("hooks").join("pre-commit");
    assert!(
        hook.is_file(),
        "pre-commit hook was not created at {}",
        hook.display()
    );
    let mode = fs::metadata(&hook).unwrap().permissions().mode();
    assert!(
        mode & 0o111 != 0,
        "pre-commit hook must be executable, got mode {mode:o}"
    );
    let body = fs::read_to_string(&hook).unwrap();
    assert!(
        body.contains("scripts/check.sh"),
        "hook must delegate to scripts/check.sh; body: {body}"
    );
}

#[test]
fn install_hooks_refuses_to_overwrite_existing_hook() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());
    let hooks_dir = dir.path().join(".git").join("hooks");
    let hook = hooks_dir.join("pre-commit");
    fs::write(&hook, "#!/bin/sh\necho 'user hook'\n").unwrap();
    let mut perms = fs::metadata(&hook).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook, perms).unwrap();

    let (code, _stdout, stderr) = run_install_hooks(dir.path(), &[]);
    assert_ne!(code, 0, "must refuse to overwrite an existing hook");
    assert!(
        stderr.contains("--force") || stderr.contains("exists"),
        "stderr must explain how to overwrite: {stderr}"
    );

    let body = fs::read_to_string(&hook).unwrap();
    assert!(
        body.contains("user hook"),
        "existing hook must remain intact when overwrite is refused; body: {body}"
    );
}

#[test]
fn install_hooks_overwrites_with_force_flag() {
    let dir = tempdir().unwrap();
    init_git_repo(dir.path());
    let hooks_dir = dir.path().join(".git").join("hooks");
    let hook = hooks_dir.join("pre-commit");
    fs::write(&hook, "#!/bin/sh\necho 'old hook'\n").unwrap();
    let mut perms = fs::metadata(&hook).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&hook, perms).unwrap();

    let (code, _stdout, stderr) = run_install_hooks(dir.path(), &["--force"]);
    assert_eq!(code, 0, "--force must overwrite; stderr: {stderr}");

    let body = fs::read_to_string(&hook).unwrap();
    assert!(
        body.contains("scripts/check.sh"),
        "after --force, hook must delegate to scripts/check.sh; body: {body}"
    );
    assert!(
        !body.contains("old hook"),
        "old hook content must be replaced; body: {body}"
    );
}

#[test]
fn install_hooks_fails_outside_git_repo() {
    let dir = tempdir().unwrap();
    // No `git init` — the script must refuse to guess.

    let (code, _stdout, stderr) = run_install_hooks(dir.path(), &[]);
    assert_ne!(code, 0, "must fail outside a git work tree");
    assert!(
        stderr.contains("git") || stderr.contains("repository"),
        "stderr should explain the missing repo: {stderr}"
    );
}
