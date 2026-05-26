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
fn agenda_conflicts_with_tasks_flag() {
    // `--agenda day` (or week/month) selects a windowed view; `--tasks`
    // selects a flat list. The two modes are mutually exclusive at the
    // clap layer via conflicts_with on --agenda. Pin the rejection so a
    // refactor that drops the conflict cannot quietly let one mode
    // override the other.
    bin()
        .args([
            "--dir",
            "examples",
            "--current-date",
            "2025-12-05",
            "--agenda",
            "week",
            "--tasks",
        ])
        .assert()
        .failure()
        .stderr(contains("cannot be used"));
}

#[test]
fn verbose_conflicts_with_quiet() {
    // `-v` raises log level above warn; `-q` lowers it to error. Combining
    // them is meaningless: the user can't both want more and less
    // diagnostics at the same time. The conflict is on the --quiet arg.
    bin()
        .args([
            "--dir",
            "examples",
            "--current-date",
            "2025-12-05",
            "--verbose",
            "--quiet",
        ])
        .assert()
        .failure()
        .stderr(contains("cannot be used"));

    // `-v` short form must trigger the same conflict; the relationship is
    // on the long names but short aliases share the same arg id.
    bin()
        .args([
            "--dir",
            "examples",
            "--current-date",
            "2025-12-05",
            "-v",
            "-q",
        ])
        .assert()
        .failure()
        .stderr(contains("cannot be used"));
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
fn help_verbose_documents_the_trace_ceiling() {
    // MIN-8 (2026-05-25 review): the --verbose help promised
    // info/debug/trace but said nothing about `-vvvv+` saturating at
    // trace. A user who escalated past `-vvv` expecting "more than trace"
    // got a runtime saturation warning with no documentation behind it.
    // The help now states the ceiling; pin the wording so it cannot
    // silently regress while `verbose_saturation_warns_on_vvvv_and_beyond`
    // keeps pinning the runtime side.
    let out = bin().arg("--help").output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("-vvv` is the maximum") || stdout.contains("-vvv is the maximum"),
        "expected the --verbose help to document the trace ceiling, got: {stdout}"
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
fn help_long_about_includes_exit_status_section() {
    // CLI-UX info 1 / finding 9 (2026-05-25 review): shell scripts and bug
    // reports branch on exit codes, but they were documented only in the
    // source and the README. `--help` now carries an `Exit status:` block so
    // the codes are discoverable from the binary itself. Pin the heading plus
    // the two least-obvious codes (130 for signal, 74 for IO).
    let out = bin().arg("--help").output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Exit status:"),
        "expected an `Exit status:` block in --help, got: {stdout}"
    );
    for code in ["130", "74"] {
        assert!(
            stdout.contains(code),
            "expected exit code `{code}` in the --help Exit status block, got: {stdout}"
        );
    }
}

#[test]
fn help_long_about_includes_environment_section() {
    // CLI-UX info 8 / finding (2026-05-25 review): the recognised env vars
    // (RUST_LOG, NO_COLOR, CLICOLOR, CLICOLOR_FORCE) were scattered across
    // individual flag help-texts. `--help` now consolidates them under an
    // `Environment:` block. Pin the heading and RUST_LOG (the one with
    // behavioural precedence over --verbose/--quiet).
    let out = bin().arg("--help").output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Environment:"),
        "expected an `Environment:` block in --help, got: {stdout}"
    );
    assert!(
        stdout.contains("RUST_LOG"),
        "expected RUST_LOG in the --help Environment block, got: {stdout}"
    );
}

#[test]
fn short_about_mentions_json_default() {
    // CLI-UX info 1 / recommendation 2 (2026-05-25 review): JSON is the
    // default wire format (ADR-0001), but the short `about` shown by `-h`
    // did not say so — a user saw it only in the long `--help`. Pin the
    // JSON-default mention in the at-a-glance summary.
    let out = bin().arg("-h").output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("JSON by default"),
        "expected the short -h about to mention JSON as the default, got: {stdout}"
    );
}

#[test]
fn completions_help_uses_user_local_path() {
    // CLI-UX info 7 / recommendation 4 (2026-05-25 review): the per-arg help
    // for --completions suggested a system-wide path (/etc/bash_completion.d,
    // needs sudo) while the Examples block used a user-local one. A
    // root-free CLI should not steer users toward sudo. The system-wide
    // path is gone from --help; assert it does not reappear.
    let out = bin().arg("--help").output().expect("run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("/etc/bash_completion.d"),
        "the --completions help must not steer users to a sudo-only system path, got: {stdout}"
    );
}

#[test]
fn completions_emit_elvish_and_powershell() {
    // CLI-UX info 5 / recommendation 5 (2026-05-25 review): the ValueEnum
    // accepts elvish and powershell, and the README lists them, but only
    // bash/zsh/fish were exercised. Smoke-pin that these two also emit a
    // non-trivial script and exit 0, so a clap_complete bump that drops one
    // fails CI.
    for shell in ["elvish", "powershell"] {
        let out = bin().args(["--completions", shell]).output().expect("run");
        assert!(
            out.status.success(),
            "--completions {shell} must succeed; stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stdout.len() > 200,
            "--completions {shell} should emit a non-trivial script, got {} bytes",
            out.stdout.len()
        );
    }
}

