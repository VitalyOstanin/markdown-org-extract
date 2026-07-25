//! Integration tests for `scripts/generate-third-party-licenses.sh`.
//!
//! The script renders `THIRD-PARTY-LICENSES.txt`: the crates that end up in
//! the published binary, their SPDX expression and source URL, plus the full
//! text of every licence file they ship. The file is committed and shipped
//! inside the release archives, and CI regenerates it to catch staleness, so
//! the format has to be deterministic down to the byte.
//!
//! The tests drive the script with a fake `cargo` (canned `tree` and
//! `metadata` output) pointing at a fake crate registry in a tempdir, so no
//! network, no real dependency graph, and no dependence on which crates the
//! project happens to use today.
//!
//! Unix-only: the script is POSIX bash, same reasoning as
//! `release_packaging.rs`.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::{tempdir, TempDir};

const APACHE_TEXT: &str =
    "Apache License\nVersion 2.0, January 2004\n(shared verbatim by several crates)\n";
const MIT_ALPHA_TEXT: &str = "MIT License\n\nCopyright (c) 2020 Alpha Author\n";
const BSD_GAMMA_TEXT: &str = "BSD 2-Clause License\n\nCopyright (c) 2019 Gamma Author\n";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn generator_script() -> PathBuf {
    manifest_dir()
        .join("scripts")
        .join("generate-third-party-licenses.sh")
}

/// A fake crate directory as cargo would unpack it into the registry:
/// `<registry>/<name>-<version>/` with a `Cargo.toml` and licence files.
fn write_fake_crate(registry: &Path, name: &str, version: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = registry.join(format!("{name}-{version}"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\n"),
    )
    .unwrap();
    for (file_name, body) in files {
        fs::write(dir.join(file_name), body).unwrap();
    }
    dir
}

struct Fixture {
    tmp: TempDir,
    cargo: PathBuf,
    out: PathBuf,
}

/// One entry of the fake dependency graph.
struct FakeCrate {
    name: &'static str,
    version: &'static str,
    license: &'static str,
    repository: Option<&'static str>,
    licence_files: Vec<(&'static str, &'static str)>,
}

/// Builds the standard fixture: three third-party crates plus the workspace
/// root. `omit_licence_files_for` drops the licence files of one crate so the
/// fail-closed path can be exercised.
fn fixture(omit_licence_files_for: Option<&str>) -> Fixture {
    let tmp = tempdir().unwrap();
    let registry = tmp.path().join("registry");
    fs::create_dir_all(&registry).unwrap();

    let mut crates = vec![
        FakeCrate {
            name: "alpha",
            version: "1.0.0",
            license: "MIT OR Apache-2.0",
            repository: Some("https://example.invalid/alpha"),
            licence_files: vec![
                ("LICENSE-APACHE", APACHE_TEXT),
                ("LICENSE-MIT", MIT_ALPHA_TEXT),
            ],
        },
        FakeCrate {
            name: "beta",
            version: "2.3.4",
            license: "Apache-2.0",
            repository: Some("https://example.invalid/beta"),
            licence_files: vec![("LICENSE-APACHE", APACHE_TEXT)],
        },
        FakeCrate {
            name: "gamma",
            version: "0.1.0",
            license: "BSD-2-Clause",
            repository: None,
            licence_files: vec![("COPYING", BSD_GAMMA_TEXT)],
        },
    ];
    if let Some(name) = omit_licence_files_for {
        for entry in crates.iter_mut() {
            if entry.name == name {
                entry.licence_files.clear();
            }
        }
    }

    let mut packages: Vec<String> = Vec::new();
    for entry in &crates {
        let dir = write_fake_crate(&registry, entry.name, entry.version, &entry.licence_files);
        let repository = match entry.repository {
            Some(url) => format!("\"{url}\""),
            None => "null".to_string(),
        };
        packages.push(format!(
            "{{\"name\":\"{name}\",\"version\":\"{version}\",\"license\":\"{license}\",\
             \"license_file\":null,\"repository\":{repository},\
             \"manifest_path\":\"{manifest}\"}}",
            name = entry.name,
            version = entry.version,
            license = entry.license,
            manifest = dir.join("Cargo.toml").display(),
        ));
    }
    // The workspace root itself: present in `cargo metadata`, and the only
    // tree entry carrying a path suffix.
    packages.push(format!(
        "{{\"name\":\"myroot\",\"version\":\"9.9.9\",\"license\":\"MIT\",\
         \"license_file\":null,\"repository\":null,\"manifest_path\":\"{}\"}}",
        tmp.path().join("Cargo.toml").display(),
    ));

    let metadata = format!("{{\"packages\":[{}]}}\n", packages.join(","));
    fs::write(tmp.path().join("metadata.json"), metadata).unwrap();

    // `cargo tree` output as the script consumes it: the root carries a path
    // suffix, repeats are marked `(*)`, proc-macro crates `(proc-macro)`.
    let tree = "myroot v9.9.9 (/somewhere/myroot)\n\
                alpha v1.0.0\n\
                beta v2.3.4 (proc-macro)\n\
                gamma v0.1.0\n\
                alpha v1.0.0 (*)\n";
    fs::write(tmp.path().join("tree.txt"), tree).unwrap();

    let cargo = tmp.path().join("fake-cargo");
    fs::write(
        &cargo,
        format!(
            r#"#!/usr/bin/env bash
case "${{1:-}}" in
  tree) cat "{tree}" ;;
  metadata) cat "{metadata}" ;;
  *) echo "fake cargo: unexpected invocation: $*" >&2; exit 1 ;;
esac
"#,
            tree = tmp.path().join("tree.txt").display(),
            metadata = tmp.path().join("metadata.json").display(),
        ),
    )
    .unwrap();
    let mut perms = fs::metadata(&cargo).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&cargo, perms).unwrap();

    let out = tmp.path().join("THIRD-PARTY-LICENSES.txt");
    Fixture { tmp, cargo, out }
}

