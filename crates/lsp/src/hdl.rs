//! Language features for `.hdl` documents.

use std::collections::HashMap;
use std::sync::Arc;

use lsp_types as lsp;
use lsp_types::Url;
use n2t_core::hdl::ChipUnit;
use n2t_core::hdl::ast::ChipImpl;
use n2t_core::hdl::interface::{ChipInterface, ChipOrigin, PinDirection, PinInfo};
use n2t_core::hdl::lexer::{Keyword, Token, TokenContent};
use n2t_core::hdl::resolve::is_valid_chip_name;
use n2t_core::hdl::semantic::{SignalKind, SymbolTarget};
use n2t_core::source::Span;

use crate::convert::clean_doc;
use crate::workspace::Workspace;

// -------------------------------------------------------------------------
// Shared helpers
// -------------------------------------------------------------------------

/// The symbol under the cursor, including the chip's own name.
fn target_at(unit: &ChipUnit, offset: usize) -> Option<(SymbolTarget, Span)> {
    if let Some(r) = unit.analysis.as_ref().and_then(|a| a.reference_at(offset)) {
        return Some((r.target.clone(), r.span));
    }
    let decl = unit.decl()?;
    decl.name
        .span
        .contains(offset)
        .then(|| (SymbolTarget::Chip(decl.name.name.clone()), decl.name.span))
}

pub fn pin_text(direction: PinDirection, pin: &PinInfo) -> String {
    let keyword = match direction {
        PinDirection::Input => "IN",
        PinDirection::Output => "OUT",
    };
    format!("{keyword} {pin}")
}