#[test]
fn version_flag_prints_semver() {
    // CLI-UX info 6 / recommendation 6 (2026-05-25 review): `#[command(version)]`
    // wires up --version, but nothing pinned it, so an accidental removal
    // would go unnoticed. Pin the exact `markdown-org-extract <X.Y.Z>` line
    // against the crate version.
    let out = bin().arg("--version").output().expect("run");
    assert!(
        out.status.success(),
        "--version must succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let expected = format!("markdown-org-extract {}", env!("CARGO_PKG_VERSION"));
    assert_eq!(
        stdout.trim(),
        expected,
        "--version output must be `{expected}`, got: {stdout}"
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
fn debug_log_uses_unified_file_key_not_path() {
    // -vv enables debug-level events. The parser emits a `parsed file` event
    // inside a `file` span. Both the span and the parser events key the path
    // under `file = ...` (matching `Task.file`); the older split where the
    // span used `path=` while events used `file=` (O3, 2026-05-25 review) is
    // gone. The tracing fmt-layer prints span fields in the message, so a
    // clean run's stderr must carry `file=` and must NOT carry a stray `path=`
    // segment. Locks the unified key against regression.
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
        stderr.contains("file="),
        "expected `file=` from the unified file key, got stderr: {stderr}"
    );
    assert!(
        !stderr.contains("path="),
        "the file span/events must use `file=`, never a stray `path=`; stderr: {stderr}"
    );
}

#[test]
fn run_span_wraps_scan_finished_at_info() {
    // O4 (2026-05-25 review): a root `run` span carries the scanned `dir` so
    // every event under it — including the info-level `scan finished` summary
    // — is attributable to a run. At -v the info span is active, so the
    // fmt-layer prefixes the `scan finished` line with `run{dir=...}:`. Pins
    // the root span against accidental removal without bloating the default
    // (warn) output, where an info span is inactive.
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
    let scan_line = stderr
        .lines()
        .find(|l| l.contains("scan finished"))
        .unwrap_or_else(|| panic!("no `scan finished` line at -v; stderr: {stderr}"));
    assert!(
        scan_line.contains("run{dir="),
        "the `scan finished` line must inherit the root `run{{dir=...}}` span; line was: {scan_line}"
    );
}

#[test]
fn parse_repeater_rejection_uses_static_event_name() {
    // O6 (2026-05-25 review): the trace event fired when `parse_repeater`
    // rejects an input used the message `parse_repeater: rejected`, mixing the
    // operation identifier with the text. With `with_target(false)` the
    // operation is otherwise invisible, so the message is now a single static
    // name `parse_repeater_rejected` (the reason stays in the `reason` field).
    // A zero-step repeater `+0d` is the cheapest rejected input to exercise.
    let dir = tempdir().unwrap();
    let path = dir.path().join("repeater.md");
    fs::write(
        &path,
        "### TODO repeater probe\n`SCHEDULED: <2024-12-09 Mon +0d>`\n",
    )
    .unwrap();

    let out = bin()
        .args([
            "--dir",
            dir.path().to_str().unwrap(),
            "--current-date",
            "2024-12-09",
            "-vvv",
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
        stderr.contains("parse_repeater_rejected"),
        "expected the static event name `parse_repeater_rejected` at -vvv; stderr: {stderr}"
    );
    assert!(
        !stderr.contains("parse_repeater: rejected"),
        "the colon-style message must be gone; stderr: {stderr}"
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
    // The Io variant now embeds the failing path in Display; pin that so a
    // refactor that drops the context (e.g. by reinstating a blanket
    // From<io::Error>) leaves an empty "io: : ..." trail and breaks loudly.
    let stderr = String::from_utf8_lossy(&out.stderr);
    let path_str = out_path.to_string_lossy();
    assert!(
        stderr.contains(&*path_str),
        "expected the failing path '{path_str}' in stderr, got: {stderr}"
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
    parsed.as_array().expect("top-level array").len()
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
        let out = bin().args(["--completions", shell]).output().expect("run");
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
            "--dir", "ws", "--glob", "sub/*.md", "--tasks", "--format", "json", "--quiet",
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
fn output_write_to_readonly_parent_exits_74_with_path_in_stderr() {
    // EACCES on the write itself (parent dir is r-x, no w) is the most
    // common --output failure in CI sandboxes and locked-down deploy
    // directories. The path must be in stderr — without it the user
    // sees a bare "Permission denied (os error 13)" and has to guess.
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().expect("tmpdir");
    let ro_dir = tmp.path().join("ro");
    fs::create_dir(&ro_dir).expect("mkdir ro");
    let out_path = ro_dir.join("out.json");

    let mut perms = fs::metadata(&ro_dir).expect("metadata").permissions();
    perms.set_mode(0o555);
    fs::set_permissions(&ro_dir, perms).expect("chmod 555");

    let out = bin()
        .args([
            "--dir",
            "examples",
            "--output",
            out_path.to_str().unwrap(),
            "--current-date",
            "2025-12-05",
            "--quiet",
        ])
        .output()
        .expect("run");

    // Restore perms before assertions so tempdir cleanup can remove the dir.
    let mut perms = fs::metadata(&ro_dir).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&ro_dir, perms).expect("chmod restore");

    assert_eq!(
        out.status.code(),
        Some(74),
        "write into read-only parent must exit 74 (EX_IOERR); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let path_str = out_path.to_string_lossy();
    assert!(
        stderr.contains(&*path_str),
        "expected the failing path '{path_str}' in stderr, got: {stderr}"
    );
}

#[cfg(unix)]
#[test]
fn output_write_to_readonly_file_exits_74_with_path_in_stderr() {
    // Overwriting an existing file that has no write bit set is the
    // second failure mode for --output. Same exit code (74), same
    // path-in-stderr contract — pin both so a refactor that swallows
    // the path or downgrades the exit code regresses loudly.
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().expect("tmpdir");
    let out_path = tmp.path().join("locked.json");
    fs::write(&out_path, b"placeholder").expect("write placeholder");
    let mut perms = fs::metadata(&out_path).expect("metadata").permissions();
    perms.set_mode(0o444);
    fs::set_permissions(&out_path, perms).expect("chmod 444");

    let out = bin()
        .args([
            "--dir",
            "examples",
            "--output",
            out_path.to_str().unwrap(),
            "--current-date",
            "2025-12-05",
            "--quiet",
        ])
        .output()
        .expect("run");

    // Restore so tempdir can clean up.
    let mut perms = fs::metadata(&out_path).expect("metadata").permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&out_path, perms).expect("chmod restore");

    assert_eq!(
        out.status.code(),
        Some(74),
        "overwrite of read-only file must exit 74 (EX_IOERR); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    let path_str = out_path.to_string_lossy();
    assert!(
        stderr.contains(&*path_str),
        "expected the failing path '{path_str}' in stderr, got: {stderr}"
    );
}

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

// Byte-exact JSON snapshots. The wire contract is documented in ADR-0001
// (JSON on stdout) and consumed by downstream tooling; a reordering of
// fields, a change of indentation, or a missing newline would silently
// break that contract. The tests below pin two output shapes against a
// hand-written fixture so any structural drift requires updating the
// snapshot here in the same commit as the source change.

#[test]
fn json_snapshot_tasks_mode_minimal_fixture() {
    // A single TODO with SCHEDULED + relative paths is the smallest input
    // that exercises every Task field (file, line, heading, content,
    // task_type, timestamp, timestamp_type, timestamp_date). `tasks` mode
    // forbids --current-date by ADR-0009, so there are no date-dependent
    // outputs to make the snapshot drift between runs.
    let tmp = tempdir().expect("tmpdir");
    fs::write(
        tmp.path().join("notes.md"),
        "# Notes\n\n### TODO Pin me\n`SCHEDULED: <2026-05-21 Thu>`\n",
    )
    .expect("write notes.md");

    let out = bin()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--tasks",
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
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let expected = "\
[
  {
    \"file\": \"notes.md\",
    \"line\": 3,
    \"heading\": \"Pin me\",
    \"content\": \"\",
    \"task_type\": \"TODO\",
    \"timestamp\": \"SCHEDULED: <2026-05-21 Thu>\",
    \"timestamp_type\": \"SCHEDULED\",
    \"timestamp_active\": true,
    \"timestamp_date\": \"2026-05-21\"
  }
]
";
    assert_eq!(
        stdout, expected,
        "JSON tasks snapshot must be byte-exact; got:\n{stdout}"
    );
}

#[test]
fn json_snapshot_agenda_day_minimal_fixture() {
    // Pin the agenda-day envelope (date, scheduled_timed, scheduled_no_time,
    // upcoming). Same fixture as the tasks snapshot but with
    // `--agenda day --current-date 2026-05-21` to materialise the wrapper
    // fields. Without this snapshot a renamed array key or a flip of
    // overdue vs scheduled would slip past every existing test.
    let tmp = tempdir().expect("tmpdir");
    fs::write(
        tmp.path().join("notes.md"),
        "# Notes\n\n### TODO Pin me\n`SCHEDULED: <2026-05-21 Thu>`\n",
    )
    .expect("write notes.md");

    let out = bin()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--agenda",
            "day",
            "--current-date",
            "2026-05-21",
            "--tz",
            "UTC",
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
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let expected = "\
[
  {
    \"date\": \"2026-05-21\",
    \"scheduled_timed\": [],
    \"scheduled_no_time\": [
      {
        \"file\": \"notes.md\",
        \"line\": 3,
        \"heading\": \"Pin me\",
        \"content\": \"\",
        \"task_type\": \"TODO\",
        \"timestamp\": \"SCHEDULED: <2026-05-21 Thu>\",
        \"timestamp_type\": \"SCHEDULED\",
        \"timestamp_active\": true,
        \"timestamp_date\": \"2026-05-21\"
      }
    ],
    \"upcoming\": []
  }
]
";
    assert_eq!(
        stdout, expected,
        "JSON agenda-day snapshot must be byte-exact; got:\n{stdout}"
    );
}

#[test]
fn json_snapshot_tasks_mode_clock_entry() {
    // MIN-12 (2026-05-25 tests review): the existing snapshots never
    // exercised the CLOCK fields. Pin the `clocks` array element shape
    // (start / end / duration) and the derived `total_clock_time` so a
    // rename or reordering of those keys -- a breaking change under
    // ADR-0015 -- cannot slip past the suite. CLOCK bracket forms are
    // governed by ADR-0003.
    let tmp = tempdir().expect("tmpdir");
    fs::write(
        tmp.path().join("clock.md"),
        "# Notes\n\n### TODO Clocked task\n`SCHEDULED: <2026-05-21 Thu>`\n\
         `CLOCK: [2026-05-21 Thu 10:00]--[2026-05-21 Thu 11:30] => 1:30`\n",
    )
    .expect("write clock.md");

    let out = bin()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--tasks",
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
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let expected = "\
[
  {
    \"file\": \"clock.md\",
    \"line\": 3,
    \"heading\": \"Clocked task\",
    \"content\": \"\",
    \"task_type\": \"TODO\",
    \"timestamp\": \"SCHEDULED: <2026-05-21 Thu>\",
    \"timestamp_type\": \"SCHEDULED\",
    \"timestamp_active\": true,
    \"timestamp_date\": \"2026-05-21\",
    \"clocks\": [
      {
        \"start\": \"2026-05-21 Thu 10:00\",
        \"end\": \"2026-05-21 Thu 11:30\",
        \"duration\": \"1:30\"
      }
    ],
    \"total_clock_time\": \"1:30\"
  }
]
";
    assert_eq!(
        stdout, expected,
        "JSON CLOCK snapshot must be byte-exact; got:\n{stdout}"
    );
}

#[test]
fn json_snapshot_tasks_mode_inactive_timestamp() {
    // MIN-12: pin `timestamp_active: false` for an inactive `[...]`
    // timestamp. The active/inactive marker is the ADR-0014 contract that
    // markdown-org-vscode relies on to round-trip the bracket form; a
    // regression that dropped or inverted it would be a silent breaking
    // change.
    let tmp = tempdir().expect("tmpdir");
    fs::write(
        tmp.path().join("inactive.md"),
        "# Notes\n\n### TODO Inactive stamp\n`[2026-05-21 Thu]`\n",
    )
    .expect("write inactive.md");

    let out = bin()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--tasks",
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
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let expected = "\
[
  {
    \"file\": \"inactive.md\",
    \"line\": 3,
    \"heading\": \"Inactive stamp\",
    \"content\": \"\",
    \"task_type\": \"TODO\",
    \"timestamp\": \"[2026-05-21 Thu]\",
    \"timestamp_type\": \"PLAIN\",
    \"timestamp_active\": false,
    \"timestamp_date\": \"2026-05-21\"
  }
]
";
    assert_eq!(
        stdout, expected,
        "JSON inactive-timestamp snapshot must be byte-exact; got:\n{stdout}"
    );
}