fn run(fixture: &Fixture) -> Output {
    Command::new("bash")
        .arg(generator_script())
        .arg(&fixture.out)
        .env("CARGO", &fixture.cargo)
        .current_dir(fixture.tmp.path())
        .output()
        .expect("invoke generate-third-party-licenses.sh")
}

fn generated(fixture: &Fixture) -> String {
    let out = run(fixture);
    assert!(
        out.status.success(),
        "generator failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    fs::read_to_string(&fixture.out).expect("read generated file")
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The notice index a crate entry points at, e.g. `[2]` for `LICENSE-MIT`.
fn notice_index(rendered: &str, crate_line: &str, licence_file: &str) -> String {
    let entry = rendered
        .split(crate_line)
        .nth(1)
        .unwrap_or_else(|| panic!("crate entry not found: {crate_line}\n{rendered}"));
    let notices = entry
        .lines()
        .find(|l| l.trim_start().starts_with("Notices:"))
        .unwrap_or_else(|| panic!("no Notices line after {crate_line}"));
    for part in notices.split(", ") {
        if part.trim_end().ends_with(licence_file) {
            let start = part.find('[').expect("notice reference is bracketed");
            let end = part.find(']').expect("notice reference is bracketed");
            return part[start..=end].to_string();
        }
    }
    panic!("{licence_file} not referenced in: {notices}");
}

#[test]
fn lists_every_third_party_crate_with_version_licence_and_source() {
    let f = fixture(None);
    let rendered = generated(&f);
    assert!(rendered.contains("alpha 1.0.0"), "{rendered}");
    assert!(rendered.contains("beta 2.3.4"), "{rendered}");
    assert!(rendered.contains("gamma 0.1.0"), "{rendered}");
    assert!(rendered.contains("MIT OR Apache-2.0"), "{rendered}");
    assert!(
        rendered.contains("https://example.invalid/alpha"),
        "{rendered}"
    );
}

#[test]
fn omits_the_workspace_root_from_the_crate_list() {
    let f = fixture(None);
    let rendered = generated(&f);
    let crates_section = rendered
        .split("CRATES")
        .nth(1)
        .expect("CRATES section present");
    assert!(
        !crates_section.contains("myroot 9.9.9"),
        "the crate being licensed must not list itself as a third party:\n{crates_section}"
    );
}

#[test]
fn names_the_crate_being_licensed_without_pinning_its_version() {
    let f = fixture(None);
    let rendered = generated(&f);
    let header = rendered.split("CRATES").next().unwrap();
    assert!(
        header.contains("myroot"),
        "header should name the distributed crate:\n{header}"
    );
    assert!(
        !rendered.contains("9.9.9"),
        "the notice file must not carry the project's own version: it would \
         then go stale on every release bump, failing CI for no licence reason"
    );
}

#[test]
fn reproduces_each_licence_text_in_full() {
    let f = fixture(None);
    let rendered = generated(&f);
    assert!(rendered.contains(MIT_ALPHA_TEXT.trim_end()), "{rendered}");
    assert!(rendered.contains(BSD_GAMMA_TEXT.trim_end()), "{rendered}");
    assert!(rendered.contains(APACHE_TEXT.trim_end()), "{rendered}");
}

#[test]
fn shares_one_notice_between_crates_with_identical_texts() {
    let f = fixture(None);
    let rendered = generated(&f);
    assert_eq!(
        count(&rendered, "Version 2.0, January 2004"),
        1,
        "an identical licence text must be emitted once, not per crate:\n{rendered}"
    );
    let from_alpha = notice_index(&rendered, "alpha 1.0.0", "LICENSE-APACHE");
    let from_beta = notice_index(&rendered, "beta 2.3.4", "LICENSE-APACHE");
    assert_eq!(
        from_alpha, from_beta,
        "both crates must point at the same notice"
    );
    assert!(
        rendered.contains(&format!("NOTICE {from_alpha}")),
        "the referenced notice must exist:\n{rendered}"
    );
}

#[test]
fn marks_a_shared_notice_with_the_number_of_crates() {
    let f = fixture(None);
    let rendered = generated(&f);
    assert!(
        rendered.contains("shared by 2 crates"),
        "a notice used by several crates should say so:\n{rendered}"
    );
}

#[test]
fn counts_a_crate_once_despite_repeat_and_proc_macro_markers() {
    let f = fixture(None);
    let rendered = generated(&f);
    let crates_section = rendered.split("NOTICE [").next().unwrap();
    assert_eq!(
        count(crates_section, "alpha 1.0.0"),
        1,
        "the `(*)` repeat marker must not duplicate a crate:\n{crates_section}"
    );
    assert_eq!(
        count(crates_section, "beta 2.3.4"),
        1,
        "the `(proc-macro)` marker must not be part of the crate name:\n{crates_section}"
    );
    assert!(
        !crates_section.contains("(proc-macro)") && !crates_section.contains("(*)"),
        "cargo tree markers must not leak into the output:\n{crates_section}"
    );
}

#[test]
fn fails_when_a_crate_ships_no_licence_text() {
    let f = fixture(Some("gamma"));
    let out = run(&f);
    assert!(
        !out.status.success(),
        "a crate whose licence text cannot be shipped must fail the build, \
         not be silently dropped"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("gamma"),
        "the error must name the offending crate: {stderr}"
    );
    assert!(
        !f.out.exists(),
        "a failed run must not leave a partial file behind"
    );
}

#[test]
fn is_byte_for_byte_reproducible() {
    let f = fixture(None);
    let first = generated(&f);
    let second = generated(&f);
    assert_eq!(
        first, second,
        "CI compares a regenerated file against the committed one, so the \
         output must not depend on run order or filesystem order"
    );
}

#[test]
fn writes_the_repository_file_when_no_path_is_given() {
    let f = fixture(None);
    let out = Command::new("bash")
        .arg(generator_script())
        .env("CARGO", &f.cargo)
        .env("OUTPUT_ROOT", f.tmp.path())
        .current_dir(f.tmp.path())
        .output()
        .expect("invoke generator without an explicit path");
    assert!(
        out.status.success(),
        "generator failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        f.tmp.path().join("THIRD-PARTY-LICENSES.txt").is_file(),
        "the default output path is THIRD-PARTY-LICENSES.txt at the repository root"
    );
}

#[test]
fn committed_notice_file_covers_the_current_dependency_graph() {
    // Guards the shipped artefact itself: every crate name printed by the
    // real `cargo tree` must appear in the committed file. CI additionally
    // regenerates and diffs; this catches a stale file in a local checkout
    // before the release workflow does.
    let notices = manifest_dir().join("THIRD-PARTY-LICENSES.txt");
    let rendered = fs::read_to_string(&notices).expect("THIRD-PARTY-LICENSES.txt is committed");
    let tree = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args([
            "tree",
            "--edges",
            "normal",
            "--target",
            "all",
            "--prefix",
            "none",
            "--format",
            "{p}",
            "--offline",
        ])
        .current_dir(manifest_dir())
        .output()
        .expect("invoke cargo tree");
    if !tree.status.success() {
        eprintln!("skipping: cargo tree unavailable offline");
        return;
    }
    let stdout = String::from_utf8_lossy(&tree.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.contains(" (/") {
            continue; // the workspace root itself
        }
        let mut parts = line.split(" v");
        let name = parts.next().unwrap();
        let version = match parts.next() {
            Some(rest) => rest.split_whitespace().next().unwrap(),
            None => continue,
        };
        assert!(
            rendered.contains(&format!("{name} {version}")),
            "{name} {version} is linked but missing from THIRD-PARTY-LICENSES.txt; \
             run scripts/generate-third-party-licenses.sh"
        );
    }
}