/// Markdown describing a chip interface.
pub fn interface_markdown(interface: &ChipInterface, unit: Option<&ChipUnit>) -> String {
    let pins = |pins: &[PinInfo]| {
        pins.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut body = format!("```hdl\nCHIP {} {{\n", interface.name);
    if !interface.inputs.is_empty() {
        body.push_str(&format!("    IN {};\n", pins(&interface.inputs)));
    }
    if !interface.outputs.is_empty() {
        body.push_str(&format!("    OUT {};\n", pins(&interface.outputs)));
    }
    body.push_str("}\n```");
    if let Some(doc) = &interface.doc {
        let doc = clean_doc(doc);
        if !doc.is_empty() {
            body.push_str("\n\n");
            body.push_str(&doc);
        }
    }
    let mut notes = Vec::new();
    if interface.origin == ChipOrigin::Builtin {
        notes.push("built-in chip");
    }
    if let Some(definition) = unit.and_then(|u| u.definition.as_ref()) {
        if definition.sequential {
            notes.push("sequential (clocked)");
        }
    } else if unit.is_some_and(|u| u.has_errors()) {
        notes.push("has errors");
    }
    if !notes.is_empty() {
        body.push_str(&format!("\n\n*{}*", notes.join(" · ")));
    }
    body
}

fn markdown(value: String, range: Option<lsp::Range>) -> lsp::Hover {
    lsp::Hover {
        contents: lsp::HoverContents::Markup(lsp::MarkupContent {
            kind: lsp::MarkupKind::Markdown,
            value,
        }),
        range,
    }
}

fn doc_of(info: &PinInfo) -> String {
    info.doc
        .as_deref()
        .map(clean_doc)
        .filter(|d| !d.is_empty())
        .map(|d| format!("\n\n{d}"))
        .unwrap_or_default()
}

// -------------------------------------------------------------------------
// Hover
// -------------------------------------------------------------------------

pub fn hover(ws: &mut Workspace, uri: &Url, position: lsp::Position) -> Option<lsp::Hover> {
    let unit = ws.hdl_unit(uri)?;
    let doc = ws.docs.get(uri)?;
    let dir = doc.dir.clone();
    let text = doc.text();
    let offset = text.offset(position);
    let (target, span) = target_at(&unit, offset)?;
    let range = Some(text.range(span));

    let value = match target {
        SymbolTarget::Chip(name) => {
            let child = ws.project(&dir).elaborator.compile(&name);
            match &child.interface {
                Some(interface) => interface_markdown(interface, Some(&child)),
                None => format!("unknown chip `{name}`"),
            }
        }
        SymbolTarget::FormalPin {
            chip,
            direction,
            index,
        } => {
            let child = ws.project(&dir).elaborator.compile(&chip);
            let interface = child.interface.as_ref()?;
            let pin = interface.pin_by_index(direction, index)?;
            format!(
                "```hdl\n{}\n```\n{} pin of `{}`{}",
                pin_text(direction, pin),
                direction,
                chip,
                doc_of(pin)
            )
        }
        SymbolTarget::Signal(id) => {
            let analysis = unit.analysis.as_ref()?;
            let signal = analysis.signals.get(id)?;
            match signal.kind {
                SignalKind::Input(i) => {
                    let pin = &analysis.interface.inputs[i];
                    format!(
                        "```hdl\n{}\n```\ninput of this chip{}",
                        pin_text(PinDirection::Input, pin),
                        doc_of(pin)
                    )
                }
                SignalKind::Output(i) => {
                    let pin = &analysis.interface.outputs[i];
                    format!(
                        "```hdl\n{}\n```\noutput of this chip{}",
                        pin_text(PinDirection::Output, pin),
                        doc_of(pin)
                    )
                }
                SignalKind::Internal => {
                    let width = signal.width.map_or("unknown width".to_string(), |w| {
                        format!("{w} bit{}", if w == 1 { "" } else { "s" })
                    });
                    format!(
                        "internal signal `{}` ({width})\n\n{} driver{}, {} reader{}",
                        signal.name,
                        signal.drivers.len(),
                        if signal.drivers.len() == 1 { "" } else { "s" },
                        signal.readers.len(),
                        if signal.readers.len() == 1 { "" } else { "s" },
                    )
                }
            }
        }
        SymbolTarget::BuiltinImplementation(symbol) => match ws.builtins.by_symbol(&symbol) {
            Some(chip) => format!(
                "built-in implementation `{symbol}`\n\n{}",
                interface_markdown(&chip.interface, None)
            ),
            None => format!("unknown built-in implementation `{symbol}`"),
        },
    };
    Some(markdown(value, range))
}

// -------------------------------------------------------------------------
// Definition
// -------------------------------------------------------------------------

pub fn definition(ws: &mut Workspace, uri: &Url, position: lsp::Position) -> Option<lsp::Location> {
    let unit = ws.hdl_unit(uri)?;
    let doc = ws.docs.get(uri)?;
    let dir = doc.dir.clone();
    let offset = doc.text().offset(position);
    let (target, _) = target_at(&unit, offset)?;
    match target {
        SymbolTarget::Chip(name) => {
            let child = ws.project(&dir).elaborator.compile(&name);
            let span = child.interface.as_ref()?.name_span?;
            ws.unit_location(&child, span)
        }
        SymbolTarget::FormalPin {
            chip,
            direction,
            index,
        } => {
            let child = ws.project(&dir).elaborator.compile(&chip);
            let span = child
                .interface
                .as_ref()?
                .pin_by_index(direction, index)?
                .span?;
            ws.unit_location(&child, span)
        }
        SymbolTarget::Signal(id) => {
            let signal = unit.analysis.as_ref()?.signals.get(id)?;
            let doc = ws.docs.get(uri)?;
            Some(lsp::Location::new(
                uri.clone(),
                doc.text().range(signal.declaration),
            ))
        }
        SymbolTarget::BuiltinImplementation(symbol) => {
            let chip = ws.builtins.by_symbol(&symbol)?.clone();
            let span = chip.interface.name_span?;
            ws.builtin_location(&chip, span)
        }
    }
}

// -------------------------------------------------------------------------
// References, highlights and rename
// -------------------------------------------------------------------------

/// The cross-file target equivalent to a symbol (e.g. a top-level pin is
/// referred to as a formal pin by other chips).
fn global_target(unit: &ChipUnit, target: &SymbolTarget) -> Option<SymbolTarget> {
    match target {
        SymbolTarget::Chip(_) | SymbolTarget::FormalPin { .. } => Some(target.clone()),
        SymbolTarget::Signal(id) => {
            let analysis = unit.analysis.as_ref()?;
            let signal = analysis.signals.get(*id)?;
            let (direction, index) = match signal.kind {
                SignalKind::Input(i) => (PinDirection::Input, i),
                SignalKind::Output(i) => (PinDirection::Output, i),
                SignalKind::Internal => return None,
            };
            Some(SymbolTarget::FormalPin {
                chip: analysis.interface.name.clone(),
                direction,
                index,
            })
        }
        SymbolTarget::BuiltinImplementation(_) => None,
    }
}

/// Every reference to `target` in the chips of the document's folder.
fn project_references(
    ws: &mut Workspace,
    uri: &Url,
    target: &SymbolTarget,
) -> Vec<(Arc<ChipUnit>, Span)> {
    let Some(dir) = ws.docs.get(uri).map(|d| d.dir.clone()) else {
        return Vec::new();
    };
    let mut names = ws.project(&dir).local_chips();
    for doc_uri in ws.docs_in(&dir) {
        let doc = &ws.docs[&doc_uri];
        if doc.is_overlay_chip() && !names.contains(&doc.stem) {
            names.push(doc.stem.clone());
        }
    }
    let mut found = Vec::new();
    for name in names {
        let unit = ws.project(&dir).elaborator.compile(&name);
        if let Some(analysis) = &unit.analysis {
            for r in analysis.references_to(target) {
                found.push((unit.clone(), r.span));
            }
        }
    }
    found
}

pub fn references(
    ws: &mut Workspace,
    uri: &Url,
    position: lsp::Position,
    include_declaration: bool,
) -> Vec<lsp::Location> {
    let Some(unit) = ws.hdl_unit(uri) else {
        return Vec::new();
    };
    let Some(doc) = ws.docs.get(uri) else {
        return Vec::new();
    };
    let offset = doc.text().offset(position);
    let Some((target, _)) = target_at(&unit, offset) else {
        return Vec::new();
    };

    let mut locations = Vec::new();
    if let SymbolTarget::Signal(_) = target
        && let Some(analysis) = &unit.analysis
    {
        let text = doc.text();
        for r in analysis.references_to(&target) {
            if include_declaration || !r.is_declaration {
                locations.push(lsp::Location::new(uri.clone(), text.range(r.span)));
            }
        }
    }
    if let Some(global) = global_target(&unit, &target) {
        for (other, span) in project_references(ws, uri, &global) {
            if let Some(location) = ws.unit_location(&other, span) {
                locations.push(location);
            }
        }
        if include_declaration {
            let dir = ws.docs[uri].dir.clone();
            let declaration = match &global {
                SymbolTarget::Chip(name) => {
                    let child = ws.project(&dir).elaborator.compile(name);
                    child
                        .interface
                        .as_ref()
                        .and_then(|i| i.name_span)
                        .and_then(|s| ws.unit_location(&child, s))
                }
                SymbolTarget::FormalPin {
                    chip,
                    direction,
                    index,
                } if !matches!(target, SymbolTarget::Signal(_)) => {
                    let child = ws.project(&dir).elaborator.compile(chip);
                    child
                        .interface
                        .as_ref()
                        .and_then(|i| i.pin_by_index(*direction, *index))
                        .and_then(|p| p.span)
                        .and_then(|s| ws.unit_location(&child, s))
                }
                _ => None,
            };
            locations.extend(declaration);
        }
    }
    locations.dedup();
    locations
}

pub fn highlights(
    ws: &mut Workspace,
    uri: &Url,
    position: lsp::Position,
) -> Vec<lsp::DocumentHighlight> {
    let Some(unit) = ws.hdl_unit(uri) else {
        return Vec::new();
    };
    let Some(doc) = ws.docs.get(uri) else {
        return Vec::new();
    };
    let text = doc.text();
    let offset = text.offset(position);
    let Some((target, _)) = target_at(&unit, offset) else {
        return Vec::new();
    };
    let Some(analysis) = &unit.analysis else {
        return Vec::new();
    };
    analysis
        .references_to(&target)
        .map(|r| {
            let kind = match &target {
                SymbolTarget::Signal(id)
                    if analysis.signals[*id].drivers.contains(&r.span) || r.is_declaration =>
                {
                    lsp::DocumentHighlightKind::WRITE
                }
                _ => lsp::DocumentHighlightKind::READ,
            };
            lsp::DocumentHighlight {
                range: text.range(r.span),
                kind: Some(kind),
            }
        })
        .collect()
}

pub fn prepare_rename(
    ws: &mut Workspace,
    uri: &Url,
    position: lsp::Position,
) -> Option<lsp::PrepareRenameResponse> {
    let unit = ws.hdl_unit(uri)?;
    let doc = ws.docs.get(uri)?;
    let text = doc.text();
    let offset = text.offset(position);
    match target_at(&unit, offset)? {
        (SymbolTarget::Signal(_), span) => {
            Some(lsp::PrepareRenameResponse::Range(text.range(span)))
        }
        _ => None,
    }
}

pub fn rename(
    ws: &mut Workspace,
    uri: &Url,
    position: lsp::Position,
    new_name: &str,
) -> Result<Option<lsp::WorkspaceEdit>, String> {
    let Some(unit) = ws.hdl_unit(uri) else {
        return Ok(None);
    };
    let Some(doc) = ws.docs.get(uri) else {
        return Ok(None);
    };
    let offset = doc.text().offset(position);
    let Some((target, _)) = target_at(&unit, offset) else {
        return Ok(None);
    };
    let SymbolTarget::Signal(id) = target else {
        return Err("only pins and internal signals can be renamed".into());
    };
    if !is_valid_chip_name(new_name) || Keyword::lookup(new_name).is_some() {
        return Err(format!("`{new_name}` is not a valid name"));
    }
    let analysis = unit.analysis.as_ref().ok_or("chip could not be analysed")?;
    if analysis
        .signals
        .iter()
        .enumerate()
        .any(|(i, s)| i != id && s.name == new_name)
    {
        return Err(format!("a signal named `{new_name}` already exists"));
    }

    let mut changes: HashMap<Url, Vec<lsp::TextEdit>> = HashMap::new();
    let text = doc.text();
    for r in analysis.references_to(&target) {
        changes
            .entry(uri.clone())
            .or_default()
            .push(lsp::TextEdit::new(text.range(r.span), new_name.to_string()));
    }
    if let Some(global) = global_target(&unit, &target) {
        for (other, span) in project_references(ws, uri, &global) {
            if let Some(location) = ws.unit_location(&other, span) {
                changes
                    .entry(location.uri)
                    .or_default()
                    .push(lsp::TextEdit::new(location.range, new_name.to_string()));
            }
        }
    }
    for edits in changes.values_mut() {
        edits.sort_by_key(|e| (e.range.start.line, e.range.start.character));
        edits.dedup();
    }
    Ok(Some(lsp::WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    }))
}

