//! Integration tests for `scripts/verify-archive.sh` and
//! `scripts/package-archive.sh`.
//!
//! These scripts back the `package-binaries` matrix in
//! `.github/workflows/release.yml`. `package-archive.sh` produces the
//! `.tar.gz` / `.zip` plus its `.sha256` companion. `verify-archive.sh`
//! enforces the downstream-packager contract documented in the README:
//! exact filename template, sibling `.sha256`, single top-level directory
//! matching the archive stem, and exactly the documented files inside
//! (binary, `README.md`, `LICENSE`).
//!
//! Unix-only: the scripts are POSIX bash. On the windows-latest GHA
//! runner Git for Windows bash plus CRLF defaults make `bash`-driven
//! integration tests unreliable; the scripts themselves run under
//! Git-bash on Windows, but verifying that path is the workflow's job.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn verify_script() -> PathBuf {
    manifest_dir().join("scripts").join("verify-archive.sh")
}

fn package_script() -> PathBuf {
    manifest_dir().join("scripts").join("package-archive.sh")
}

fn run_verify(asset: &Path, bin_name: &str) -> Output {
    Command::new("bash")
        .arg(verify_script())
        .arg(asset)
        .arg(bin_name)
        .output()
        .expect("invoke verify-archive.sh")
}

fn has_tool(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn sha256_for(dir: &Path, asset_name: &str) -> PathBuf {
    let out = Command::new("sha256sum")
        .arg(asset_name)
        .current_dir(dir)
        .output()
        .expect("sha256sum failed");
    assert!(out.status.success(), "sha256sum failed: {out:?}");
    let sha = dir.join(format!("{asset_name}.sha256"));
    fs::write(&sha, &out.stdout).expect("write sha256");
    sha
}

fn write_staged_files(stage: &Path, bin_name: &str) {
    fs::create_dir_all(stage).unwrap();
    fs::write(stage.join(bin_name), b"fake binary\n").unwrap();
    fs::write(stage.join("README.md"), b"fake readme\n").unwrap();
    fs::write(stage.join("LICENSE"), b"fake license\n").unwrap();
}

fn make_tar_gz_top_level(dir: &Path, stem: &str, bin_name: &str) -> PathBuf {
    let stage = dir.join(stem);
    write_staged_files(&stage, bin_name);
    let asset_name = format!("{stem}.tar.gz");
    let status = Command::new("tar")
        .arg("--sort=name")
        .arg("--owner=0")
        .arg("--group=0")
        .arg("--numeric-owner")
        .arg("--mtime=@0")
        .arg("-czf")
        .arg(dir.join(&asset_name))
        .arg("-C")
        .arg(dir)
        .arg(stem)
        .status()
        .expect("tar invocation");
    assert!(status.success(), "tar failed");
    sha256_for(dir, &asset_name);
    dir.join(asset_name)
}

fn make_flat_tar_gz(dir: &Path, stem: &str, bin_name: &str) -> PathBuf {
    let stage = dir.join("flat-stage");
    write_staged_files(&stage, bin_name);
    let asset_name = format!("{stem}.tar.gz");
    let status = Command::new("tar")
        .arg("-czf")
        .arg(dir.join(&asset_name))
        .arg("-C")
        .arg(&stage)
        .arg(".")
        .status()
        .expect("tar invocation");
    assert!(status.success(), "tar failed");
    sha256_for(dir, &asset_name);
    dir.join(asset_name)
}

fn make_zip_top_level(dir: &Path, stem: &str, bin_name: &str) -> PathBuf {
    let stage = dir.join(stem);
    write_staged_files(&stage, bin_name);
    let asset_name = format!("{stem}.zip");
    let status = Command::new("7z")
        .arg("a")
        .arg("-tzip")
        .arg("-mtc=off")
        .arg(dir.join(&asset_name))
        .arg(stem)
        .current_dir(dir)
        .stdout(std::process::Stdio::null())
        .status()
        .expect("7z invocation");
    assert!(status.success(), "7z failed");
    sha256_for(dir, &asset_name);
    dir.join(asset_name)
}

fn make_flat_zip(dir: &Path, stem: &str, bin_name: &str) -> PathBuf {
    let stage = dir.join("flat-zip-stage");
    write_staged_files(&stage, bin_name);
    let asset_name = format!("{stem}.zip");
    // Reproduces the original release.yml bug: ${stage}/* glob stores files
    // flat at the archive root rather than under ${stem}/.
    let status = Command::new("7z")
        .arg("a")
        .arg("-tzip")
        .arg("-mtc=off")
        .arg(dir.join(&asset_name))
        .arg(format!("{}/*", stage.display()))
        .stdout(std::process::Stdio::null())
        .status()
        .expect("7z invocation");
    assert!(status.success(), "7z failed");
    sha256_for(dir, &asset_name);
    dir.join(asset_name)
}

#[test]
fn verify_passes_on_well_formed_tar_gz() {
    let tmp = tempdir().unwrap();
    let stem = "markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu";
    let asset = make_tar_gz_top_level(tmp.path(), stem, "markdown-org-extract");
    let out = run_verify(&asset, "markdown-org-extract");
    assert!(
        out.status.success(),
        "expected verify to pass; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn verify_passes_on_well_formed_zip() {
    if !has_tool("7z") {
        eprintln!("skipping: 7z not available");
        return;
    }
    let tmp = tempdir().unwrap();
    let stem = "markdown-org-extract-0.3.1-x86_64-pc-windows-msvc";
    let asset = make_zip_top_level(tmp.path(), stem, "markdown-org-extract.exe");
    let out = run_verify(&asset, "markdown-org-extract.exe");
    assert!(
        out.status.success(),
        "expected verify to pass; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn verify_rejects_tar_gz_without_top_level_dir() {
    let tmp = tempdir().unwrap();
    let stem = "markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu";
    let asset = make_flat_tar_gz(tmp.path(), stem, "markdown-org-extract");
    let out = run_verify(&asset, "markdown-org-extract");
    assert!(
        !out.status.success(),
        "expected verify to reject flat tar.gz"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("top-level") || stderr.contains("root"),
        "stderr should mention top-level/root: {stderr}"
    );
}

#[test]
fn verify_rejects_flat_zip() {
    if !has_tool("7z") {
        eprintln!("skipping: 7z not available");
        return;
    }
    let tmp = tempdir().unwrap();
    let stem = "markdown-org-extract-0.3.1-x86_64-pc-windows-msvc";
    let asset = make_flat_zip(tmp.path(), stem, "markdown-org-extract.exe");
    let out = run_verify(&asset, "markdown-org-extract.exe");
    assert!(
        !out.status.success(),
        "expected verify to reject flat zip (the original release.yml bug)"
    );
}

#[test]
fn verify_rejects_missing_sha256() {
    let tmp = tempdir().unwrap();
    let stem = "markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu";
    let asset = make_tar_gz_top_level(tmp.path(), stem, "markdown-org-extract");
    let sha = PathBuf::from(format!("{}.sha256", asset.display()));
    fs::remove_file(&sha).unwrap();
    let out = run_verify(&asset, "markdown-org-extract");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("sha256"), "stderr: {stderr}");
}

#[test]
fn verify_rejects_mismatching_sha256() {
    let tmp = tempdir().unwrap();
    let stem = "markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu";
    let asset = make_tar_gz_top_level(tmp.path(), stem, "markdown-org-extract");
    let sha = PathBuf::from(format!("{}.sha256", asset.display()));
    let asset_file = asset.file_name().unwrap().to_string_lossy().into_owned();
    fs::write(&sha, format!("{}  {}\n", "0".repeat(64), asset_file)).unwrap();
    let out = run_verify(&asset, "markdown-org-extract");
    assert!(
        !out.status.success(),
        "expected sha256 mismatch to fail verify"
    );
}

#[test]
fn verify_rejects_archive_with_extra_file() {
    let tmp = tempdir().unwrap();
    let stem = "markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu";
    let stage = tmp.path().join(stem);
    write_staged_files(&stage, "markdown-org-extract");
    fs::write(stage.join("EXTRA.txt"), b"extra\n").unwrap();
    let asset_name = format!("{stem}.tar.gz");
    let status = Command::new("tar")
        .arg("-czf")
        .arg(tmp.path().join(&asset_name))
        .arg("-C")
        .arg(tmp.path())
        .arg(stem)
        .status()
        .expect("tar");
    assert!(status.success());
    sha256_for(tmp.path(), &asset_name);
    let asset = tmp.path().join(&asset_name);
    let out = run_verify(&asset, "markdown-org-extract");
    assert!(!out.status.success(), "expected reject on extra file");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unexpected") || stderr.contains("EXTRA"),
        "stderr: {stderr}"
    );
}

#[test]
fn verify_rejects_archive_with_missing_required_file() {
    let tmp = tempdir().unwrap();
    let stem = "markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu";
    let stage = tmp.path().join(stem);
    fs::create_dir_all(&stage).unwrap();
    fs::write(stage.join("markdown-org-extract"), b"fake\n").unwrap();
    fs::write(stage.join("README.md"), b"fake\n").unwrap();
    // LICENSE intentionally missing
    let asset_name = format!("{stem}.tar.gz");
    let status = Command::new("tar")
        .arg("-czf")
        .arg(tmp.path().join(&asset_name))
        .arg("-C")
        .arg(tmp.path())
        .arg(stem)
        .status()
        .expect("tar");
    assert!(status.success());
    sha256_for(tmp.path(), &asset_name);
    let asset = tmp.path().join(&asset_name);
    let out = run_verify(&asset, "markdown-org-extract");
    assert!(!out.status.success(), "expected reject on missing LICENSE");
}

#[test]
fn verify_rejects_filename_not_matching_template() {
    let tmp = tempdir().unwrap();
    let stem = "markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu";
    let asset = make_tar_gz_top_level(tmp.path(), stem, "markdown-org-extract");
    let renamed = tmp.path().join("wrong-name-0.3.1.tar.gz");
    fs::rename(&asset, &renamed).unwrap();
    // sha256 companion intentionally not renamed (verification of pattern
    // fires before sha256, so the test isolates the filename check).
    let out = run_verify(&renamed, "markdown-org-extract");
    assert!(!out.status.success(), "expected reject on bad filename");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("template") || stderr.contains("filename") || stderr.contains("pattern"),
        "stderr: {stderr}"
    );
}