#[test]
fn json_snapshot_tasks_mode_repeater_and_warning_preserved() {
    // MIN-12: the repeater (`+1m`) and warning cookie (`-3d`) are not
    // separate JSON fields -- they live verbatim inside the `timestamp`
    // string, which downstream tooling re-parses. Pin that the string is
    // surfaced byte-for-byte so a future "helpful" normalisation of the
    // timestamp cannot silently drop the cookies.
    let tmp = tempdir().expect("tmpdir");
    fs::write(
        tmp.path().join("rep.md"),
        "# Notes\n\n### TODO Repeating with warning\n`DEADLINE: <2026-05-21 Thu +1m -3d>`\n",
    )
    .expect("write rep.md");

    let out = bin()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--tasks",
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
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let expected = "\
[
  {
    \"file\": \"rep.md\",
    \"line\": 3,
    \"heading\": \"Repeating with warning\",
    \"content\": \"\",
    \"task_type\": \"TODO\",
    \"timestamp\": \"DEADLINE: <2026-05-21 Thu +1m -3d>\",
    \"timestamp_type\": \"DEADLINE\",
    \"timestamp_active\": true,
    \"timestamp_date\": \"2026-05-21\"
  }
]
";
    assert_eq!(
        stdout, expected,
        "JSON repeater/warning snapshot must be byte-exact; got:\n{stdout}"
    );
}