// -------------------------------------------------------------------------
// Completion
// -------------------------------------------------------------------------

fn keyword_item(label: &str, insert: &str, snippets: bool) -> lsp::CompletionItem {
    lsp::CompletionItem {
        label: label.to_string(),
        kind: Some(lsp::CompletionItemKind::KEYWORD),
        insert_text: Some(if snippets {
            insert.to_string()
        } else {
            label.to_string()
        }),
        insert_text_format: Some(if snippets {
            lsp::InsertTextFormat::SNIPPET
        } else {
            lsp::InsertTextFormat::PLAIN_TEXT
        }),
        ..Default::default()
    }
}

fn chip_item(name: &str, unit: &ChipUnit, snippets: bool) -> lsp::CompletionItem {
    let interface = unit.interface.as_deref();
    let (insert_text, format) = match interface {
        Some(i) if snippets => {
            let pins: Vec<String> = i
                .inputs
                .iter()
                .chain(&i.outputs)
                .enumerate()
                .map(|(n, p)| format!("{}=${{{}}}", p.name, n + 1))
                .collect();
            (
                format!("{name}({});", pins.join(", ")),
                lsp::InsertTextFormat::SNIPPET,
            )
        }
        _ => (name.to_string(), lsp::InsertTextFormat::PLAIN_TEXT),
    };
    lsp::CompletionItem {
        label: name.to_string(),
        kind: Some(lsp::CompletionItemKind::CLASS),
        detail: interface.map(|i| i.to_string()),
        documentation: interface.map(|i| {
            lsp::Documentation::MarkupContent(lsp::MarkupContent {
                kind: lsp::MarkupKind::Markdown,
                value: interface_markdown(i, Some(unit)),
            })
        }),
        insert_text: Some(insert_text),
        insert_text_format: Some(format),
        ..Default::default()
    }
}

