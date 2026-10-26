//! Semantic tokens: classifies names using semantic analysis (chip types,
//! formal pins, top-level pins, internal signals). Keywords, comments and
//! literals are left to the editor's syntax grammar.

use lsp_types as lsp;
use lsp_types::Url;
use n2t_core::hdl::interface::{ChipOrigin, PinDirection};
use n2t_core::hdl::semantic::{SignalKind, SymbolTarget};
use n2t_core::source::Span;
use n2t_core::tst::semantic::{PinRole, ScriptTarget, VariableTarget};

use crate::convert::Text;
use crate::workspace::{DocKind, Workspace};

const TYPE: u32 = 0;
const PROPERTY: u32 = 1;
const PARAMETER: u32 = 2;
const VARIABLE: u32 = 3;
const FUNCTION: u32 = 4;

const DECLARATION: u32 = 1 << 0;
const READONLY: u32 = 1 << 1;
const DEFAULT_LIBRARY: u32 = 1 << 2;

pub fn legend() -> lsp::SemanticTokensLegend {
    lsp::SemanticTokensLegend {
        token_types: vec![
            lsp::SemanticTokenType::TYPE,
            lsp::SemanticTokenType::PROPERTY,
            lsp::SemanticTokenType::PARAMETER,
            lsp::SemanticTokenType::VARIABLE,
            lsp::SemanticTokenType::FUNCTION,
        ],
        token_modifiers: vec![
            lsp::SemanticTokenModifier::DECLARATION,
            lsp::SemanticTokenModifier::READONLY,
            lsp::SemanticTokenModifier::DEFAULT_LIBRARY,
        ],
    }
}

pub fn full(ws: &mut Workspace, uri: &Url) -> Option<lsp::SemanticTokens> {
    let kind = ws.docs.get(uri)?.kind;
    let tokens = match kind {
        DocKind::Hdl => hdl_tokens(ws, uri)?,
        DocKind::Tst => tst_tokens(ws, uri)?,
        DocKind::Other => return None,
    };
    let doc = ws.docs.get(uri)?;
    Some(lsp::SemanticTokens {
        result_id: None,
        data: encode(&doc.text(), tokens),
    })
}

fn hdl_tokens(ws: &mut Workspace, uri: &Url) -> Option<Vec<(Span, u32, u32)>> {
    let unit = ws.hdl_unit(uri)?;
    let dir = ws.docs.get(uri)?.dir.clone();
    let analysis = unit.analysis.as_ref()?;
    let mut tokens = Vec::new();
    if let Some(decl) = unit.decl() {
        tokens.push((decl.name.span, TYPE, DECLARATION));
    }
    for r in &analysis.references {
        let declaration = if r.is_declaration { DECLARATION } else { 0 };
        let (token_type, modifiers) = match &r.target {
            SymbolTarget::Chip(name) => {
                let child = ws.project(&dir).elaborator.compile(name);
                let builtin = child
                    .interface
                    .as_ref()
                    .is_some_and(|i| i.origin == ChipOrigin::Builtin);
                (TYPE, if builtin { DEFAULT_LIBRARY } else { 0 })
            }
            SymbolTarget::FormalPin { direction, .. } => (
                PROPERTY,
                if *direction == PinDirection::Output {
                    READONLY
                } else {
                    0
                },
            ),
            SymbolTarget::Signal(id) => match analysis.signals.get(*id).map(|s| s.kind) {
                Some(SignalKind::Input(_)) => (PARAMETER, declaration),
                Some(SignalKind::Output(_)) => (PARAMETER, declaration | READONLY),
                _ => (VARIABLE, declaration),
            },
            SymbolTarget::BuiltinImplementation(_) => (FUNCTION, DEFAULT_LIBRARY),
        };
        tokens.push((r.span, token_type, modifiers));
    }
    Some(tokens)
}

fn tst_tokens(ws: &mut Workspace, uri: &Url) -> Option<Vec<(Span, u32, u32)>> {
    let (_, analysis) = ws.tst_state(uri)?;
    Some(
        analysis
            .references
            .iter()
            .map(|r| {
                let (token_type, modifiers) = match &r.target {
                    ScriptTarget::Chip(_) => (TYPE, 0),
                    ScriptTarget::Variable(VariableTarget::Time) => {
                        (VARIABLE, READONLY | DEFAULT_LIBRARY)
                    }
                    ScriptTarget::Variable(VariableTarget::Pin { role, .. }) => match role {
                        PinRole::Input => (PARAMETER, 0),
                        PinRole::Output => (PARAMETER, READONLY),
                        PinRole::Internal => (VARIABLE, READONLY),
                    },
                    ScriptTarget::Variable(VariableTarget::BuiltinState { .. }) => {
                        (PROPERTY, DEFAULT_LIBRARY)
                    }
                };
                (r.span, token_type, modifiers)
            })
            .collect(),
    )
}

/// Sorts, de-duplicates and delta-encodes single-line tokens.
fn encode(text: &Text<'_>, mut tokens: Vec<(Span, u32, u32)>) -> Vec<lsp::SemanticToken> {
    tokens.sort_by_key(|(span, ..)| (span.start, span.end));
    let mut data = Vec::with_capacity(tokens.len());
    let (mut line, mut column, mut last_end) = (0u32, 0u32, 0usize);
    for (span, token_type, modifiers) in tokens {
        if span.is_empty() || span.start < last_end {
            continue;
        }
        let start = text.position(span.start);
        let end = text.position(span.end);
        if start.line != end.line {
            continue;
        }
        let delta_line = start.line - line;
        let delta_start = if delta_line == 0 {
            start.character - column
        } else {
            start.character
        };
        data.push(lsp::SemanticToken {
            delta_line,
            delta_start,
            length: end.character - start.character,
            token_type,
            token_modifiers_bitset: modifiers,
        });
        line = start.line;
        column = start.character;
        last_end = span.end;
    }
    data
}
