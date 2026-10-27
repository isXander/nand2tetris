//! End-to-end checks of the `n2t` binary against the repository's samples.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn samples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../samples")
        .canonicalize()
        .unwrap()
}

fn n2t(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_n2t"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Copies a directory tree into a fresh temporary folder (scripts write `.out` files).
fn scratch_copy(from: &Path, tag: &str) -> PathBuf {
    fn copy(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap().map(Result::unwrap) {
            let path = entry.path();
            if path.is_dir() {
                copy(&path, &to.join(entry.file_name()));
            } else {
                fs::copy(&path, to.join(entry.file_name())).unwrap();
            }
        }
    }
    let target = std::env::temp_dir().join(format!("n2t-cli-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&target);
    copy(from, &target);
    target
}

#[test]
fn check_accepts_the_projects() {
    let (code, _, stderr) = n2t(&["check", samples().join("projects").to_str().unwrap()]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains(" 0 errors, 0 warnings"), "{stderr}");
}

#[test]
fn check_reports_errors_with_exit_code_1() {
    let (code, _, stderr) = n2t(&[
        "check",
        samples()
            .join("errors/hdl/WidthMismatch.hdl")
            .to_str()
            .unwrap(),
    ]);
    assert_eq!(code, 1);
    assert!(stderr.contains("error[hdl-width-mismatch]"), "{stderr}");
    assert!(stderr.contains("WidthMismatch.hdl:6:9"), "{stderr}");
}

#[test]
fn test_runs_the_projects() {
    let dir = scratch_copy(&samples().join("projects"), "projects");
    let (code, stdout, stderr) = n2t(&["test", dir.to_str().unwrap()]);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, 0, "{stdout}{stderr}");
    assert!(stdout.contains("33 passed, 0 failed"), "{stdout}");
}

#[test]
fn test_reports_a_comparison_failure() {
    let dir = scratch_copy(&samples().join("runtime"), "runtime");
    let (code, stdout, stderr) = n2t(&["test", dir.join("Mismatch.tst").to_str().unwrap()]);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(code, 1);
    assert!(stdout.contains("FAIL"), "{stdout}");
    assert!(
        stderr.contains("comparison failure at line 2, column 11"),
        "{stderr}"
    );
}

#[test]
fn eval_prints_pins() {
    let alu = samples().join("projects/02/ALU.hdl");
    // x + y with x = 2, y = 3.
    let (code, stdout, stderr) = n2t(&["eval", alu.to_str().unwrap(), "x=2", "y=3", "f=1"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("OUT out  0000000000000101  (5)"),
        "{stdout}"
    );
}

#[test]
fn missing_path_exits_with_code_2() {
    let (code, _, stderr) = n2t(&["check", "definitely/not/here"]);
    assert_eq!(code, 2);
    assert!(stderr.contains("does not exist"), "{stderr}");
}
