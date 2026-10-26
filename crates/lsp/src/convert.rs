//! Conversions between core types and LSP types.

use std::path::{Path, PathBuf};

use lsp_types as lsp;
use lsp_types::Url;
use n2t_core::diagnostic::{Diagnostic, Severity};
use n2t_core::{LineIndex, Position, Span};

/// A text plus its line index, for position conversions.
pub struct Text<'a> {
    pub text: &'a str,
    pub lines: &'a LineIndex,
}

impl<'a> Text<'a> {
    pub fn new(text: &'a str, lines: &'a LineIndex) -> Self {
        Self { text, lines }
    }

    pub fn position(&self, offset: usize) -> lsp::Position {
        let p = self.lines.position_utf16(self.text, offset);
        lsp::Position::new(p.line, p.column)
    }

    pub fn range(&self, span: Span) -> lsp::Range {
        lsp::Range::new(self.position(span.start), self.position(span.end))
    }

    pub fn offset(&self, position: lsp::Position) -> usize {
        self.lines
            .offset_utf16(
                self.text,
                Position {
                    line: position.line,
                    column: position.character,
                },
            )
            .unwrap_or(self.text.len())
    }
}

/// Converts a range in some other source text (read from disk if needed).
pub fn range_in(text: &str, span: Span) -> lsp::Range {
    let lines = LineIndex::new(text);
    Text::new(text, &lines).range(span)
}

pub fn path_to_url(path: &Path) -> Option<Url> {
    Url::from_file_path(path).ok()
}

pub fn url_to_path(url: &Url) -> Option<PathBuf> {
    if url.scheme() != "file" {
        return None;
    }
    url.to_file_path().ok()
}

/// Canonicalises a path whose file may not exist yet (canonicalising the parent).
pub fn canonical_path(path: &Path) -> PathBuf {
    if let Ok(p) = path.canonicalize() {
        return p;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf())
            .join(name),
        _ => path.to_path_buf(),
    }
}

fn severity(severity: Severity) -> lsp::DiagnosticSeverity {
    match severity {
        Severity::Error => lsp::DiagnosticSeverity::ERROR,
        Severity::Warning => lsp::DiagnosticSeverity::WARNING,
        Severity::Information => lsp::DiagnosticSeverity::INFORMATION,
        Severity::Hint => lsp::DiagnosticSeverity::HINT,
    }
}

/// Converts a core diagnostic for a document. Related locations in other
/// sources are resolved by `resolve_source`, which maps a source identity to
/// its URL and text.
pub fn diagnostic(
    doc_uri: &Url,
    text: &Text<'_>,
    d: &Diagnostic,
    resolve_source: &mut dyn FnMut(&str) -> Option<(Url, String)>,
) -> lsp::Diagnostic {
    let range = d
        .span
        .map(|s| text.range(s))
        .unwrap_or_else(|| lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 0)));
    let mut message = d.message.clone();
    let mut related = Vec::new();
    for r in &d.related {
        let location = r.location.as_ref().and_then(|loc| match &loc.source {
            None => Some(lsp::Location::new(doc_uri.clone(), text.range(loc.span))),
            Some(id) => resolve_source(id)
                .map(|(uri, src)| lsp::Location::new(uri, range_in(&src, loc.span))),
        });
        match location {
            Some(location) => related.push(lsp::DiagnosticRelatedInformation {
                location,
                message: r.message.clone(),
            }),
            None => {
                message.push_str("\nnote: ");
                message.push_str(&r.message);
            }
        }
    }
    lsp::Diagnostic {
        range,
        severity: Some(severity(d.severity)),
        code: Some(lsp::NumberOrString::String(d.code.as_str().to_string())),
        source: Some("n2t".to_string()),
        message,
        related_information: (!related.is_empty()).then_some(related),
        tags: matches!(
            d.code,
            n2t_core::DiagnosticCode::UnusedSignal | n2t_core::DiagnosticCode::UnusedInput
        )
        .then(|| vec![lsp::DiagnosticTag::UNNECESSARY]),
        ..Default::default()
    }
}

/// Strips comment decoration (`*` gutters) from a documentation comment body.
pub fn clean_doc(doc: &str) -> String {
    let lines: Vec<&str> = doc
        .lines()
        .map(|l| {
            let t = l.trim();
            let t = t.strip_prefix('*').unwrap_or(t);
            t.strip_prefix(' ').unwrap_or(t)
        })
        .collect();
    let start = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(start, |e| e + 1);
    lines[start..end].join("\n")
}
