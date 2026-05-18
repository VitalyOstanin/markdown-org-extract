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
        .stderr(contains("directory does not exist"));
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
    // With --absolute-paths we should see the path containing the fixture
    // directory. Use the platform-native separator so this works on Windows
    // (where JSON output preserves backslashes) as well as POSIX.
    let needle = format!("examples{}", std::path::MAIN_SEPARATOR);
    assert!(
        stdout.contains(&needle),
        "expected absolute path containing {needle:?} in stdout: {stdout:.200}"
    );
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
fn verbose_and_quiet_are_mutually_exclusive() {
    bin()
        .args([
            "--dir",
            "examples",
            "-v",
            "--quiet",
            "--current-date",
            "2025-12-05",
        ])
        .assert()
        .failure()
        .stderr(contains("cannot be used"));
}

#[test]
fn no_color_flag_is_accepted() {
    bin()
        .args([
            "--dir",
            "examples",
            "--no-color",
            "--current-date",
            "2025-12-05",
        ])
        .assert()
        .success();
}

#[test]
fn rejects_invalid_max_tasks() {
    bin()
        .args([
            "--dir",
            "examples",
            "--max-tasks",
            "0",
            "--current-date",
            "2025-12-05",
        ])
        .assert()
        .failure()
        .stderr(contains("--max-tasks"));
}

#[test]
fn max_tasks_one_caps_output() {
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--format",
            "json",
            "--tasks",
            "--max-tasks",
            "1",
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
    // Count top-level JSON objects in the flat task list. Minimal sanity check:
    // limit=1 must not produce a multi-element array opening with `{` after `[`.
    // We rely on parsed shape: an array with at most one element.
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let arr = parsed.as_array().expect("array");
    assert!(
        arr.len() <= 1,
        "got {} tasks, expected at most 1",
        arr.len()
    );
}

#[test]
fn holidays_conflicts_with_scan_flags() {
    // --holidays short-circuits before any scanning; combining it with a
    // scan/agenda flag is almost certainly a user mistake — fail loudly
    // instead of silently ignoring the extra flag.
    bin()
        .args(["--holidays", "2026", "--dir", "examples"])
        .assert()
        .failure()
        .stderr(contains("cannot be used"));

    bin()
        .args(["--holidays", "2026", "--tasks"])
        .assert()
        .failure()
        .stderr(contains("cannot be used"));
}

#[test]
fn tasks_conflicts_with_range_flags() {
    // --tasks emits a flat list and ignores agenda windowing; --from/--to
    // only make sense with --agenda week/month, so combining them with
    // --tasks should fail rather than silently drop the range.
    bin()
        .args([
            "--tasks",
            "--from",
            "2026-01-01",
            "--current-date",
            "2026-01-15",
        ])
        .assert()
        .failure()
        .stderr(contains("cannot be used"));

    bin()
        .args([
            "--tasks",
            "--to",
            "2026-01-31",
            "--current-date",
            "2026-01-15",
        ])
        .assert()
        .failure()
        .stderr(contains("cannot be used"));
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

#[test]
fn agenda_tasks_mode_produces_flat_list() {
    // `--agenda tasks` is the value-enum form of the legacy `--tasks` flag.
    // Both must produce the same flat-list JSON shape (top-level array of
    // task objects, not an array of day-objects).
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--current-date",
            "2025-12-05",
            "--agenda",
            "tasks",
            "--format",
            "json",
            "--max-tasks",
            "3",
            "--quiet",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let arr = parsed.as_array().expect("top-level array");
    // Flat task list: each element is a task object with `file`/`line` etc.,
    // not a day object with `date`/`overdue`/... keys.
    if let Some(first) = arr.first() {
        let obj = first.as_object().expect("task object");
        assert!(
            obj.contains_key("file") && obj.contains_key("line"),
            "expected flat-task shape, got: {first}"
        );
        assert!(
            !obj.contains_key("date"),
            "got day-shaped object instead of flat task: {first}"
        );
    }
}

#[test]
fn unknown_locale_emits_warning_on_stderr() {
    // --locale silently dropping an unrecognised entry is a foot-gun: a user
    // who typed `--locale en,de` got zero weekday mappings for `de` with no
    // hint that the project ships only `ru,en`. We now warn at the warn level
    // (default visibility) so the user sees it.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--current-date",
            "2025-12-05",
            "--locale",
            "ru,xx",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown --locale"),
        "expected warning about unknown locale, got: {stderr}"
    );
    assert!(
        stderr.contains("xx"),
        "expected offending value, got: {stderr}"
    );
}

#[test]
fn known_locales_do_not_warn() {
    // ru and en are both supported (en as a no-op). Neither should emit a
    // warning even when used together.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--current-date",
            "2025-12-05",
            "--locale",
            "ru,en",
            "--quiet",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "expected no warnings for ru,en, got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn validator_error_messages_match_clap_lowercase_style() {
    // clap prints `error: invalid value '<v>' for '--<arg> ...':` before the
    // validator's text. If our validators start with `Invalid <kind> '<v>':`
    // the whole line becomes `invalid value ...: Invalid <kind> ...:` -- the
    // same noun twice with mismatched capitalisation. Pin the style: no
    // re-echoed value, no capitalised prefix, lowercased reason.
    let out = bin()
        .args(["--dir", "examples", "--current-date", "abc"])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid value 'abc'"),
        "expected clap prefix, got: {stderr}"
    );
    assert!(
        !stderr.contains("Invalid date"),
        "validator must not start with `Invalid date`, got: {stderr}"
    );
    assert!(
        stderr.contains("use YYYY-MM-DD format"),
        "expected lowercase hint, got: {stderr}"
    );
}