pub fn completion(
    ws: &mut Workspace,
    uri: &Url,
    position: lsp::Position,
    snippets: bool,
) -> Vec<lsp::CompletionItem> {
    let Some(unit) = ws.hdl_unit(uri) else {
        return Vec::new();
    };
    let Some(doc) = ws.docs.get(uri) else {
        return Vec::new();
    };
    let dir = doc.dir.clone();
    let text = doc.text.clone();
    let offset = doc.text().offset(position);
    let Some(parsed) = unit.parsed.clone() else {
        return Vec::new();
    };

    // Inside a comment: nothing to complete.
    for t in parsed.tokens.iter().filter(|t| t.is_trivia()) {
        let is_line = text[t.span.start..].starts_with("//");
        if t.span.start < offset && (offset < t.span.end || (is_line && offset == t.span.end)) {
            return Vec::new();
        }
    }

    let mut before: Vec<&Token> = parsed
        .tokens
        .iter()
        .filter(|t| !t.is_trivia() && t.content != TokenContent::Eof && t.span.end <= offset)
        .collect();
    if let Some(last) = before.last()
        && last.span.end == offset
        && matches!(
            last.content,
            TokenContent::Ident(_) | TokenContent::Keyword(_)
        )
    {
        before.pop();
    }
    let prev = before.last().map(|t| &t.content);

    if before.is_empty() {
        return vec![keyword_item(
            "CHIP",
            "CHIP ${1:Name} {\n    IN ${2:in};\n    OUT ${3:out};\n\n    PARTS:\n    $0\n}",
            snippets,
        )];
    }

    // Inside a part's parentheses?
    let mut depth = 0;
    let mut part_chip: Option<String> = None;
    let mut inside_part = false;
    for i in (0..before.len()).rev() {
        match &before[i].content {
            TokenContent::RightParen => depth += 1,
            TokenContent::LeftParen => {
                if depth == 0 {
                    inside_part = true;
                    if i > 0
                        && let TokenContent::Ident(name) = &before[i - 1].content
                    {
                        part_chip = Some(name.clone());
                    }
                    break;
                }
                depth -= 1;
            }
            TokenContent::Semicolon | TokenContent::LeftBrace | TokenContent::RightBrace => break,
            _ => {}
        }
    }

    if inside_part {
        let Some(chip) = part_chip else {
            return Vec::new();
        };
        let child = ws.project(&dir).elaborator.compile(&chip);
        let Some(interface) = child.interface.clone() else {
            return Vec::new();
        };
        return match prev {
            Some(TokenContent::LeftParen | TokenContent::Comma) => interface
                .inputs
                .iter()
                .map(|p| (PinDirection::Input, p))
                .chain(interface.outputs.iter().map(|p| (PinDirection::Output, p)))
                .enumerate()
                .map(|(i, (direction, pin))| lsp::CompletionItem {
                    label: pin.name.clone(),
                    kind: Some(lsp::CompletionItemKind::FIELD),
                    detail: Some(pin_text(direction, pin)),
                    insert_text: Some(format!("{}=", pin.name)),
                    sort_text: Some(format!("{i:03}")),
                    documentation: pin
                        .doc
                        .as_deref()
                        .map(|d| lsp::Documentation::String(clean_doc(d))),
                    ..Default::default()
                })
                .collect(),
            Some(TokenContent::Equals) => {
                // Which formal pin is being connected?
                let mut j = before.len() - 1;
                if j > 0 && before[j - 1].content == TokenContent::RightBracket {
                    while j > 0 && before[j - 1].content != TokenContent::LeftBracket {
                        j -= 1;
                    }
                    j = j.saturating_sub(1);
                }
                let formal = match j.checked_sub(1).map(|k| &before[k].content) {
                    Some(TokenContent::Ident(name)) => interface.pin(name).map(|p| p.direction),
                    _ => None,
                };
                signal_items(&unit, formal)
            }
            _ => Vec::new(),
        };
    }

    let has_parts = before
        .iter()
        .any(|t| t.content == TokenContent::Keyword(Keyword::Parts));
    let has_builtin = before
        .iter()
        .any(|t| t.content == TokenContent::Keyword(Keyword::Builtin));
    let clocked_index = before
        .iter()
        .rposition(|t| t.content == TokenContent::Keyword(Keyword::Clocked));
    match prev {
        Some(TokenContent::Keyword(Keyword::Builtin)) => ws
            .builtins
            .iter()
            .map(|c| lsp::CompletionItem {
                label: c.symbol.clone(),
                kind: Some(lsp::CompletionItemKind::MODULE),
                detail: Some(c.interface.to_string()),
                ..Default::default()
            })
            .collect(),
        Some(TokenContent::Keyword(Keyword::Clocked) | TokenContent::Comma)
            if clocked_index.is_some() =>
        {
            let Some(analysis) = &unit.analysis else {
                return Vec::new();
            };
            analysis
                .interface
                .inputs
                .iter()
                .map(|p| (PinDirection::Input, p))
                .chain(
                    analysis
                        .interface
                        .outputs
                        .iter()
                        .map(|p| (PinDirection::Output, p)),
                )
                .map(|(d, p)| lsp::CompletionItem {
                    label: p.name.clone(),
                    kind: Some(lsp::CompletionItemKind::FIELD),
                    detail: Some(pin_text(d, p)),
                    ..Default::default()
                })
                .collect()
        }
        Some(TokenContent::Colon | TokenContent::Semicolon | TokenContent::RightParen)
            if has_parts =>
        {
            let names = ws.project(&dir).all_chips();
            names
                .iter()
                .filter(|n| **n != unit.name)
                .map(|n| {
                    let child = ws.project(&dir).elaborator.compile(n);
                    chip_item(n, &child, snippets)
                })
                .collect()
        }
        Some(TokenContent::LeftBrace | TokenContent::Semicolon) if !has_parts && !has_builtin => {
            let mut items = Vec::new();
            let has = |k: Keyword| before.iter().any(|t| t.content == TokenContent::Keyword(k));
            if !has(Keyword::In) && !has(Keyword::Out) {
                items.push(keyword_item("IN", "IN ${1:in};", snippets));
            }
            if !has(Keyword::Out) {
                items.push(keyword_item("OUT", "OUT ${1:out};", snippets));
            }
            items.push(keyword_item("PARTS:", "PARTS:\n    $0", snippets));
            items.push(keyword_item("BUILTIN", "BUILTIN ${1:Name};", snippets));
            items
        }
        Some(TokenContent::Semicolon) if has_builtin && clocked_index.is_none() => {
            vec![keyword_item("CLOCKED", "CLOCKED ${1:in};", snippets)]
        }
        _ => Vec::new(),
    }
}

