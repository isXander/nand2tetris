//! Terminal rendering of diagnostics.

use std::io::IsTerminal;

use n2t_core::diagnostic::{Diagnostic, Severity};
use n2t_core::{LineIndex, Span};

pub struct Reporter {
    color: bool,
    pub errors: usize,
    pub warnings: usize,
}

impl Reporter {
    pub fn new() -> Self {
        Self {
            color: std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
            errors: 0,
            warnings: 0,
        }
    }

    pub fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    pub fn bold(&self, text: &str) -> String {
        self.paint("1", text)
    }

    pub fn green(&self, text: &str) -> String {
        self.paint("1;32", text)
    }

    pub fn red(&self, text: &str) -> String {
        self.paint("1;31", text)
    }

    pub fn yellow(&self, text: &str) -> String {
        self.paint("1;33", text)
    }

    fn severity(&self, severity: Severity) -> String {
        match severity {
            Severity::Error => self.red("error"),
            Severity::Warning => self.yellow("warning"),
            Severity::Information => self.paint("1;36", "info"),
            Severity::Hint => self.paint("1;36", "hint"),
        }
    }

    /// Prints a diagnostic for `source` (displayed as `path`).
    pub fn diagnostic(&mut self, path: &str, source: &str, d: &Diagnostic) {
        match d.severity {
            Severity::Error => self.errors += 1,
            Severity::Warning => self.warnings += 1,
            _ => {}
        }
        eprintln!(
            "{}{}: {}",
            self.severity(d.severity),
            self.bold(&format!("[{}]", d.code)),
            self.bold(&d.message)
        );
        match d.span {
            Some(span) => self.snippet(path, source, span, d.severity),
            None => eprintln!("  {} {path}", self.paint("1;34", "-->")),
        }
        for related in &d.related {
            eprintln!("   {} note: {}", self.paint("1;34", "="), related.message);
        }
        eprintln!();
    }

    /// Prints a message with a source excerpt.
    pub fn message(
        &mut self,
        severity: Severity,
        title: &str,
        path: &str,
        source: &str,
        span: Option<Span>,
    ) {
        match severity {
            Severity::Error => self.errors += 1,
            Severity::Warning => self.warnings += 1,
            _ => {}
        }
        eprintln!("{}: {}", self.severity(severity), self.bold(title));
        if let Some(span) = span {
            self.snippet(path, source, span, severity);
        }
        eprintln!();
    }

    fn snippet(&self, path: &str, source: &str, span: Span, severity: Severity) {
        let lines = LineIndex::new(source);
        let (line, column) = lines.line_col_one_based(source, span.start);
        let gutter = line.to_string().len();
        let blue = |t: &str| self.paint("1;34", t);
        eprintln!(
            "{}{} {path}:{line}:{column}",
            " ".repeat(gutter),
            blue("-->")
        );
        let Some(start) = lines.line_start(line as u32 - 1) else {
            return;
        };
        let text = source[start..].lines().next().unwrap_or("");
        eprintln!("{} {}", " ".repeat(gutter), blue("|"));
        eprintln!(
            "{} {} {}",
            blue(&line.to_string()),
            blue("|"),
            text.replace('\t', "    ")
        );
        let prefix: String = text
            .chars()
            .take(column - 1)
            .map(|c| {
                if c == '\t' {
                    "    ".to_string()
                } else {
                    " ".to_string()
                }
            })
            .collect();
        let width = source[span.start..span.end.min(start + text.len()).max(span.start)]
            .chars()
            .count()
            .max(1);
        let marker = "^".repeat(width);
        let marker = match severity {
            Severity::Error => self.red(&marker),
            Severity::Warning => self.yellow(&marker),
            _ => marker,
        };
        eprintln!("{} {} {prefix}{marker}", " ".repeat(gutter), blue("|"));
    }
}
