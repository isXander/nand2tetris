//! Recursive-descent parser for `.hdl` sources with error recovery.
//!
//! The parser produces as much of the tree as it can: malformed bindings and
//! parts are dropped with a diagnostic, and parsing resumes at the next
//! synchronisation point. A tree produced alongside error diagnostics is
//! intended for editor tooling only, never for execution.

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::hdl::ast::*;
use crate::hdl::lexer::*;

#[derive(Debug, Clone)]
pub struct ParseResult {
    pub chip: Option<ChipDecl>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Parses a token stream (as produced by [`crate::hdl::lexer::lex`]).
pub fn parse(tokens: &[Token]) -> ParseResult {
    Parser::new(tokens).parse()
}

struct SigToken<'t> {
    token: &'t Token,
    /// The closest documentation comment between the previous significant
    /// token and this one.
    doc: Option<&'t str>,
}

pub struct Parser<'t> {
    tokens: Vec<SigToken<'t>>,
    pos: usize,
    eof: Token,
    diagnostics: Vec<Diagnostic>,
    last_end: usize,
}

impl<'t> Parser<'t> {
    pub fn new(tokens: &'t [Token]) -> Self {
        let mut sig = Vec::with_capacity(tokens.len());
        let mut doc = None;
        for token in tokens {
            match &token.content {
                TokenContent::DocComment(text) => doc = Some(text.as_str()),
                TokenContent::Comment(_) => {}
                _ => sig.push(SigToken {
                    token,
                    doc: doc.take(),
                }),
            }
        }
        let end = tokens.last().map_or(0, |t| t.span.end);
        Self {
            tokens: sig,
            pos: 0,
            eof: Token {
                content: TokenContent::Eof,
                span: Span::empty(end),
            },
            diagnostics: Vec::new(),
            last_end: 0,
        }
    }

    pub fn parse(mut self) -> ParseResult {
        let chip = self.file();
        ParseResult {
            chip,
            diagnostics: self.diagnostics,
        }
    }

    // ---------------------------------------------------------------------
    // Token helpers
    // ---------------------------------------------------------------------

    fn at_eof(&self) -> bool {
        matches!(self.content(), TokenContent::Eof)
    }

    fn content(&self) -> &TokenContent {
        match self.tokens.get(self.pos) {
            Some(t) => &t.token.content,
            None => &self.eof.content,
        }
    }

    fn content_at(&self, ahead: usize) -> &TokenContent {
        match self.tokens.get(self.pos + ahead) {
            Some(t) => &t.token.content,
            None => &self.eof.content,
        }
    }

    fn span(&self) -> Span {
        match self.tokens.get(self.pos) {
            Some(t) => t.token.span,
            None => self.eof.span,
        }
    }

    fn doc(&self) -> Option<String> {
        self.tokens
            .get(self.pos)
            .and_then(|t| t.doc)
            .map(str::to_string)
    }

    fn at(&self, content: &TokenContent) -> bool {
        std::mem::discriminant(self.content()) == std::mem::discriminant(content)
    }

    fn at_kw(&self, keyword: Keyword) -> bool {
        matches!(self.content(), TokenContent::Keyword(k) if *k == keyword)
    }

    fn at_ident(&self) -> bool {
        matches!(self.content(), TokenContent::Ident(_))
    }

    fn bump(&mut self) -> Span {
        let span = self.span();
        if !self.at_eof() {
            self.pos += 1;
            self.last_end = span.end;
        }
        span
    }