#[test]
fn color_flag_accepts_auto_always_never() {
    // All three values must parse. `auto` is the default and behaves like
    // pre-existing logic (TTY-based). `always` and `never` are the explicit
    // override forms.
    for v in ["auto", "always", "never"] {
        bin()
            .args([
                "--dir",
                "examples",
                "--current-date",
                "2025-12-05",
                "--color",
                v,
                "--max-tasks",
                "1",
                "--tasks",
                "--format",
                "json",
            ])
            .assert()
            .success();
    }
}

#[test]
fn color_flag_rejects_unknown_value() {
    bin()
        .args([
            "--dir",
            "examples",
            "--current-date",
            "2025-12-05",
            "--color",
            "purple",
        ])
        .assert()
        .failure()
        .stderr(contains("invalid value"));
}

#[test]
fn color_conflicts_with_no_color() {
    // Both flags carry intent; combining them is almost certainly a mistake.
    // Force the user to pick one rather than silently letting --no-color win.
    bin()
        .args([
            "--dir",
            "examples",
            "--current-date",
            "2025-12-05",
            "--color",
            "always",
            "--no-color",
        ])
        .assert()
        .failure()
        .stderr(contains("cannot be used"));
}

#[test]
fn help_mentions_format_md_alias() {
    // README documents `--format md`, so the short help must echo the alias.
    // clap doesn't render value-enum aliases in `[possible values: ...]`, so
    // the alias has to live in the per-arg docstring. Pin both `-h` and
    // `--help` against silently dropping it.
    let short = bin().arg("-h").output().expect("run");
    let short_out = String::from_utf8_lossy(&short.stdout);
    assert!(
        short_out.contains("`md`"),
        "expected `md` alias in -h, got: {short_out}"
    );
    let long = bin().arg("--help").output().expect("run");
    let long_out = String::from_utf8_lossy(&long.stdout);
    assert!(
        long_out.contains("`md`"),
        "expected `md` alias in --help, got: {long_out}"
    );
}

#[test]
fn output_dash_writes_to_stdout_and_creates_no_file() {
    // `--output -` is the standard unix sigil for "write to stdout"; with it,
    // the result must arrive on stdout and no file named `-` should appear.
    let dir = tempdir().unwrap();
    let out = bin()
        .current_dir(dir.path())
        .args([
            "--dir",
            concat!(env!("CARGO_MANIFEST_DIR"), "/examples"),
            "--format",
            "json",
            "--tasks",
            "--current-date",
            "2025-12-05",
            "--output",
            "-",
            "--max-tasks",
            "1",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let _parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert!(
        !dir.path().join("-").exists(),
        "literal file `-` must not be created"
    );
}

#[test]
fn verbose_emits_info_summary_on_stderr() {
    // -v lifts the default log level to info, which makes the `scan finished`
    // summary visible. Locks the info-emitter against accidental downgrade.
    let out = bin()
        .args(["--dir", "examples", "--current-date", "2025-12-05", "-v"])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("scan finished"),
        "expected info summary on stderr at -v, got: {stderr}"
    );
}

#[test]
fn quiet_suppresses_all_diagnostics_on_stderr() {
    // --quiet drops the log level to error and skips the processing-summary
    // print on its own. With a clean fixture set there should be nothing
    // diagnostic on stderr — pin this so future tracing additions don't
    // silently leak through quiet mode.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--current-date",
            "2025-12-05",
            "--quiet",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "expected empty stderr with --quiet, got: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn help_no_color_mentions_env_var_equivalence() {
    // The --no-color help text must say the NO_COLOR env var has the *same*
    // effect (not "honors as well", which reads ambiguously). Pin the wording
    // so a future help-text edit cannot reintroduce the ambiguity.
    let out = bin().arg("--help").output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("NO_COLOR"), "missing NO_COLOR mention");
    assert!(
        stdout.contains("same effect"),
        "expected 'same effect' wording, got: {stdout}"
    );
}

#[test]
fn rejects_inverted_from_to_range() {
    // --from > --to should fail loudly with the DateRange variant; silently
    // accepting an empty range would produce a confusingly empty agenda. The
    // check is in agenda::parse_range; pin it from the CLI surface so a
    // refactor that drops the comparison cannot ship.
    bin()
        .args([
            "--dir",
            "examples",
            "--agenda",
            "week",
            "--from",
            "2025-12-10",
            "--to",
            "2025-12-01",
            "--current-date",
            "2025-12-05",
        ])
        .assert()
        .failure()
        .stderr(contains("after end date"));
}

#[test]
fn debug_log_includes_per_file_span_field() {
    // -vv enables debug-level events. The parser emits a `parsed file` event
    // inside a `file` span carrying `path = ...`. The tracing fmt-layer prints
    // span fields in the message, so stderr must contain a `path=` segment for
    // at least one processed file. Locks the span wrapping in main.rs against
    // accidental removal.
    let out = bin()
        .args(["--dir", "examples", "--current-date", "2025-12-05", "-vv"])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("path="),
        "expected `path=` from the file span, got stderr: {stderr}"
    );
}