#[test]
fn verify_rejects_missing_asset() {
    let tmp = tempdir().unwrap();
    let asset = tmp
        .path()
        .join("markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu.tar.gz");
    let out = run_verify(&asset, "markdown-org-extract");
    assert!(!out.status.success(), "expected reject on missing asset");
}

#[test]
fn package_archive_tar_gz_produces_verifiable_layout() {
    let tmp = tempdir().unwrap();
    let bin_path = tmp.path().join("markdown-org-extract");
    fs::write(&bin_path, b"fake binary\n").unwrap();
    let readme = tmp.path().join("README.md");
    let license = tmp.path().join("LICENSE");
    fs::write(&readme, b"fake readme\n").unwrap();
    fs::write(&license, b"fake license\n").unwrap();
    let runner_temp = tmp.path().join("runner-temp");
    fs::create_dir_all(&runner_temp).unwrap();
    let out_dir = tmp.path().join("out");
    fs::create_dir_all(&out_dir).unwrap();

    let output = Command::new("bash")
        .arg(package_script())
        .env("VER", "0.3.1")
        .env("TARGET", "x86_64-unknown-linux-gnu")
        .env("ARCHIVE_EXT", "tar.gz")
        .env("BIN_NAME", "markdown-org-extract")
        .env("BIN_PATH", &bin_path)
        .env("RUNNER_TEMP", &runner_temp)
        .env("OUTPUT_DIR", &out_dir)
        .current_dir(tmp.path())
        .output()
        .expect("package-archive.sh");

    assert!(
        output.status.success(),
        "package-archive.sh failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|l| l == "asset=markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu.tar.gz"),
        "stdout should announce asset filename: {stdout}"
    );
    let asset = out_dir.join("markdown-org-extract-0.3.1-x86_64-unknown-linux-gnu.tar.gz");
    let v = run_verify(&asset, "markdown-org-extract");
    assert!(
        v.status.success(),
        "verify-archive.sh rejected the package output; stderr: {}",
        String::from_utf8_lossy(&v.stderr)
    );
}

