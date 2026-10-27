//! Runs the test scripts in the repository's `samples/` folder.
//!
//! - Every script in `samples/projects` and `samples/features` must pass
//!   against its `.cmp` file.
//! - Each script in `samples/runtime` has a `<script>.expected` golden with
//!   its rendered outcome. Set `N2T_BLESS=1` to rewrite the goldens, then
//!   review the diff.
//!
//! Scripts run in a temporary copy of their folder, so the `.out` files they
//! write never land in `samples/`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use n2t_core::LineIndex;
use n2t_core::builtins::BuiltinLibrary;
use n2t_simulator::{BuiltinRegistry, Outcome, RunnerOptions, ScriptRunner, StartError};

fn samples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../samples")
        .canonicalize()
        .unwrap()
}

fn scripts(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap().map(Result::unwrap) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "tst") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Copies a folder's files (not subfolders) into a fresh temporary folder.
fn scratch_copy(dir: &Path, tag: &str) -> PathBuf {
    let target = std::env::temp_dir().join(format!("n2t-samples-{}-{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&target);
    fs::create_dir_all(&target).unwrap();
    for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
        if entry.path().is_file() {
            fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
        }
    }
    target.canonicalize().unwrap()
}

/// Runs a script, resuming past `!` pauses and breakpoints. Returns the final
/// outcome and the suspensions seen on the way.
fn run(script: &Path, max_commands: u64) -> Result<(Outcome, Vec<String>), StartError> {
    let options = RunnerOptions {
        max_commands: Some(max_commands),
        cancel: None,
    };
    let mut runner = ScriptRunner::open(
        script,
        Arc::new(BuiltinLibrary::standard()),
        BuiltinRegistry::standard(),
        options,
    )?;
    let text = fs::read_to_string(script).unwrap();
    let lines = LineIndex::new(&text);
    let mut suspensions = Vec::new();
    loop {
        match runner.run() {
            Outcome::Suspended { reason, span } => {
                let (line, column) = lines.line_col_one_based(&text, span.start);
                suspensions.push(format!("suspended ({reason:?}) at {line}:{column}"));
            }
            outcome => return Ok((outcome, suspensions)),
        }
    }
}

fn location(script: &Path, span: Option<n2t_core::Span>) -> String {
    let Some(span) = span else {
        return String::new();
    };
    let text = fs::read_to_string(script).unwrap();
    let (line, column) = LineIndex::new(&text).line_col_one_based(&text, span.start);
    format!(" at {line}:{column}")
}

/// A stable, machine-independent rendering of a run.
fn render(script: &Path, result: Result<(Outcome, Vec<String>), StartError>) -> String {
    let dir = script.parent().unwrap().display().to_string();
    let (outcome, suspensions) = match result {
        Ok(result) => result,
        Err(StartError::Syntax(diagnostics)) => {
            return diagnostics
                .iter()
                .map(|d| format!("syntax error: {}\n", d.message))
                .collect();
        }
        Err(e) => return format!("could not start: {e}\n"),
    };
    let mut out: String = suspensions.iter().map(|s| format!("{s}\n")).collect();
    out.push_str(&match outcome {
        Outcome::Passed { compared_lines } => format!("passed ({compared_lines} lines compared)\n"),
        Outcome::CompletedUnchecked => "completed without a compare file\n".to_string(),
        Outcome::FailedComparison { mismatch, span } => {
            format!("failed{}: {mismatch}\n", location(script, span))
        }
        Outcome::FailedError(error) => {
            format!("error{}: {}\n", location(script, error.span), error.message)
        }
        Outcome::Suspended { .. } => unreachable!("run() resumes past suspensions"),
        Outcome::Cancelled => "cancelled\n".to_string(),
        Outcome::LimitExceeded => "command limit exceeded\n".to_string(),
    });
    out.replace(&dir, "<dir>")
}

#[test]
fn project_and_feature_scripts_pass() {
    let mut failures = Vec::new();
    let mut ran = 0;
    for group in ["projects", "features"] {
        let mut dirs: Vec<PathBuf> = scripts(&samples().join(group))
            .iter()
            .map(|s| s.parent().unwrap().to_path_buf())
            .collect();
        dirs.dedup();
        for (i, dir) in dirs.iter().enumerate() {
            let scratch = scratch_copy(dir, &format!("{group}-{i}"));
            for script in scripts(&scratch) {
                ran += 1;
                let result = run(&script, 10_000_000);
                if !matches!(result, Ok((Outcome::Passed { .. }, _))) {
                    let name = dir.join(script.file_name().unwrap());
                    failures.push(format!("{}: {}", name.display(), render(&script, result)));
                }
            }
            let _ = fs::remove_dir_all(&scratch);
        }
    }
    assert!(ran > 30, "only {ran} scripts were found");
    assert!(
        failures.is_empty(),
        "{} script(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn runtime_samples_match_expected_outcomes() {
    let bless = std::env::var_os("N2T_BLESS").is_some();
    let dir = samples().join("runtime");
    let scratch = scratch_copy(&dir, "runtime");
    let mut failures = Vec::new();
    let mut ran = 0;
    for script in scripts(&scratch) {
        ran += 1;
        let actual = render(&script, run(&script, 100_000));
        let golden = dir.join(format!(
            "{}.expected",
            script.file_name().unwrap().to_str().unwrap()
        ));
        if bless {
            fs::write(&golden, &actual).unwrap();
            continue;
        }
        let expected = fs::read_to_string(&golden).unwrap_or_else(|_| {
            panic!(
                "missing {}; run with N2T_BLESS=1 to create it",
                golden.display()
            )
        });
        if expected != actual {
            failures.push(format!(
                "{}\n--- expected\n{expected}--- actual\n{actual}",
                golden.display()
            ));
        }
    }
    let _ = fs::remove_dir_all(&scratch);
    assert!(ran > 10, "only {ran} runtime samples were found");
    assert!(
        failures.is_empty(),
        "{} runtime sample(s) differ from their .expected files (N2T_BLESS=1 rewrites them):\n\n{}",
        failures.len(),
        failures.join("\n")
    );
}
