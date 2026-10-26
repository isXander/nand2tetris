//! Parser for `.tst` scripts with error recovery at command boundaries.

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::source::Span;
use crate::tst::ast::*;
use crate::tst::lexer::{Token, TokenKind};

#[derive(Debug, Clone)]
pub struct ParseResult {
    pub script: Script,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn parse(tokens: &[Token]) -> ParseResult {
    Parser::new(tokens).parse()
}

/// Command words recognised by the hardware dialect (lower case).
pub const COMMANDS: &[&str] = &[
    "load",
    "output-file",
    "compare-to",
    "output-list",
    "set",
    "eval",
    "output",
    "tick",
    "tock",
    "echo",
    "clear-echo",
    "breakpoint",
    "clear-breakpoints",
    "repeat",
    "while",
];

/// Commands of the CPU/VM dialects, rejected by the hardware dialect.
pub const OTHER_DIALECT_COMMANDS: &[(&str, &str)] =
    &[("ticktock", "CPU emulator"), ("vmstep", "VM emulator")];

fn is_command_word(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    COMMANDS.contains(&lower.as_str()) || OTHER_DIALECT_COMMANDS.iter().any(|(c, _)| *c == lower)
}

fn is_decimal(word: &str) -> bool {
    let digits = word.strip_prefix('-').unwrap_or(word);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn is_variable_name(word: &str) -> bool {
    let mut chars = word.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub struct Parser<'t> {
    tokens: Vec<&'t Token>,
    pos: usize,
    eof: Token,
    diagnostics: Vec<Diagnostic>,
    last_end: usize,
    /// Set after `output-list ...,` to explain a following "unknown command".
    after_output_list_comma: bool,
}

impl<'t> Parser<'t> {
    pub fn new(tokens: &'t [Token]) -> Self {
        let end = tokens.last().map_or(0, |t| t.span.end);
        Self {
            tokens: tokens
                .iter()
                .filter(|t| !t.is_trivia() && t.kind != TokenKind::Eof)
                .collect(),
            pos: 0,
            eof: Token {
                kind: TokenKind::Eof,
                span: Span::empty(end),
            },
            diagnostics: Vec::new(),
            last_end: 0,
            after_output_list_comma: false,
        }
    }

    pub fn parse(mut self) -> ParseResult {
        let mut statements = Vec::new();
        while !self.at_eof() {
            if let Some(statement) = self.statement() {
                statements.push(statement);
            }
        }
        ParseResult {
            script: Script { statements },
            diagnostics: self.diagnostics,
        }
    }

    // ---------------------------------------------------------------------
    // Token helpers
    // ---------------------------------------------------------------------

    fn token(&self) -> &Token {
        self.tokens.get(self.pos).copied().unwrap_or(&self.eof)
    }

    fn kind(&self) -> &TokenKind {
        &self.token().kind
    }

    fn span(&self) -> Span {
        self.token().span
    }

    fn at_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn bump(&mut self) -> Span {
        let span = self.span();
        if !self.at_eof() {
            self.pos += 1;
            self.last_end = span.end;
        }
        span
    }

    fn word(&self) -> Option<&str> {
        match self.kind() {
            TokenKind::Word(w) => Some(w),
            _ => None,
        }
    }

    fn at_terminator(&self) -> bool {
        matches!(
            self.kind(),
            TokenKind::Comma | TokenKind::Semicolon | TokenKind::Bang
        )
    }

    /// Whether the current token can end a command's arguments.
    fn at_command_end(&self) -> bool {
        self.at_terminator() || self.at_eof() || matches!(self.kind(), TokenKind::RightBrace)
    }

    fn error(&mut self, code: DiagnosticCode, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    fn error_expected(&mut self, what: &str) {
        let message = format!("expected {what}, found {}", self.kind());
        let span = self.span();
        self.error(DiagnosticCode::UnexpectedToken, message, span);
    }

    fn span_from(&self, start: usize) -> Span {
        Span::new(start, self.last_end.max(start))
    }

    /// Skips to (and consumes) the next terminator, stopping before `}`.
    fn recover_to_terminator(&mut self) -> Option<Terminator> {
        while !self.at_command_end() {
            self.bump();
        }
        self.terminator()
    }

    fn terminator(&mut self) -> Option<Terminator> {
        let kind = match self.kind() {
            TokenKind::Comma => TerminatorKind::Comma,
            TokenKind::Semicolon => TerminatorKind::Semicolon,
            TokenKind::Bang => TerminatorKind::Bang,
            _ => return None,
        };
        Some(Terminator {
            kind,
            span: self.bump(),
        })
    }

    // ---------------------------------------------------------------------
    // Statements
    // ---------------------------------------------------------------------

    fn statement(&mut self) -> Option<Statement> {
        match self.word().map(str::to_ascii_lowercase).as_deref() {
            Some("repeat") => self.repeat_block().map(Statement::Repeat),
            Some("while") => self.while_block().map(Statement::While),
            Some(_) => self.simple_statement().map(Statement::Simple),
            None => {
                let span = self.span();
                let message = match self.kind() {
                    TokenKind::RightBrace => "unmatched `}`".to_string(),
                    TokenKind::Comma | TokenKind::Semicolon | TokenKind::Bang => {
                        format!("unexpected {} without a command", self.kind())
                    }
                    other => format!("expected a command, found {other}"),
                };
                self.error(DiagnosticCode::UnexpectedToken, message, span);
                self.bump();
                None
            }
        }
    }

    fn loop_body(&mut self) -> Vec<SimpleStatement> {
        let mut body = Vec::new();
        while !self.at_eof() && !matches!(self.kind(), TokenKind::RightBrace) {
            match self.word().map(str::to_ascii_lowercase).as_deref() {
                Some("repeat" | "while") => {
                    let span = self.span();
                    self.error(DiagnosticCode::NestedLoop, "loops cannot be nested", span);
                    // Parse and discard the nested block to resynchronise.
                    let _ = self.statement();
                }
                Some(_) => {
                    if let Some(statement) = self.simple_statement() {
                        body.push(statement);
                    }
                }
                None => {
                    let _ = self.statement();
                }
            }
        }
        body
    }

    /// `"{" body "}"`, reporting an empty body. Returns the body.
    fn block(&mut self, keyword: &str, keyword_span: Span) -> Vec<SimpleStatement> {
        if !matches!(self.kind(), TokenKind::LeftBrace) {
            self.error_expected("`{`");
            return Vec::new();
        }
        let open = self.bump();
        let body = self.loop_body();
        if matches!(self.kind(), TokenKind::RightBrace) {
            self.bump();
        } else {
            self.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::UnexpectedToken,
                    format!("unclosed `{keyword}` block: expected `}}`"),
                    self.span(),
                )
                .with_related_span("block opened here", open),
            );
        }
        if body.is_empty() {
            self.error(
                DiagnosticCode::EmptyLoopBody,
                format!("`{keyword}` body must contain at least one command"),
                keyword_span.merge(open),
            );
        }
        if self.at_terminator() {
            let span = self.bump();
            self.error(
                DiagnosticCode::UnexpectedToken,
                "a loop block is not followed by a terminator",
                span,
            );
        }
        body
    }

    fn repeat_block(&mut self) -> Option<RepeatBlock> {
        let keyword_span = self.bump();
        let mut count = None;
        if let Some(word) = self.word().map(str::to_string) {
            let span = self.bump();
            if is_decimal(&word) && !word.starts_with('-') {
                match word.parse::<u64>() {
                    Ok(0) => self.error(
                        DiagnosticCode::InvalidRepeatCount,
                        "repeat count must be positive; omit the count to repeat forever",
                        span,
                    ),
                    Ok(value) => count = Some(Count { value, span }),
                    Err(_) => self.error(
                        DiagnosticCode::InvalidRepeatCount,
                        "repeat count is too large",
                        span,
                    ),
                }
            } else {
                self.error(
                    DiagnosticCode::InvalidRepeatCount,
                    format!("repeat count must be a positive decimal integer, found `{word}`"),
                    span,
                );
            }
        } else if let TokenKind::Percent { .. } = self.kind() {
            let span = self.bump();
            self.error(
                DiagnosticCode::InvalidRepeatCount,
                "repeat count must be a plain positive decimal integer",
                span,
            );
        }
        let body = self.block("repeat", keyword_span);
        Some(RepeatBlock {
            count,
            body,
            keyword_span,
            span: self.span_from(keyword_span.start),
        })
    }

    fn while_block(&mut self) -> Option<WhileBlock> {
        let keyword_span = self.bump();
        let condition = self.condition();
        let body = self.block("while", keyword_span);
        let condition = condition?;
        Some(WhileBlock {
            condition,
            body,
            keyword_span,
            span: self.span_from(keyword_span.start),
        })
    }

    fn condition(&mut self) -> Option<Condition> {
        let start = self.span().start;
        let left = self.operand()?;
        let relation_span = self.span();
        let relation = match self.kind() {
            TokenKind::Equals => Relation::Equal,
            TokenKind::NotEqual => Relation::NotEqual,
            TokenKind::Less => Relation::Less,
            TokenKind::Greater => Relation::Greater,
            TokenKind::LessEqual => Relation::LessEqual,
            TokenKind::GreaterEqual => Relation::GreaterEqual,
            TokenKind::Bang => {
                self.error(
                    DiagnosticCode::InvalidRelation,
                    "`!=` is not a relation; use `<>`",
                    relation_span,
                );
                return None;
            }
            _ => {
                self.error(
                    DiagnosticCode::InvalidRelation,
                    format!(
                        "expected a relation (`=`, `<>`, `<`, `>`, `<=`, `>=`), found {}",
                        self.kind()
                    ),
                    relation_span,
                );
                return None;
            }
        };
        self.bump();
        if matches!(self.kind(), TokenKind::Equals) {
            let span = relation_span.merge(self.bump());
            self.error(
                DiagnosticCode::InvalidRelation,
                format!("`{}=` is not a relation", relation.as_str()),
                span,
            );
        }
        let right = self.operand()?;
        Some(Condition {
            left,
            relation,
            relation_span,
            right,
            span: self.span_from(start),
        })
    }

    fn operand(&mut self) -> Option<Operand> {
        match self.kind().clone() {
            TokenKind::Word(w) if is_decimal(&w) => {
                self.integer().map(|i| Operand::Scalar(Scalar::Integer(i)))
            }
            TokenKind::Word(_) => self.variable().map(Operand::Variable),
            TokenKind::Percent { .. } | TokenKind::String { .. } => {
                self.scalar().map(Operand::Scalar)
            }
            _ => {
                self.error_expected("a variable or constant");
                None
            }
        }
    }

    fn simple_statement(&mut self) -> Option<SimpleStatement> {
        let word = self.word()?.to_string();
        let keyword_span = self.bump();
        let keyword = Ident {
            name: word.clone(),
            span: keyword_span,
        };
        let lower = word.to_ascii_lowercase();
        let follows_output_list_comma = std::mem::take(&mut self.after_output_list_comma);

        let command = match lower.as_str() {
            "load" => self.file_argument("load", "hdl").map(Command::Load),
            "output-file" => self
                .file_argument("output-file", "out")
                .map(Command::OutputFile),
            "compare-to" => self
                .file_argument("compare-to", "cmp")
                .map(Command::CompareTo),
            "output-list" => self.output_list(),
            "set" => self.set(),
            "eval" => Some(Command::Eval),
            "output" => Some(Command::Output),
            "tick" => Some(Command::Tick),
            "tock" => Some(Command::Tock),
            "clear-echo" => Some(Command::ClearEcho),
            "clear-breakpoints" => Some(Command::ClearBreakpoints),
            "echo" => self.echo(),
            "breakpoint" => self.breakpoint(),
            _ if OTHER_DIALECT_COMMANDS.iter().any(|(c, _)| *c == lower) => {
                let dialect = OTHER_DIALECT_COMMANDS
                    .iter()
                    .find(|(c, _)| *c == lower)
                    .map_or("another", |(_, d)| *d);
                self.error(
                    DiagnosticCode::UnsupportedCommand,
                    format!("`{word}` is a {dialect} command and is not supported by the hardware simulator"),
                    keyword_span,
                );
                let arguments = self.arguments();
                Some(Command::Unsupported {
                    name: keyword.clone(),
                    arguments,
                })
            }
            _ if !self.at_command_end() && self.word().is_some() => {
                let method_word = self.word().unwrap().to_string();
                let method = Ident {
                    name: method_word,
                    span: self.bump(),
                };
                let arguments = self.arguments();
                Some(Command::BuiltinMethod {
                    target: keyword.clone(),
                    method,
                    arguments,
                })
            }
            _ => {
                let mut d = Diagnostic::error(
                    DiagnosticCode::UnknownCommand,
                    format!("unknown command `{word}`"),
                    keyword_span,
                );
                if follows_output_list_comma {
                    d = d.with_related(
                        "`output-list` items are separated by whitespace; the preceding `,` ended the output-list command",
                        None,
                    );
                }
                self.diagnostics.push(d);
                None
            }
        };

        let command_span = self.span_from(keyword_span.start);
        let Some(command) = command else {
            self.recover_to_terminator();
            return None;
        };

        let terminator = match self.terminator() {
            Some(t) => Some(t),
            None => {
                let found = self.kind().to_string();
                self.error(
                    DiagnosticCode::MissingTerminator,
                    format!("expected `,`, `;` or `!` after `{word}`, found {found}"),
                    Span::empty(self.last_end),
                );
                // If the next token starts a new command, assume only the
                // terminator was forgotten; otherwise skip the junk.
                let starts_command = self.word().is_some_and(is_command_word);
                if starts_command || self.at_command_end() {
                    self.terminator()
                } else {
                    self.recover_to_terminator()
                }
            }
        };

        if matches!(command, Command::OutputList(_))
            && terminator.is_some_and(|t| t.kind == TerminatorKind::Comma)
        {
            self.after_output_list_comma = true;
        }

        Some(SimpleStatement {
            command,
            command_span,
            keyword,
            terminator,
            span: self.span_from(keyword_span.start),
        })
    }

    /// Raw arguments up to the terminator.
    fn arguments(&mut self) -> Vec<Argument> {
        let mut arguments = Vec::new();
        while !self.at_command_end() {
            let kind = match self.kind().clone() {
                TokenKind::Word(w) => ArgumentKind::Word(w),
                TokenKind::String { value, .. } => ArgumentKind::String(value),
                TokenKind::Percent { format, body } => ArgumentKind::Percent { format, body },
                other => {
                    let span = self.span();
                    self.error(
                        DiagnosticCode::UnexpectedToken,
                        format!("unexpected {other} in command arguments"),
                        span,
                    );
                    self.bump();
                    continue;
                }
            };
            let span = self.bump();
            arguments.push(Argument { kind, span });
        }
        arguments
    }

    fn file_argument(&mut self, command: &str, extension: &str) -> Option<FileName> {
        let (name, quoted) = match self.kind().clone() {
            TokenKind::Word(w) => (w, false),
            TokenKind::String {
                value,
                percent: false,
            } => (value, true),
            _ => {
                self.error_expected(&format!("a `.{extension}` file name after `{command}`"));
                return None;
            }
        };
        let span = self.bump();
        let file = FileName { name, quoted, span };
        if let Some(problem) = check_basename(&file.name) {
            self.error(DiagnosticCode::InvalidFileName, problem, span);
        } else if file.extension() != Some(extension) || file.stem().is_empty() {
            self.error(
                DiagnosticCode::InvalidFileName,
                format!("`{command}` requires a file name ending in `.{extension}`"),
                span,
            );
        }
        Some(file)
    }

    fn output_list(&mut self) -> Option<Command> {
        let mut items = Vec::new();
        while !self.at_command_end() {
            match self.kind().clone() {
                TokenKind::Word(_) => {
                    let Some(variable) = self.variable() else {
                        self.bump();
                        continue;
                    };
                    let start = variable.span.start;
                    let format = match self.kind().clone() {
                        TokenKind::Percent { format, body } => {
                            let span = self.bump();
                            self.output_format(format, &body, span)
                        }
                        TokenKind::String { percent: true, .. } => {
                            let span = self.bump();
                            self.error(
                                DiagnosticCode::InvalidFormat,
                                "an output format is written `%Fpad.len.pad`, e.g. `%B1.16.1`",
                                span,
                            );
                            None
                        }
                        _ => None,
                    };
                    items.push(OutputItem {
                        variable,
                        format,
                        span: self.span_from(start),
                    });
                }
                other => {
                    let span = self.bump();
                    self.error(
                        DiagnosticCode::UnexpectedToken,
                        format!("expected an output-list item (variable name), found {other}"),
                        span,
                    );
                }
            }
        }
        if items.is_empty() {
            let span = self.span();
            self.error(
                DiagnosticCode::UnexpectedToken,
                "`output-list` requires at least one item",
                span,
            );
        }
        Some(Command::OutputList(items))
    }

    fn output_format(&mut self, letter: char, body: &str, span: Span) -> Option<OutputFormat> {
        let Some(kind) = FormatKind::from_letter(letter) else {
            self.error(
                DiagnosticCode::InvalidFormat,
                format!("unknown format `%{letter}`; use `%B`, `%D`, `%X` or `%S`"),
                span,
            );
            return None;
        };
        let numbers: Vec<Option<u32>> = body.split('.').map(|n| n.parse().ok()).collect();
        match numbers.as_slice() {
            [Some(pad_left), Some(len), Some(pad_right)] => {
                if *len == 0 {
                    self.error(
                        DiagnosticCode::InvalidFormat,
                        "the field length of an output format must be at least 1",
                        span,
                    );
                    return None;
                }
                if pad_left.saturating_add(*len).saturating_add(*pad_right) > 1024 {
                    self.error(
                        DiagnosticCode::InvalidFormat,
                        "output field is too wide (limit 1024 columns)",
                        span,
                    );
                    return None;
                }
                Some(OutputFormat {
                    kind,
                    pad_left: *pad_left,
                    len: *len,
                    pad_right: *pad_right,
                    span,
                })
            }
            _ => {
                self.error(
                    DiagnosticCode::InvalidFormat,
                    format!(
                        "invalid output format `%{letter}{body}`; expected `%{letter}padLeft.length.padRight`, e.g. `%{letter}1.16.1`"
                    ),
                    span,
                );
                None
            }
        }
    }

    fn set(&mut self) -> Option<Command> {
        let variable = self.variable()?;
        let value = self.integer()?;
        Some(Command::Set { variable, value })
    }

    fn echo(&mut self) -> Option<Command> {
        match self.kind().clone() {
            TokenKind::String { value, percent } => {
                let span = self.bump();
                Some(Command::Echo(StringLiteral {
                    value,
                    percent,
                    span,
                }))
            }
            _ => {
                self.error_expected("a quoted string after `echo`");
                None
            }
        }
    }

    fn breakpoint(&mut self) -> Option<Command> {
        let variable = self.variable()?;
        let value = self.scalar()?;
        Some(Command::Breakpoint { variable, value })
    }

    /// `name [ "[" [uint] "]" ]`
    fn variable(&mut self) -> Option<Variable> {
        let Some(word) = self.word().map(str::to_string) else {
            self.error_expected("a variable name");
            return None;
        };
        let span = self.bump();
        if !is_variable_name(&word) {
            self.error(
                DiagnosticCode::UnknownVariable,
                format!("`{word}` is not a valid variable name"),
                span,
            );
            return None;
        }
        let name = Ident { name: word, span };
        let mut index = None;
        if matches!(self.kind(), TokenKind::LeftBracket) {
            let open = self.bump();
            let mut value = None;
            if let Some(w) = self.word().map(str::to_string) {
                let span = self.bump();
                if w.contains("..") {
                    self.error(
                        DiagnosticCode::InvalidIndex,
                        "pin slices are not supported in test scripts",
                        span,
                    );
                } else {
                    match w.parse::<u64>() {
                        Ok(v) => value = Some(v),
                        Err(_) => self.error(
                            DiagnosticCode::InvalidIndex,
                            format!("index must be a non-negative decimal integer, found `{w}`"),
                            span,
                        ),
                    }
                }
            }
            if matches!(self.kind(), TokenKind::RightBracket) {
                self.bump();
            } else {
                self.error_expected("`]`");
                return None;
            }
            index = Some(VariableIndex {
                value,
                span: self.span_from(open.start),
            });
        }
        Some(Variable {
            name,
            index,
            span: self.span_from(span.start),
        })
    }

    /// `[ "%D" ] [ "-" ] digits | "%B" bits | "%X" hex`
    fn integer(&mut self) -> Option<IntegerLiteral> {
        let (radix, text) = match self.kind().clone() {
            TokenKind::Word(w) => (Radix::Decimal, w),
            TokenKind::Percent { format: 'D', body } => (Radix::Decimal, body),
            TokenKind::Percent { format: 'B', body } => (Radix::Binary, body),
            TokenKind::Percent { format: 'X', body } => (Radix::Hex, body),
            ref other @ (TokenKind::Percent { .. } | TokenKind::String { .. }) => {
                let letter = match other {
                    TokenKind::Percent { format, .. } => *format,
                    _ => 'S',
                };
                let span = self.bump();
                self.error(
                    DiagnosticCode::InvalidInteger,
                    format!("expected an integer value (decimal, `%B`, `%D` or `%X`), found a `%{letter}` value"),
                    span,
                );
                return None;
            }
            _ => {
                self.error_expected("an integer value");
                return None;
            }
        };
        let span = self.bump();
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) if radix == Radix::Decimal => (true, rest.to_string()),
            _ => (false, text.clone()),
        };
        let valid_digit = |c: char| match radix {
            Radix::Binary => c == '0' || c == '1',
            Radix::Decimal => c.is_ascii_digit(),
            Radix::Hex => c.is_ascii_hexdigit(),
        };
        if digits.is_empty() || !digits.chars().all(valid_digit) {
            let kind = match radix {
                Radix::Binary => "binary",
                Radix::Decimal => "decimal",
                Radix::Hex => "hexadecimal",
            };
            self.error(
                DiagnosticCode::InvalidInteger,
                format!("`{text}` is not a valid {kind} integer"),
                span,
            );
            return None;
        }
        let literal = IntegerLiteral {
            radix,
            negative,
            digits,
            span,
        };
        if let Err(message) = literal.validate() {
            self.error(DiagnosticCode::ValueOutOfRange, message, span);
            return None;
        }
        Some(literal)
    }

    fn scalar(&mut self) -> Option<Scalar> {
        match self.kind().clone() {
            TokenKind::String { value, percent } => {
                let span = self.bump();
                if !percent {
                    self.error(
                        DiagnosticCode::ExpectedValue,
                        format!("string values are written `%S\"{value}\"`"),
                        span,
                    );
                }
                Some(Scalar::String(StringLiteral {
                    value,
                    percent,
                    span,
                }))
            }
            TokenKind::Percent { format: 'S', body } => {
                let span = self.bump();
                // `%Stext` without quotes.
                Some(Scalar::String(StringLiteral {
                    value: body,
                    percent: true,
                    span,
                }))
            }
            TokenKind::Word(w) if !is_decimal(&w) => {
                self.error_expected("a constant value");
                None
            }
            _ => self.integer().map(Scalar::Integer),
        }
    }
}

/// Returns a problem description if `name` is not a plain basename.
pub fn check_basename(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("file name is empty".into());
    }
    if name.contains(['/', '\\']) {
        return Some(format!(
            "`{name}` contains a path separator; only a file name in the script's folder is allowed"
        ));
    }
    if name.contains(':') {
        return Some(format!(
            "`{name}` must not contain a drive or scheme prefix"
        ));
    }
    if name == "." || name == ".." {
        return Some(format!("`{name}` is not a file name"));
    }
    None
}
