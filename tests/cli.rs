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
    // Tasks mode does not accept date arguments (see ADR-0009), so no
    // --current-date here; the cap is over the flat task list and is
    // deterministic from --max-tasks alone.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--format",
            "json",
            "--tasks",
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
fn unknown_locale_is_hard_error_even_under_quiet() {
    // --locale must reject unknown entries at parse time, not at log time:
    // a tracing::warn! would be swallowed by --quiet and a user typing
    // `--locale en,de --quiet` would silently get zero `de` mappings.
    // Validate-at-CLI puts the error on the same tier as `--dir` /
    // `--tz` / `--date` checks (exit code 2 from AppError::InvalidOutput
    // equivalents -- here clap's own usage-error path produces 2).
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--current-date",
            "2025-12-05",
            "--locale",
            "ru,xx",
            "--quiet",
        ])
        .output()
        .expect("run");
    assert!(
        !out.status.success(),
        "expected failure, got success; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(2),
        "expected exit code 2 for usage error, got: {:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown locale"),
        "expected 'unknown locale' wording, got: {stderr}"
    );
    assert!(
        stderr.contains("xx"),
        "expected offending value 'xx', got: {stderr}"
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
fn help_groups_arguments_into_named_sections() {
    // The flag count has grown to the point where a flat list is hard to
    // scan. clap's `help_heading` puts related flags under labelled sections
    // ("Input:", "Output:", ...). Pin the headings so a future edit cannot
    // silently regress to a flat list and leave users wading through 19
    // options in arrival order.
    let out = bin().arg("--help").output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for heading in [
        "Input:",
        "Output:",
        "Agenda:",
        "Limits:",
        "Diagnostics:",
        "Actions:",
    ] {
        assert!(
            stdout.contains(heading),
            "expected `{heading}` section in --help, got: {stdout}"
        );
    }
}

#[test]
fn help_long_about_includes_runnable_examples() {
    // `--help` (long form) must include at least one example command so a
    // first-time reader sees what an invocation looks like. We pin the
    // ones most likely to be copy-pasted (today's agenda, holidays year,
    // bash completion install) rather than every example, so harmless
    // wording tweaks don't fail the test.
    let out = bin().arg("--help").output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Examples:"),
        "expected `Examples:` block in long --help, got: {stdout}"
    );
    for needle in [
        "markdown-org-extract --dir ~/notes --agenda day",
        "markdown-org-extract --holidays 2026",
        "markdown-org-extract --completions bash",
    ] {
        assert!(
            stdout.contains(needle),
            "expected example `{needle}` in long --help, got: {stdout}"
        );
    }
}

