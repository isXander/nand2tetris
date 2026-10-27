//! Compiles and analyses everything in the repository's `samples/` folder.
//!
//! - `samples/projects` and `samples/features` must produce no diagnostics.
//! - Each file in `samples/errors` has a `<file>.expected` golden with its
//!   rendered diagnostics. Set `N2T_BLESS=1` to rewrite the goldens, then
//!   review the diff.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use n2t_core::LineIndex;
use n2t_core::builtins::BuiltinLibrary;
use n2t_core::diagnostic::Diagnostic;
use n2t_core::hdl::Elaborator;
use n2t_core::hdl::resolve::{FsSourceProvider, is_valid_chip_name};

fn samples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../samples")
        .canonicalize()
        .unwrap()
}

/// Files with one of `extensions` under `dir`, recursively and sorted.
fn files(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).unwrap().map(Result::unwrap) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| extensions.contains(&e))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn stem(path: &Path) -> String {
    path.file_stem().unwrap().to_str().unwrap().to_string()
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// `File.hdl:line:col: severity[code]: message`, plus indented notes.
fn render(file: &str, text: &str, diagnostics: &[Diagnostic]) -> String {
    let lines = LineIndex::new(text);
    let mut out = String::new();
    for d in diagnostics {
        let location = match d.span {
            Some(span) => {
                let (line, column) = lines.line_col_one_based(text, span.start);
                format!("{file}:{line}:{column}")
            }
            None => file.to_string(),
        };
        out.push_str(&format!(
            "{location}: {}[{}]: {}\n",
            d.severity, d.code, d.message
        ));
        for related in &d.related {
            let at = match &related.location {
                Some(location) => {
                    let (other_file, other_text) = match &location.source {
                        Some(id) => (
                            file_name(id).to_string(),
                            fs::read_to_string(id).unwrap_or_default(),
                        ),
                        None => (file.to_string(), text.to_string()),
                    };
                    let (line, column) = LineIndex::new(&other_text)
                        .line_col_one_based(&other_text, location.span.start);
                    format!(" ({other_file}:{line}:{column})")
                }
                None => String::new(),
            };
            // Search paths are machine-specific.
            let message = match related.message.strip_prefix("searched ") {
                Some(path) => format!("searched {}", file_name(path)),
                None => related.message.clone(),
            };
            out.push_str(&format!("    note: {message}{at}\n"));
        }
    }
    out
}

/// Diagnostics for every `.hdl` and `.tst` file in `dir` (not recursive),
/// using one elaborator for the folder.
fn check_folder(dir: &Path) -> BTreeMap<PathBuf, String> {
    let mut elaborator = Elaborator::new(
        FsSourceProvider::new(dir),
        Arc::new(BuiltinLibrary::standard()),
    );
    let mut results = BTreeMap::new();
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.is_file() && matches!(p.extension().and_then(|e| e.to_str()), Some("hdl" | "tst"))
        })
        .collect();
    paths.sort();
    for path in paths {
        let text = fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        let diagnostics: Vec<Diagnostic> = if path.extension().unwrap() == "hdl" {
            assert!(
                is_valid_chip_name(&stem(&path)),
                "{} is not a valid chip name",
                path.display()
            );
            elaborator.compile(&stem(&path)).diagnostics.clone()
        } else {
            let (parsed, analysis) = n2t_core::tst::check(&text, &mut elaborator);
            parsed
                .diagnostics
                .into_iter()
                .chain(analysis.diagnostics)
                .collect()
        };
        results.insert(path, render(&name, &text, &diagnostics));
    }
    results
}

fn folders(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = files(root, &["hdl", "tst"])
        .iter()
        .map(|f| f.parent().unwrap().to_path_buf())
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

#[test]
fn projects_and_features_have_no_diagnostics() {
    let mut problems = String::new();
    let mut checked = 0;
    for group in ["projects", "features"] {
        for dir in folders(&samples().join(group)) {
            for (path, rendered) in check_folder(&dir) {
                checked += 1;
                if !rendered.is_empty() {
                    problems.push_str(&format!("{}:\n{rendered}\n", path.display()));
                }
            }
        }
    }
    assert!(checked > 50, "only {checked} sample files were found");
    assert!(problems.is_empty(), "samples should be clean:\n{problems}");
}

#[test]
fn error_samples_match_expected_diagnostics() {
    let bless = std::env::var_os("N2T_BLESS").is_some();
    let mut failures = Vec::new();
    let mut checked = 0;
    for dir in folders(&samples().join("errors")) {
        for (path, actual) in check_folder(&dir) {
            checked += 1;
            let golden = PathBuf::from(format!("{}.expected", path.display()));
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
                    path.display()
                ));
            }
        }
    }
    assert!(checked > 40, "only {checked} error samples were found");
    assert!(
        failures.is_empty(),
        "{} error sample(s) differ from their .expected files (N2T_BLESS=1 rewrites them):\n\n{}",
        failures.len(),
        failures.join("\n")
    );
}