    fn eat(&mut self, content: &TokenContent) -> Option<Span> {
        if self.at(content) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn eat_kw(&mut self, keyword: Keyword) -> Option<Span> {
        if self.at_kw(keyword) {
            Some(self.bump())
        } else {
            None
        }
    }

    fn error_expected(&mut self, what: &str) {
        let found = self.content().to_string();
        let span = self.span();
        // Avoid stacking several errors at the same location.
        if self
            .diagnostics
            .last()
            .is_some_and(|d| d.span == Some(span) && d.code == DiagnosticCode::UnexpectedToken)
        {
            return;
        }
        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::UnexpectedToken,
            format!("expected {what}, found {found}"),
            span,
        ));
    }

    fn expect(&mut self, content: &TokenContent, what: &str) -> Option<Span> {
        let span = self.eat(content);
        if span.is_none() {
            self.error_expected(what);
        }
        span
    }

    fn expect_ident(&mut self, what: &str) -> Option<Ident> {
        if let TokenContent::Ident(name) = self.content() {
            let name = name.clone();
            let span = self.bump();
            Some(Ident { name, span })
        } else {
            if let TokenContent::Keyword(k) = self.content() {
                let message = format!(
                    "expected {what}, found keyword `{k}`; keywords cannot be used as names"
                );
                let span = self.span();
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::UnexpectedToken,
                    message,
                    span,
                ));
            } else {
                self.error_expected(what);
            }
            None
        }
    }

    fn expect_integer(&mut self, what: &str) -> Option<(u32, Span)> {
        if let TokenContent::Integer(value) = *self.content() {
            Some((value, self.bump()))
        } else {
            self.error_expected(what);
            None
        }
    }

    /// Skips tokens until one satisfies `stop` (not consumed) or EOF.
    fn skip_until(&mut self, stop: impl Fn(&TokenContent) -> bool) {
        while !self.at_eof() && !stop(self.content()) {
            self.bump();
        }
    }

    fn span_from(&self, start: usize) -> Span {
        Span::new(start, self.last_end.max(start))
    }

    // ---------------------------------------------------------------------
    // Grammar
    // ---------------------------------------------------------------------

    fn file(&mut self) -> Option<ChipDecl> {
        if !self.at_kw(Keyword::Chip) {
            self.error_expected("`CHIP`");
            self.skip_until(|c| matches!(c, TokenContent::Keyword(Keyword::Chip)));
            if self.at_eof() {
                return None;
            }
        }
        let doc = self.doc();
        let start = self.bump().start;

        let name = self.expect_ident("chip name").unwrap_or_else(|| Ident {
            name: String::new(),
            span: Span::empty(self.span().start),
        });
        self.expect(&TokenContent::LeftBrace, "`{`");

        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut inputs_span = None;
        let mut outputs_span = None;
        loop {
            if self.at_kw(Keyword::In) {
                let kw_span = self.span();
                let (pins, span) = self.pin_section();
                if inputs_span.is_some() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::DuplicateSection,
                        "duplicate `IN` section",
                        kw_span,
                    ));
                    inputs.extend(pins);
                } else {
                    if outputs_span.is_some() {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::SectionOrder,
                            "the `IN` section must come before the `OUT` section",
                            kw_span,
                        ));
                    }
                    inputs = pins;
                    inputs_span = Some(span);
                }
            } else if self.at_kw(Keyword::Out) {
                let kw_span = self.span();
                let (pins, span) = self.pin_section();
                if outputs_span.is_some() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::DuplicateSection,
                        "duplicate `OUT` section",
                        kw_span,
                    ));
                    outputs.extend(pins);
                } else {
                    outputs = pins;
                    outputs_span = Some(span);
                }
            } else {
                break;
            }
        }

        let implementation = self.implementation();

        // Anything left before the closing brace is unexpected.
        if !self.at(&TokenContent::RightBrace) && !self.at_eof() {
            self.error_expected("`}`");
            self.skip_until(|c| matches!(c, TokenContent::RightBrace));
        }
        self.expect(&TokenContent::RightBrace, "`}`");
        let span = self.span_from(start);

        if !self.at_eof() {
            let span = self.span();
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::UnexpectedToken,
                format!(
                    "unexpected {} after the chip declaration; a file declares exactly one chip",
                    self.content()
                ),
                span,
            ));
        }

        Some(ChipDecl {
            name,
            doc,
            inputs,
            outputs,
            implementation,
            span,
            inputs_span,
            outputs_span,
        })
    }

    /// `("IN" | "OUT") pin { "," pin } ";"`
    fn pin_section(&mut self) -> (Vec<PinDecl>, Span) {
        let start = self.bump().start;
        let mut pins = Vec::new();
        let is_section_end = |c: &TokenContent| {
            matches!(
                c,
                TokenContent::Semicolon
                    | TokenContent::RightBrace
                    | TokenContent::Keyword(
                        Keyword::In | Keyword::Out | Keyword::Parts | Keyword::Builtin
                    )
            )
        };

        if self.at(&TokenContent::Semicolon) {
            self.error_expected("pin name");
        } else {
            loop {
                match self.pin_decl() {
                    Some(pin) => pins.push(pin),
                    None => {
                        self.skip_until(|c| matches!(c, TokenContent::Comma) || is_section_end(c))
                    }
                }
                if self.eat(&TokenContent::Comma).is_none() {
                    break;
                }
            }
        }

        if self.eat(&TokenContent::Semicolon).is_none() {
            self.error_expected("`,` or `;`");
            self.skip_until(is_section_end);
            self.eat(&TokenContent::Semicolon);
        }
        (pins, self.span_from(start))
    }

    /// `IDENT [ "[" INTEGER "]" ]`
    fn pin_decl(&mut self) -> Option<PinDecl> {
        let doc = self.doc();
        let name = self.expect_ident("pin name")?;
        let start = name.span.start;
        let mut width = None;
        if self.at(&TokenContent::LeftBracket) {
            self.bump();
            let (value, span) = self.expect_integer("bus width")?;
            if self.at(&TokenContent::DotDot) {
                let span = self.span();
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::UnexpectedToken,
                    "pin declarations take a width (`name[16]`), not a range",
                    span,
                ));
                return None;
            }
            self.expect(&TokenContent::RightBracket, "`]`")?;
            width = Some(Width { value, span });
        }
        Some(PinDecl {
            name,
            width,
            doc,
            span: self.span_from(start),
        })
    }

    fn implementation(&mut self) -> Option<ChipImpl> {
        if self.at_kw(Keyword::Parts) {
            return Some(self.composite());
        }
        if self.at_kw(Keyword::Builtin) {
            return self.builtin();
        }
        let span = self.span();
        if self.at(&TokenContent::RightBrace) || self.at_eof() {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::MissingImplementation,
                "expected `PARTS:` or `BUILTIN` implementation",
                span,
            ));
            return None;
        }
        self.error_expected("`IN`, `OUT`, `PARTS:` or `BUILTIN`");
        self.skip_until(|c| {
            matches!(
                c,
                TokenContent::RightBrace | TokenContent::Keyword(Keyword::Parts | Keyword::Builtin)
            )
        });
        if self.at_kw(Keyword::Parts) || self.at_kw(Keyword::Builtin) {
            self.implementation()
        } else {
            None
        }
    }

    /// `"PARTS" ":" { part }`
    fn composite(&mut self) -> ChipImpl {
        let start = self.bump().start;
        self.expect(&TokenContent::Colon, "`:` after `PARTS`");
        let keyword_span = self.span_from(start);

        let mut parts = Vec::new();
        while !self.at(&TokenContent::RightBrace) && !self.at_eof() {
            if self.at_ident() {
                if let Some(part) = self.part() {
                    parts.push(part);
                }
            } else {
                let span = self.span();
                let message = match self.content() {
                    TokenContent::Keyword(Keyword::Clocked) => {
                        "`CLOCKED` is only allowed in built-in chips".to_string()
                    }
                    TokenContent::Keyword(Keyword::In | Keyword::Out) => {
                        "pin sections must come before `PARTS:`".to_string()
                    }
                    other => format!("expected a part (chip name), found {other}"),
                };
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::UnexpectedToken,
                    message,
                    span,
                ));
                self.bump();
                // Skip the rest of the junk statement.
                self.skip_until(|c| {
                    matches!(c, TokenContent::Semicolon | TokenContent::RightBrace)
                });
                self.eat(&TokenContent::Semicolon);
            }
        }
        ChipImpl::Composite {
            keyword_span,
            parts,
        }
    }

    /// `IDENT "(" [ binding { "," binding } ] ")" ";"`
    fn part(&mut self) -> Option<PartDecl> {
        let doc = self.doc();
        let chip_name = self.expect_ident("chip name")?;
        let start = chip_name.span.start;

        if self.eat(&TokenContent::LeftParen).is_none() {
            self.error_expected("`(`");
            self.sync_after_part();
            return None;
        }

        let mut connections = Vec::new();
        if !self.at(&TokenContent::RightParen) {
            loop {
                match self.binding() {
                    Some(binding) => connections.push(binding),
                    None => self.skip_until(|c| {
                        matches!(
                            c,
                            TokenContent::Comma
                                | TokenContent::RightParen
                                | TokenContent::Semicolon
                                | TokenContent::RightBrace
                        )
                    }),
                }
                if self.eat(&TokenContent::Comma).is_some() {
                    continue;
                }
                // A forgotten comma between bindings: `Not(in=a out=b)`.
                if self.at_ident()
                    && matches!(
                        self.content_at(1),
                        TokenContent::Equals | TokenContent::LeftBracket
                    )
                {
                    self.error_expected("`,` or `)`");
                    continue;
                }
                break;
            }
        }

        if self.eat(&TokenContent::RightParen).is_none() {
            self.error_expected("`,` or `)`");
            // Unbalanced part: recover at the closing parenthesis, the end of
            // the statement, or the start of the next part.
            loop {
                match self.content() {
                    TokenContent::RightParen => {
                        self.bump();
                        self.eat(&TokenContent::Semicolon);
                        break;
                    }
                    TokenContent::Semicolon => {
                        self.bump();
                        break;
                    }
                    TokenContent::RightBrace | TokenContent::Eof => break,
                    TokenContent::Ident(_)
                        if matches!(self.content_at(1), TokenContent::LeftParen) =>
                    {
                        break;
                    }
                    _ => {
                        self.bump();
                    }
                }
            }
            return Some(PartDecl {
                chip_name,
                connections,
                doc,
                span: self.span_from(start),
            });
        }

        if self.eat(&TokenContent::Semicolon).is_none() {
            // Common typo; the next part (or `}`) can still be parsed.
            self.error_expected("`;` after part");
        }

        Some(PartDecl {
            chip_name,
            connections,
            doc,
            span: self.span_from(start),
        })
    }

    fn sync_after_part(&mut self) {
        self.skip_until(|c| matches!(c, TokenContent::Semicolon | TokenContent::RightBrace));
        self.eat(&TokenContent::Semicolon);
    }

    /// `pin_ref "=" (pin_ref | "true" | "false")`
    fn binding(&mut self) -> Option<PinConnection> {
        let pin = self.pin_ref("pin name")?;
        self.expect(&TokenContent::Equals, "`=`")?;
        let signal = if let Some(span) = self.eat_kw(Keyword::True) {
            self.reject_constant_selector();
            SignalExpr::Constant { value: true, span }
        } else if let Some(span) = self.eat_kw(Keyword::False) {
            self.reject_constant_selector();
            SignalExpr::Constant { value: false, span }
        } else if self.at_ident() {
            SignalExpr::Reference(self.pin_ref("signal name")?)
        } else {
            self.error_expected("signal name, `true` or `false`");
            return None;
        };
        let span = pin.span.merge(signal.span());
        Some(PinConnection { pin, signal, span })
    }

    fn reject_constant_selector(&mut self) {
        if self.at(&TokenContent::LeftBracket) {
            let start = self.span().start;
            self.skip_until(|c| {
                matches!(
                    c,
                    TokenContent::RightBracket
                        | TokenContent::Comma
                        | TokenContent::RightParen
                        | TokenContent::Semicolon
                )
            });
            self.eat(&TokenContent::RightBracket);
            let span = self.span_from(start);
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::UnexpectedToken,
                "constants `true` and `false` cannot be indexed",
                span,
            ));
        }
    }

    /// `IDENT [ "[" INTEGER [ ".." INTEGER ] "]" ]`
    fn pin_ref(&mut self, what: &str) -> Option<PinRef> {
        let name = self.expect_ident(what)?;
        let start = name.span.start;
        let mut selection = None;
        if self.at(&TokenContent::LeftBracket) {
            let sel_start = self.bump().start;
            let (first, _) = self.expect_integer("bit index")?;
            let kind = if self.eat(&TokenContent::DotDot).is_some() {
                let (end, _) = self.expect_integer("end of bit range")?;
                BitSelection::Range { start: first, end }
            } else {
                BitSelection::Index(first)
            };
            self.expect(&TokenContent::RightBracket, "`]`")?;
            selection = Some(Selector {
                kind,
                span: self.span_from(sel_start),
            });
        }
        Some(PinRef {
            name,
            selection,
            span: self.span_from(start),
        })
    }

    /// `"BUILTIN" IDENT ";" [ "CLOCKED" [ IDENT { "," IDENT } ] ";" ]`
    fn builtin(&mut self) -> Option<ChipImpl> {
        let keyword_span = self.bump();
        let name = self.expect_ident("built-in implementation name");
        let name = match name {
            Some(name) => name,
            None => {
                self.skip_until(|c| {
                    matches!(c, TokenContent::Semicolon | TokenContent::RightBrace)
                });
                self.eat(&TokenContent::Semicolon);
                return None;
            }
        };
        if self.eat(&TokenContent::Semicolon).is_none() {
            self.error_expected("`;`");
        }

        let mut clocked = None;
        let mut clocked_span = None;
        if self.at_kw(Keyword::Clocked) {
            let start = self.bump().start;
            let mut pins = Vec::new();
            if !self.at(&TokenContent::Semicolon) {
                loop {
                    match self.expect_ident("pin name") {
                        Some(pin) => pins.push(pin),
                        None => self.skip_until(|c| {
                            matches!(
                                c,
                                TokenContent::Comma
                                    | TokenContent::Semicolon
                                    | TokenContent::RightBrace
                            )
                        }),
                    }
                    if self.eat(&TokenContent::Comma).is_none() {
                        break;
                    }
                }
            }
            if self.eat(&TokenContent::Semicolon).is_none() {
                self.error_expected("`,` or `;`");
                self.skip_until(|c| {
                    matches!(c, TokenContent::Semicolon | TokenContent::RightBrace)
                });
                self.eat(&TokenContent::Semicolon);
            }
            clocked = Some(pins);
            clocked_span = Some(self.span_from(start));
        }

        Some(ChipImpl::Builtin {
            keyword_span,
            name,
            clocked,
            clocked_span,
        })
    }
}
