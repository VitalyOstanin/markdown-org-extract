//! Signal-handling integration tests. Unix-only: SIGINT semantics on Windows
//! differ enough (console events vs POSIX signals) that the verification here
//! would not transfer. The test spawns the CLI, sends SIGINT mid-scan via the
//! POSIX `kill` utility, and asserts the conventional `128 + SIGINT = 130`
//! exit code together with the partial summary on stderr.
#![cfg(unix)]

use assert_cmd::cargo::CommandCargoExt;
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

/// Populate `dir` with `count` tiny markdown files, each carrying a single
/// timestamp so the parser actually has work to do per file. Sequential
/// creation; on a typical SSD this is ~1–3s for 30k files, which is fine for
/// a single integration test.
fn populate_files(dir: &std::path::Path, count: usize) {
    for i in 0..count {
        let path = dir.join(format!("note-{i:06}.md"));
        // Single TODO with a SCHEDULED timestamp. Keeps the parser engaged
        // (regex search hits, extract_tasks runs) so per-file walk time is
        // non-trivial without bloating the test fixture.
        fs::write(
            &path,
            b"# TODO Task\n\n`<2025-12-05 Fri 10:00>` SCHEDULED\n",
        )
        .expect("write fixture");
    }
}

#[test]
fn sigint_during_scan_exits_with_130_and_partial_summary() {
    // Generous file count so the walker is guaranteed to be in-flight when
    // the test sends SIGINT. Setup cost (~2s) is the price of removing flake
    // risk: a smaller fixture would race with the scan finishing first.
    let dir = tempdir().expect("tempdir");
    populate_files(dir.path(), 30_000);

    let child = Command::cargo_bin("markdown-org-extract")
        .expect("locate built binary")
        .args([
            "--dir",
            &dir.path().display().to_string(),
            "--format",
            "json",
            "--current-date",
            "2025-12-05",
            "-vv",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn cli");

    let pid = child.id() as i32;

    // Two timing concerns govern the kill loop:
    // 1. First SIGINT must arrive *after* the child installs its handler,
    //    otherwise the default disposition kills the process and the exit
    //    code is `signal 2` (None from std), not 130.
    // 2. SIGINT must arrive *before* the walker finishes its 30k-file scan,
    //    otherwise the flag is set but the loop has already exited and the
    //    process completes normally with code 0.
    //
    // 100 ms initial wait is comfortably above debug-binary startup
    // (~10–30 ms on this project) and small relative to walker time for the
    // 30k-file fixture (>500 ms even on a fast SSD). The 25 ms retry
    // interval keeps re-flipping the flag during the scan window.
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            // Silence `kill: No such process` messages once the child has
            // exited and the loop is just running out the timeout — they
            // are not failures, just retry noise.
            let _ = Command::new("kill")
                .args(["-INT", &pid.to_string()])
                .stderr(Stdio::null())
                .status();
            thread::sleep(Duration::from_millis(25));
        }
    });

    let output = child.wait_with_output().expect("wait for child");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let code = output.status.code();

    assert_eq!(
        code,
        Some(130),
        "expected exit code 130 after SIGINT; got status={:?}, stderr=\n{stderr}",
        output.status,
    );
    assert!(
        stderr.contains("interrupted"),
        "expected 'interrupted' marker in stderr summary; got:\n{stderr}",
    );
    assert!(
        stderr.contains("processing summary"),
        "expected 'processing summary' line in stderr; got:\n{stderr}",
    );
}

#[test]
fn normal_completion_does_not_mark_interrupted() {
    // Negative-control test: a quick scan with no signal must finish with
    // status 0 and no 'interrupted' marker. Pairs with the SIGINT test so
    // a regression that always sets the flag would still be caught.
    let dir = tempdir().expect("tempdir");
    populate_files(dir.path(), 10);

    let output = Command::cargo_bin("markdown-org-extract")
        .expect("locate built binary")
        .args([
            "--dir",
            &dir.path().display().to_string(),
            "--format",
            "json",
            "--current-date",
            "2025-12-05",
            "-vv",
        ])
        .output()
        .expect("run cli");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected success; status={:?}, stderr=\n{stderr}",
        output.status,
    );
    assert!(
        !stderr.contains("interrupted = true"),
        "uninterrupted run must not advertise 'interrupted = true'; got:\n{stderr}",
    );
}