/// Signals usable on the right of `=` for a formal pin of `direction`.
fn signal_items(unit: &ChipUnit, direction: Option<PinDirection>) -> Vec<lsp::CompletionItem> {
    let Some(analysis) = &unit.analysis else {
        return Vec::new();
    };
    let mut items: Vec<lsp::CompletionItem> = analysis
        .signals
        .iter()
        .filter(|s| {
            !matches!(
                (direction, s.kind),
                (Some(PinDirection::Input), SignalKind::Output(_))
                    | (Some(PinDirection::Output), SignalKind::Input(_))
            )
        })
        .map(|s| {
            let (kind, detail) = match s.kind {
                SignalKind::Input(i) => (
                    lsp::CompletionItemKind::FIELD,
                    pin_text(PinDirection::Input, &analysis.interface.inputs[i]),
                ),
                SignalKind::Output(i) => (
                    lsp::CompletionItemKind::FIELD,
                    pin_text(PinDirection::Output, &analysis.interface.outputs[i]),
                ),
                SignalKind::Internal => (
                    lsp::CompletionItemKind::VARIABLE,
                    match s.width {
                        Some(w) => {
                            format!("internal signal ({w} bit{})", if w == 1 { "" } else { "s" })
                        }
                        None => "internal signal".to_string(),
                    },
                ),
            };
            lsp::CompletionItem {
                label: s.name.clone(),
                kind: Some(kind),
                detail: Some(detail),
                ..Default::default()
            }
        })
        .collect();
    if direction != Some(PinDirection::Output) {
        for constant in ["true", "false"] {
            items.push(lsp::CompletionItem {
                label: constant.to_string(),
                kind: Some(lsp::CompletionItemKind::CONSTANT),
                ..Default::default()
            });
        }
    }
    items
}