#[test]
fn json_snapshot_agenda_week_envelope() {
    // MIN-12: pin the week envelope. It is an array of seven day objects
    // (date / scheduled_timed / scheduled_no_time / upcoming) starting on
    // the Monday of the --current-date's ISO week (2026-05-18). The single
    // task lands on 2026-05-21 under scheduled_no_time; the other six days
    // are empty buckets, which pins both the day count and the per-day
    // shape against an array-key rename.
    let tmp = tempdir().expect("tmpdir");
    fs::write(
        tmp.path().join("wk.md"),
        "# Notes\n\n### TODO Week task\n`SCHEDULED: <2026-05-21 Thu>`\n",
    )
    .expect("write wk.md");

    let out = bin()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--agenda",
            "week",
            "--current-date",
            "2026-05-21",
            "--tz",
            "UTC",
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
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let expected = "\
[
  {
    \"date\": \"2026-05-18\",
    \"scheduled_timed\": [],
    \"scheduled_no_time\": [],
    \"upcoming\": []
  },
  {
    \"date\": \"2026-05-19\",
    \"scheduled_timed\": [],
    \"scheduled_no_time\": [],
    \"upcoming\": []
  },
  {
    \"date\": \"2026-05-20\",
    \"scheduled_timed\": [],
    \"scheduled_no_time\": [],
    \"upcoming\": []
  },
  {
    \"date\": \"2026-05-21\",
    \"scheduled_timed\": [],
    \"scheduled_no_time\": [
      {
        \"file\": \"wk.md\",
        \"line\": 3,
        \"heading\": \"Week task\",
        \"content\": \"\",
        \"task_type\": \"TODO\",
        \"timestamp\": \"SCHEDULED: <2026-05-21 Thu>\",
        \"timestamp_type\": \"SCHEDULED\",
        \"timestamp_active\": true,
        \"timestamp_date\": \"2026-05-21\"
      }
    ],
    \"upcoming\": []
  },
  {
    \"date\": \"2026-05-22\",
    \"scheduled_timed\": [],
    \"scheduled_no_time\": [],
    \"upcoming\": []
  },
  {
    \"date\": \"2026-05-23\",
    \"scheduled_timed\": [],
    \"scheduled_no_time\": [],
    \"upcoming\": []
  },
  {
    \"date\": \"2026-05-24\",
    \"scheduled_timed\": [],
    \"scheduled_no_time\": [],
    \"upcoming\": []
  }
]
";
    assert_eq!(
        stdout, expected,
        "JSON agenda-week snapshot must be byte-exact; got:\n{stdout}"
    );
}