#[test]
fn short_help_omits_examples_block() {
    // `-h` is the at-a-glance summary; the multi-line `Examples:` block
    // belongs only in `--help`. clap normally hides `long_about` from
    // `-h`, but if a future edit moves the examples into `about` they
    // would leak into `-h` and clutter the summary. Pin the contract.
    let out = bin().arg("-h").output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("Examples:"),
        "short `-h` must not include the Examples block, got: {stdout}"
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

// Exit-code routing per AppError category. The values come from `sysexits.h`
// where applicable (74 = EX_IOERR, 70 = EX_SOFTWARE); usage errors use `2` to
// match clap's own argument-error exit code so the boundary between
// clap-level and app-level validation failures is invisible to the caller.

#[test]
fn exit_code_2_for_invalid_directory() {
    let out = bin()
        .args([
            "--dir",
            "/this/path/should/never/exist_xyz_exitcode",
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    assert_eq!(
        out.status.code(),
        Some(2),
        "invalid --dir is a usage error, must exit 2 (got {:?}); stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn exit_code_2_for_invalid_output_parent() {
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--output",
            "/this/parent/should/never/exist/out.json",
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    assert_eq!(
        out.status.code(),
        Some(2),
        "invalid --output (missing parent) is a usage error, must exit 2 (got {:?}); stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn exit_code_74_for_io_when_output_is_a_directory() {
    let tmp = tempdir().expect("tmpdir");
    let out_path = tmp.path().join("collision-dir");
    fs::create_dir(&out_path).expect("create collision dir");

    let out = bin()
        .args([
            "--dir",
            "examples",
            "--output",
            out_path.to_str().unwrap(),
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    assert_eq!(
        out.status.code(),
        Some(74),
        "writing to a path that is a directory is an IO error, must exit 74 (got {:?}); stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

// Unified date-window semantics (ADR-0009). The agenda module accepts
// --from/--to as an alternative to --date in day/week/month, fills a
// missing edge from current_date (--current-date or today), and rejects
// any date argument in tasks mode. The integration tests below pin the
// CLI surface so a future agenda refactor cannot silently regress.

fn day_count_in_json(stdout: &str) -> usize {
    let parsed: serde_json::Value =
        serde_json::from_str(stdout).expect("stdout must be valid JSON");
    parsed
        .as_array()
        .expect("top-level array")
        .len()
}

#[test]
fn agenda_day_with_from_to_emits_multi_day() {
    // --from/--to in day mode is no longer ignored: each day in [from..to]
    // produces a DayAgenda. Range 2025-12-01..2025-12-07 -> 7 days.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--agenda",
            "day",
            "--from",
            "2025-12-01",
            "--to",
            "2025-12-07",
            "--current-date",
            "2025-12-05",
            "--format",
            "json",
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
    assert_eq!(
        day_count_in_json(&stdout),
        7,
        "expected 7 day-agendas for [2025-12-01..2025-12-07]; got {stdout:.200}"
    );
}

#[test]
fn agenda_week_from_only_fills_to_from_current_date() {
    // --from X without --to: end is current_date. Range 2025-12-01..2025-12-05
    // -> 5 days.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--agenda",
            "week",
            "--from",
            "2025-12-01",
            "--current-date",
            "2025-12-05",
            "--format",
            "json",
            "--quiet",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(day_count_in_json(&String::from_utf8_lossy(&out.stdout)), 5);
}

#[test]
fn agenda_month_to_only_fills_from_from_current_date() {
    // --to Y without --from: start is current_date. Range 2025-12-05..2025-12-10
    // -> 6 days.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--agenda",
            "month",
            "--to",
            "2025-12-10",
            "--current-date",
            "2025-12-05",
            "--format",
            "json",
            "--quiet",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(day_count_in_json(&String::from_utf8_lossy(&out.stdout)), 6);
}

#[test]
fn agenda_day_from_after_current_date_fails() {
    // --from X without --to, where X > current_date: the inferred range is
    // inverted, must surface as DateRange.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--agenda",
            "day",
            "--from",
            "2026-01-15",
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    assert!(!out.status.success(), "expected failure");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("after end date"),
        "expected DateRange diagnostic; got: {stderr}"
    );
}

#[test]
fn agenda_tasks_rejects_date_argument() {
    // Tasks mode is task-based, not date-centric: ADR-0009 rejects --date,
    // --from, --to, --current-date in this mode. --from is already blocked at
    // clap level (conflicts_with = "tasks"); --date must surface from agenda.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--agenda",
            "tasks",
            "--date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("tasks mode does not accept date arguments"),
        "expected ADR-0009 tasks-mode rejection; got: {stderr}"
    );
}

/// Shell completions: `--completions <SHELL>` short-circuits scanning and
/// emits the completion script for the given shell. The integration test
/// pins three shells (bash, zsh, fish) and asserts that the output mentions
/// the binary name; a script that does not at least name the binary cannot
/// be a valid completion file. The exact dialect of each shell's script is
/// owned by clap_complete and not re-asserted here.
#[test]
fn completions_emit_per_shell_script() {
    for shell in ["bash", "zsh", "fish"] {
        let out = bin()
            .args(["--completions", shell])
            .output()
            .expect("run");
        assert!(
            out.status.success(),
            "completions for {shell} must succeed; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("markdown-org-extract"),
            "completion script for {shell} must mention the binary name; got: {stdout:.200}"
        );
        assert!(
            stdout.len() > 200,
            "completion script for {shell} looks empty ({} bytes)",
            stdout.len()
        );
    }
}

#[test]
fn completions_conflicts_with_scan_flags() {
    // --completions is a short-circuit like --holidays; mixing it with scan
    // flags would produce nonsense, so clap rejects the combination.
    bin()
        .args(["--completions", "bash", "--dir", "examples"])
        .assert()
        .failure()
        .stderr(contains("cannot be used"));
}

#[test]
fn completions_rejects_unknown_shell() {
    bin()
        .args(["--completions", "tcsh"])
        .assert()
        .failure()
        .stderr(contains("invalid value"));
}

/// Multi-segment glob pattern against a relative `--dir`. WalkBuilder used
/// to be fed `&cli.dir` (relative), so emitted paths stayed relative and
/// `strip_prefix(dir_canonical)` failed, dropping callers to a `file_name()`
/// fallback that could not match a multi-segment pattern like `sub/*.md`.
/// Feeding WalkBuilder the canonical absolute path fixes this; this test
/// pins the fix so any later refactor cannot regress it.
#[test]
fn multi_segment_glob_matches_with_relative_dir() {
    let tmp = tempdir().expect("tmp");
    let workspace = tmp.path().join("ws");
    let sub = workspace.join("sub");
    fs::create_dir_all(&sub).expect("mkdir sub");
    fs::write(sub.join("foo.md"), "### TODO Foo task\n").expect("write foo.md");
    fs::write(sub.join("bar.md"), "### TODO Bar task\n").expect("write bar.md");
    fs::write(
        workspace.join("top.md"),
        "### TODO Top task should not be matched\n",
    )
    .expect("write top.md");

    let out = bin()
        .current_dir(tmp.path())
        .args([
            "--dir",
            "ws",
            "--glob",
            "sub/*.md",
            "--tasks",
            "--format",
            "json",
            "--quiet",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "scan must succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let arr = parsed.as_array().expect("array");
    assert_eq!(
        arr.len(),
        2,
        "expected exactly 2 matches (foo.md, bar.md); got {arr:?}"
    );
    let headings: Vec<&str> = arr
        .iter()
        .filter_map(|t| t.get("heading").and_then(|h| h.as_str()))
        .collect();
    assert!(
        headings.iter().any(|h| h.contains("Foo")),
        "expected Foo task; headings: {headings:?}"
    );
    assert!(
        headings.iter().any(|h| h.contains("Bar")),
        "expected Bar task; headings: {headings:?}"
    );
    assert!(
        !headings.iter().any(|h| h.contains("Top")),
        "Top task must not match `sub/*.md`; headings: {headings:?}"
    );
}

/// Test fixture: an unreadable subdirectory should not abort the scan. The
/// test creates a workspace with one readable file and one mode-0 subtree,
/// runs the binary against the workspace root, and verifies that
///
/// 1. The exit code is 0 (the scan reported usable output).
/// 2. The readable file's tasks are present in stdout.
/// 3. The summary on stderr mentions walk_errors > 0.
#[cfg(unix)]
#[test]
fn walker_continues_after_permission_denied_subdir() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempdir().expect("tmp");
    fs::write(
        root.path().join("ok.md"),
        "# Notes\n\n### TODO First\n`SCHEDULED: <2025-12-05 Fri>`\n",
    )
    .expect("write ok.md");

    let blocked = root.path().join("blocked");
    fs::create_dir(&blocked).expect("mkdir blocked");
    fs::write(
        blocked.join("hidden.md"),
        "# Hidden\n### TODO Hidden task\n",
    )
    .expect("write hidden.md");
    let mut perms = fs::metadata(&blocked).expect("metadata").permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&blocked, perms).expect("chmod 0");

    let out = bin()
        .args([
            "--dir",
            root.path().to_str().unwrap(),
            "--tasks",
            "--format",
            "json",
            "-v",
        ])
        .output()
        .expect("run");

    // Restore permissions before assertions so the tempdir cleanup can recurse.
    let mut perms = fs::metadata(&blocked).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&blocked, perms).expect("chmod restore");

    assert!(
        out.status.success(),
        "scan must succeed despite walker error; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("First"),
        "readable file's task must be in output; stdout: {stdout:.500}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("walk_errors") || stderr.contains("walker entry failed"),
        "summary or per-error warning must mention the walker error; stderr: {stderr}"
    );
}

#[test]
fn agenda_tasks_rejects_current_date_argument() {
    // --current-date in tasks mode is also rejected: tasks mode has no
    // overdue calculation, so the "today" reference has no effect.
    let out = bin()
        .args([
            "--dir",
            "examples",
            "--agenda",
            "tasks",
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("tasks mode does not accept date arguments"),
        "expected ADR-0009 tasks-mode rejection; got: {stderr}"
    );
}