// -------------------------------------------------------------------------
// Document symbols and inlay hints
// -------------------------------------------------------------------------

#[allow(deprecated)] // `DocumentSymbol::deprecated` must still be initialised.
pub fn document_symbols(ws: &mut Workspace, uri: &Url) -> Vec<lsp::DocumentSymbol> {
    let Some(unit) = ws.hdl_unit(uri) else {
        return Vec::new();
    };
    let Some(doc) = ws.docs.get(uri) else {
        return Vec::new();
    };
    let Some(decl) = unit.decl() else {
        return Vec::new();
    };
    let text = doc.text();
    let symbol =
        |name: String, detail: Option<String>, kind, span: Span, selection: Span, children| {
            lsp::DocumentSymbol {
                name,
                detail,
                kind,
                tags: None,
                deprecated: None,
                range: text.range(span),
                selection_range: text.range(selection),
                children,
            }
        };

    let mut children = Vec::new();
    for (direction, pins) in [("IN", &decl.inputs), ("OUT", &decl.outputs)] {
        for pin in pins {
            let detail = if pin.width() == 1 {
                direction.to_string()
            } else {
                format!("{direction} [{}]", pin.width())
            };
            children.push(symbol(
                pin.name.name.clone(),
                Some(detail),
                lsp::SymbolKind::FIELD,
                pin.span,
                pin.name.span,
                None,
            ));
        }
    }
    if let Some(ChipImpl::Composite { parts, .. }) = &decl.implementation {
        for (i, part) in parts.iter().enumerate() {
            children.push(symbol(
                part.chip_name.name.clone(),
                Some(format!("part #{i}")),
                lsp::SymbolKind::OBJECT,
                part.span,
                part.chip_name.span,
                None,
            ));
        }
    }
    vec![symbol(
        decl.name.name.clone(),
        Some("CHIP".to_string()),
        lsp::SymbolKind::CLASS,
        decl.span,
        decl.name.span,
        Some(children),
    )]
}

pub fn inlay_hints(ws: &mut Workspace, uri: &Url, range: lsp::Range) -> Vec<lsp::InlayHint> {
    let Some(unit) = ws.hdl_unit(uri) else {
        return Vec::new();
    };
    let Some(doc) = ws.docs.get(uri) else {
        return Vec::new();
    };
    let Some(analysis) = &unit.analysis else {
        return Vec::new();
    };
    let text = doc.text();
    let (start, end) = (text.offset(range.start), text.offset(range.end));
    analysis
        .signals
        .iter()
        .filter(|s| s.kind == SignalKind::Internal)
        .filter_map(|s| {
            let width = s.width?;
            let span = s.drivers.first().copied().unwrap_or(s.declaration);
            (width > 1 && span.start >= start && span.end <= end).then(|| lsp::InlayHint {
                position: text.position(span.end),
                label: lsp::InlayHintLabel::String(format!(": {width}")),
                kind: Some(lsp::InlayHintKind::TYPE),
                text_edits: None,
                tooltip: Some(lsp::InlayHintTooltip::String(format!(
                    "inferred width: {width} bits"
                ))),
                padding_left: Some(false),
                padding_right: Some(false),
                data: None,
            })
        })
        .collect()
}