#[test]
fn json_snapshot_agenda_month_envelope_shape() {
    // MIN-12: the month envelope reuses the same per-day object the week
    // snapshot pins byte-exactly, so rather than freeze a ~190-line
    // literal that breaks on every intentional edit, pin the month-window
    // contract structurally: 31 day objects for May 2026, spanning
    // 2026-05-01..2026-05-31, with the single task on the 21st. The
    // per-day key shape is already pinned by the week snapshot.
    let tmp = tempdir().expect("tmpdir");
    fs::write(
        tmp.path().join("mo.md"),
        "# Notes\n\n### TODO Month task\n`SCHEDULED: <2026-05-21 Thu>`\n",
    )
    .expect("write mo.md");

    let out = bin()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--agenda",
            "month",
            "--current-date",
            "2026-05-21",
            "--tz",
            "UTC",
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
    let stdout = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let days: Vec<serde_json::Value> = serde_json::from_str(&stdout).expect("valid JSON array");
    assert_eq!(days.len(), 31, "May 2026 has 31 day buckets");
    assert_eq!(days[0]["date"], "2026-05-01", "first bucket is the 1st");
    assert_eq!(days[30]["date"], "2026-05-31", "last bucket is the 31st");
    // Every bucket carries the four envelope keys.
    for (i, day) in days.iter().enumerate() {
        for key in ["date", "scheduled_timed", "scheduled_no_time", "upcoming"] {
            assert!(
                day.get(key).is_some(),
                "day {i} is missing the `{key}` envelope key: {day}"
            );
        }
    }
    // The task lands on the 21st under scheduled_no_time and nowhere else.
    let on_21 = &days[20];
    assert_eq!(on_21["date"], "2026-05-21");
    assert_eq!(
        on_21["scheduled_no_time"][0]["heading"], "Month task",
        "the task must sit on the 21st: {on_21}"
    );
    let total_tasks: usize = days
        .iter()
        .map(|d| d["scheduled_no_time"].as_array().map_or(0, |a| a.len()))
        .sum();
    assert_eq!(total_tasks, 1, "the task must appear on exactly one day");
}

// Output ends with a trailing newline regardless of format and destination.
// Rationale: POSIX defines a "text file" as ending in `\n`; without it the
// shell prompt is rendered on the same line as the last JSON `]`/HTML
// closing tag, and `diff` / line-counting tools mis-count the last line.
// Covers JSON / Markdown / HTML for both stdout and file outputs; the
// holiday short-circuit (`--holidays`) is exercised separately because it
// goes through a different write site (`handle_holidays`).

fn fixture_with_one_task() -> tempfile::TempDir {
    let tmp = tempdir().expect("tmpdir");
    fs::write(
        tmp.path().join("notes.md"),
        "# Notes\n\n### TODO Pin me\n`SCHEDULED: <2026-05-21 Thu>`\n",
    )
    .expect("write notes.md");
    tmp
}

fn run_with_format(tmp: &std::path::Path, format: &str) -> Vec<u8> {
    let out = bin()
        .args([
            "--dir",
            tmp.to_str().unwrap(),
            "--tasks",
            "--format",
            format,
            "--quiet",
        ])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

#[test]
fn stdout_json_ends_with_newline() {
    let tmp = fixture_with_one_task();
    let bytes = run_with_format(tmp.path(), "json");
    assert_eq!(
        bytes.last().copied(),
        Some(b'\n'),
        "JSON stdout must end with a trailing newline; got tail: {:?}",
        String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(8)..])
    );
}

#[test]
fn stdout_markdown_ends_with_newline() {
    let tmp = fixture_with_one_task();
    let bytes = run_with_format(tmp.path(), "markdown");
    assert_eq!(
        bytes.last().copied(),
        Some(b'\n'),
        "Markdown stdout must end with a trailing newline; got tail: {:?}",
        String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(8)..])
    );
}

#[test]
fn stdout_html_ends_with_newline() {
    let tmp = fixture_with_one_task();
    let bytes = run_with_format(tmp.path(), "html");
    assert_eq!(
        bytes.last().copied(),
        Some(b'\n'),
        "HTML stdout must end with a trailing newline; got tail: {:?}",
        String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(8)..])
    );
}