#[test]
fn package_archive_zip_produces_verifiable_layout() {
    if !has_tool("7z") {
        eprintln!("skipping: 7z not available");
        return;
    }
    let tmp = tempdir().unwrap();
    let bin_path = tmp.path().join("markdown-org-extract.exe");
    fs::write(&bin_path, b"fake binary\n").unwrap();
    fs::write(tmp.path().join("README.md"), b"fake readme\n").unwrap();
    fs::write(tmp.path().join("LICENSE"), b"fake license\n").unwrap();
    let runner_temp = tmp.path().join("runner-temp");
    fs::create_dir_all(&runner_temp).unwrap();
    let out_dir = tmp.path().join("out");
    fs::create_dir_all(&out_dir).unwrap();

    let output = Command::new("bash")
        .arg(package_script())
        .env("VER", "0.3.1")
        .env("TARGET", "x86_64-pc-windows-msvc")
        .env("ARCHIVE_EXT", "zip")
        .env("BIN_NAME", "markdown-org-extract.exe")
        .env("BIN_PATH", &bin_path)
        .env("RUNNER_TEMP", &runner_temp)
        .env("OUTPUT_DIR", &out_dir)
        .current_dir(tmp.path())
        .output()
        .expect("package-archive.sh");

    assert!(
        output.status.success(),
        "package-archive.sh failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let asset = out_dir.join("markdown-org-extract-0.3.1-x86_64-pc-windows-msvc.zip");
    let v = run_verify(&asset, "markdown-org-extract.exe");
    assert!(
        v.status.success(),
        "verify-archive.sh rejected the package output; stderr: {}",
        String::from_utf8_lossy(&v.stderr)
    );
}