#[test]
fn output_file_ends_with_newline_for_each_format() {
    // The file-write path is `fs::write(p, output)`. Test all three formats
    // against the file path so a regression in only one format-stream pair
    // surfaces a precise failure rather than a generic "tail differs".
    let tmp = fixture_with_one_task();
    for format in ["json", "markdown", "html"] {
        let out_path = tmp.path().join(format!("out.{format}"));
        let result = bin()
            .args([
                "--dir",
                tmp.path().to_str().unwrap(),
                "--tasks",
                "--format",
                format,
                "--output",
                out_path.to_str().unwrap(),
                "--quiet",
            ])
            .output()
            .expect("run");
        assert!(
            result.status.success(),
            "format {format} failed to write: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let body = fs::read(&out_path).expect("read written file");
        assert_eq!(
            body.last().copied(),
            Some(b'\n'),
            "{} file output must end with a trailing newline; got tail: {:?}",
            format,
            String::from_utf8_lossy(&body[body.len().saturating_sub(8)..])
        );
    }
}

#[cfg(unix)]
#[test]
fn broken_pipe_exits_silently_without_diagnostic() {
    // Piping the binary into a consumer that closes the pipe (e.g.
    // `... | head -n 1`) used to surface `error: io: <stdout>: Broken
    // pipe (os error 32)` on stderr and a non-zero exit, even though
    // every Unix tool consuming the same pipeline is expected to terminate
    // quietly. Build a fixture large enough to exceed the typical 64 KB
    // pipe buffer so the write that fails is observed by the binary
    // (small outputs land entirely in the kernel buffer and the writer
    // never sees EPIPE).
    use std::path::PathBuf;
    use std::process::{Command as StdCommand, Stdio};

    let tmp = tempdir().expect("tmpdir");
    let block = "### TODO Task {{n}}\n`SCHEDULED: <2026-05-21 Thu>`\nContent line.\n\n";
    // 10 files × 100 tasks ≈ 1k entries ≈ ~200 KB of JSON — comfortably past
    // the typical 64 KiB pipe buffer so the binary observes EPIPE, without
    // making the test slow to generate.
    for i in 0..10 {
        let mut body = String::from("# Notes\n\n");
        for j in 0..100 {
            body.push_str(&block.replace("{{n}}", &format!("{i}_{j}")));
        }
        fs::write(tmp.path().join(format!("notes_{i:03}.md")), body).expect("write fixture file");
    }

    let bin_path: PathBuf = assert_cmd::cargo::cargo_bin("markdown-org-extract");
    let mut child = StdCommand::new(bin_path)
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--tasks",
            "--format",
            "json",
            "--quiet",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn binary");

    // Drop the read end of the stdout pipe immediately. The first write
    // from the binary that does not fit in the kernel buffer hits EPIPE.
    drop(child.stdout.take());

    let output = child.wait_with_output().expect("wait");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "binary must exit 0 on broken pipe; got status {:?}, stderr: {}",
        output.status,
        stderr
    );
    assert!(
        !stderr.contains("Broken pipe"),
        "stderr must not surface the broken-pipe error; got: {stderr}"
    );
    assert!(
        !stderr.contains("error:"),
        "stderr must not carry any 'error:' diagnostic for a broken pipe; got: {stderr}"
    );
}

/// End-to-end pin for the `-N<unit>` warning-period cookie on a DEADLINE.
/// At day 5 (outside the 3-day window) the task must not show as
/// upcoming, even though the default 14-day window would include it.
/// At day 2 (inside the 3-day window) the same task must show.
#[test]
fn deadline_warning_cookie_overrides_default_window() {
    let tmp = tempdir().expect("tmpdir");
    fs::write(
        tmp.path().join("notes.md"),
        "### TODO [#A] Cookie task\n`DEADLINE: <2025-12-10 Wed -3d>`\n",
    )
    .expect("write fixture");

    // Day 5 — outside the cookie's 3-day window. The default 14-day
    // window would have included this task, so a non-empty `upcoming`
    // here would mean the cookie is being ignored.
    let out = bin()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
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
        "scan must succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");
    let upcoming_at_5 = parsed
        .as_array()
        .and_then(|days| days.first())
        .and_then(|d| d.get("upcoming"))
        .and_then(|u| u.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(
        upcoming_at_5, 0,
        "DEADLINE with -3d must be silent 5 days out; full output: {parsed}"
    );

    // Day 8 — inside the 3-day window. Task must surface in upcoming.
    let out = bin()
        .args([
            "--dir",
            tmp.path().to_str().unwrap(),
            "--current-date",
            "2025-12-08",
            "--format",
            "json",
            "--quiet",
        ])
        .output()
        .expect("run");
    assert!(out.status.success());
    let parsed: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");
    let upcoming_at_8 = parsed
        .as_array()
        .and_then(|days| days.first())
        .and_then(|d| d.get("upcoming"))
        .and_then(|u| u.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(
        upcoming_at_8, 1,
        "DEADLINE with -3d must surface in upcoming 2 days out; full output: {parsed}"
    );
}

#[test]
fn verbose_saturation_warns_on_vvvv_and_beyond() {
    // `-vvvv` and longer maps to TRACE just like `-vvv` does. Silently
    // accepting it leaves a user who expected "more detail than trace" with
    // no signal that the level is already maxed out. A single warn on the
    // first overflow point is the cheapest acknowledgement that "-vvvv"
    // is the same as "-vvv".
    let out = bin()
        .args(["--dir", "examples", "--current-date", "2025-12-05", "-vvvv"])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "expected success on -vvvv; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("saturated") || stderr.contains("--verbose"),
        "expected verbose saturation message in stderr; got:\n{stderr}"
    );
}

#[test]
fn verbose_at_trace_threshold_does_not_warn() {
    // Negative control: `-vvv` is the documented trace level and must NOT
    // produce the saturation warning. Without this guard a regression that
    // moves the threshold off-by-one would slip through.
    let out = bin()
        .args(["--dir", "examples", "--current-date", "2025-12-05", "-vvv"])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "expected success on -vvv; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("saturated"),
        "expected no saturation message at -vvv; got:\n{stderr}"
    );
}

#[test]
fn holidays_stdout_ends_with_newline() {
    let out = bin()
        .args(["--holidays", "2026", "--quiet"])
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.stdout.last().copied(),
        Some(b'\n'),
        "--holidays JSON must end with a trailing newline; got tail: {:?}",
        String::from_utf8_lossy(&out.stdout[out.stdout.len().saturating_sub(8)..])
    );
}

#[test]
fn print_summary_aggregates_failed_paths_into_single_warn() {
    // The 2026-05-25 observability review (O5) flagged that
    // `ProcessingStats::print_summary` emitted up to 22 warn-level
    // records in a row (one summary, one header, and one per failed
    // path up to MAX_DIAGNOSTIC_ITEMS=20). This drowned out real
    // warnings on a noisy run. The aggregated form keeps everything
    // in one structured record: jq / grep can still extract the list
    // through a single field instead of stitching together multiple
    // lines.
    let dir = tempdir().unwrap();
    // Two files that pass the grep-searcher pre-filter (they contain
    // a `# TODO` keyword) but fail `std::str::from_utf8` because of an
    // explicit lone 0xFF byte. Both paths land in
    // `ProcessingStats::failed_paths` -> the previous code emitted three
    // separate warn records per path.
    for n in 0..2 {
        let path = dir.path().join(format!("bad{n}.md"));
        let mut bytes = b"# TODO test\n".to_vec();
        bytes.push(0xFF);
        bytes.extend_from_slice(b"\n");
        fs::write(&path, &bytes).unwrap();
    }

    let out = bin()
        .args([
            "--dir",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    let stderr = String::from_utf8_lossy(&out.stderr);

    let summary_lines: Vec<&str> = stderr
        .lines()
        .filter(|l| l.contains("processing summary"))
        .collect();
    assert_eq!(
        summary_lines.len(),
        1,
        "exactly one 'processing summary' warn line expected; stderr was:\n{stderr}"
    );

    assert!(
        !stderr.contains("failed paths (up to first"),
        "the per-list header line must be folded into the summary; stderr:\n{stderr}"
    );

    // Each individual failed_path line in the old format had a literal
    // `failed path` event message with a `path=...` field. The aggregated
    // form uses the plural `failed_paths` field name and emits no
    // standalone records.
    let standalone_path_lines = stderr.matches("\"failed path\"").count()
        + stderr
            .lines()
            .filter(|l| l.ends_with("failed path"))
            .count();
    assert_eq!(
        standalone_path_lines, 0,
        "no per-path 'failed path' records should remain; stderr:\n{stderr}"
    );

    assert!(
        summary_lines[0].contains("bad0.md") || summary_lines[0].contains("bad1.md"),
        "the aggregated summary must surface the failed paths; stderr:\n{stderr}"
    );
}

#[test]
fn per_file_failure_reason_is_logged_at_debug() {
    // m3 (2026-05-25 code / error-handling review): the three per-file
    // failure branches in scan_files (read / search / utf8) used to
    // discard the underlying io::Error / Utf8Error, recording only the
    // path. The error cause is now logged at debug level so `-vv`
    // explains *why* a path failed, while the default warn stream stays
    // aggregated (one summary record, per O5). This pins both halves:
    // the cause is present at -vv and absent at default verbosity.
    let dir = tempdir().unwrap();
    let path = dir.path().join("bad.md");
    // Passes the keyword pre-filter (`# TODO`) but fails str::from_utf8
    // on the lone 0xFF byte, taking the utf8 branch.
    let mut bytes = b"# TODO test\n".to_vec();
    bytes.push(0xFF);
    bytes.extend_from_slice(b"\n");
    fs::write(&path, &bytes).unwrap();

    // At -vv the per-file reason is visible.
    let verbose = bin()
        .args([
            "--dir",
            dir.path().to_str().unwrap(),
            "-vv",
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    let verbose_stderr = String::from_utf8_lossy(&verbose.stderr);
    assert!(
        verbose_stderr.contains("file is not valid UTF-8; skipping"),
        "at -vv the per-file failure reason must be logged; stderr was:\n{verbose_stderr}"
    );
    assert!(
        verbose_stderr.contains("bad.md"),
        "the per-file debug record must carry the path; stderr was:\n{verbose_stderr}"
    );

    // At default verbosity the per-file debug record is suppressed; the
    // path still appears once, in the aggregated summary warn.
    let quiet = bin()
        .args([
            "--dir",
            dir.path().to_str().unwrap(),
            "--current-date",
            "2025-12-05",
        ])
        .output()
        .expect("run");
    let quiet_stderr = String::from_utf8_lossy(&quiet.stderr);
    assert!(
        !quiet_stderr.contains("file is not valid UTF-8; skipping"),
        "at default verbosity the per-file debug record must be silent; stderr was:\n{quiet_stderr}"
    );
}

#[test]
fn utf8_bom_prefix_does_not_swallow_first_heading() {
    // Files saved by editors such as Windows Notepad or VS Code with the
    // "UTF-8 with BOM" option ship a leading EF BB BF byte sequence
    // (U+FEFF). CommonMark does not strip the BOM, so without explicit
    // handling the first heading line becomes "\u{FEFF}# TODO ..." and
    // the heading downgrades to a paragraph -- silently losing the task.
    // The encoding review (point 1) called this out as a real-world
    // regression for vault files originating on Windows.
    let dir = tempdir().unwrap();
    let path = dir.path().join("bom.md");
    let body = "# TODO BOM-prefixed heading\n\n`SCHEDULED: <2025-12-05 Fri>`\n";
    let mut content = Vec::with_capacity(3 + body.len());
    content.extend_from_slice(b"\xEF\xBB\xBF");
    content.extend_from_slice(body.as_bytes());
    fs::write(&path, &content).unwrap();

    let out = bin()
        .args([
            "--dir",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
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
    let stdout = String::from_utf8(out.stdout).expect("utf-8 stdout");
    assert!(
        stdout.contains("BOM-prefixed heading"),
        "BOM-prefixed first heading must still be extracted; stdout: {stdout}"
    );
    assert!(
        !stdout.contains('\u{FEFF}'),
        "BOM must not leak into the output text; stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"task_type\""),
        "task_type must survive BOM strip; stdout: {stdout}"
    );
    assert!(
        stdout.contains("\"TODO\""),
        "TODO marker must be parsed past the BOM; stdout: {stdout}"
    );
}

#[test]
fn rust_log_env_overrides_verbose_flag() {
    // ADR-0016 pins the precedence: `RUST_LOG` always wins over
    // `--verbose` / `--quiet`. With `-vv` the binary emits
    // `tracing::info!("scan finished")` on stderr; with
    // `RUST_LOG=error` the same level filter is muted.
    let baseline = bin()
        .args([
            "--dir",
            "examples",
            "--format",
            "json",
            "--current-date",
            "2025-12-05",
            "-vv",
        ])
        .env_remove("RUST_LOG")
        .output()
        .expect("baseline run");
    assert!(
        baseline.status.success(),
        "baseline stderr: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let baseline_err = String::from_utf8_lossy(&baseline.stderr);
    assert!(
        baseline_err.contains("scan finished"),
        "baseline -vv must emit info-level 'scan finished'; stderr: {baseline_err}"
    );

    let muted = bin()
        .args([
            "--dir",
            "examples",
            "--format",
            "json",
            "--current-date",
            "2025-12-05",
            "-vv",
        ])
        .env("RUST_LOG", "error")
        .output()
        .expect("muted run");
    assert!(
        muted.status.success(),
        "muted stderr: {}",
        String::from_utf8_lossy(&muted.stderr)
    );
    let muted_err = String::from_utf8_lossy(&muted.stderr);
    assert!(
        !muted_err.contains("scan finished"),
        "RUST_LOG=error must mute the -vv info line; stderr: {muted_err}"
    );
}

// Linux-only: the test must *create* a file whose name is not valid UTF-8,
// which only Linux allows (filenames are arbitrary non-NUL bytes). macOS
// APFS/HFS+ reject a non-Unicode filename at `fs::write`, so the scenario
// cannot even be set up there — exactly the platform analysis ADR-0019
// records (the lossy branch is unreachable on macOS). Windows is already
// excluded by `unix`. Gating macOS out keeps the macOS CI matrix green.
#[cfg(all(unix, not(target_os = "macos")))]
#[test]
fn non_utf8_path_is_processed_and_warned() {
    // ADR-0019: a file whose name is not valid UTF-8 (legal on Linux, where
    // filenames are arbitrary non-NUL byte sequences) is still read and its
    // tasks emitted — the I/O goes through the OsStr path, not the lossy
    // string. The `file` field is rendered with U+FFFD replacement chars, so
    // the tool warns once per run that the path will not round-trip. The file
    // content itself is valid UTF-8; only the name is not.
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let dir = tempdir().unwrap();
    // `bad\xFFname.md`: 0xFF is a lone invalid UTF-8 byte; the `.md` suffix is
    // intact so the default `*.md` glob still selects the file.
    let fname = OsString::from_vec(b"bad\xFFname.md".to_vec());
    let path = dir.path().join(&fname);
    fs::write(
        &path,
        "### TODO non utf8 path task\n`SCHEDULED: <2024-12-09 Mon>`\n",
    )
    .unwrap();

    let out = bin()
        .args([
            "--dir",
            dir.path().to_str().unwrap(),
            "--current-date",
            "2024-12-09",
        ])
        .output()
        .expect("run over a non-UTF-8 named file");

    assert!(
        out.status.success(),
        "scan must succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("non utf8 path task"),
        "the task from the non-UTF-8 named file must still be emitted; stdout: {stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("not valid UTF-8"),
        "a non-UTF-8 path must trigger a warning; stderr: {stderr}"
    );
}

#[test]
fn tasks_json_includes_properties_from_org_properties_block() {
    let dir = tempdir().unwrap();
    let content = "### TODO Ship release\n`SCHEDULED: <2026-06-01 Mon 10:00>`\n```org-properties\nGCAL_EVENT_ID: abc123/primary\nID: 11111111-2222-3333-4444-555555555555\n```\n\nBody.\n";
    fs::write(dir.path().join("t.md"), content).unwrap();

    let out = bin()
        .args([
            "--dir",
            dir.path().to_str().unwrap(),
            "--tasks",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let task = &parsed.as_array().expect("array of tasks")[0];
    assert_eq!(
        task["properties"]["GCAL_EVENT_ID"], "abc123/primary",
        "properties.GCAL_EVENT_ID must be in the JSON: {stdout}"
    );
    assert_eq!(
        task["properties"]["ID"], "11111111-2222-3333-4444-555555555555",
        "properties.ID must be in the JSON: {stdout}"
    );
}

#[test]
fn tasks_json_omits_properties_when_absent() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("t.md"),
        "### TODO No props\n`SCHEDULED: <2026-06-01 Mon>`\n",
    )
    .unwrap();

    let out = bin()
        .args([
            "--dir",
            dir.path().to_str().unwrap(),
            "--tasks",
            "--format",
            "json",
        ])
        .assert()
        .success();

    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(
        !stdout.contains("properties"),
        "absent properties must not appear in JSON: {stdout}"
    );
}
