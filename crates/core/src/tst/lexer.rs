//! Lexer for `.tst` test scripts.
//!
//! Script syntax is context sensitive (file names, `%`-prefixed literals and
//! output formats share characters), so the lexer produces deliberately
//! coarse tokens: a [`TokenKind::Word`] is any run of name-like characters
//! (command names such as `output-list`, variables, file names such as
//! `Not.hdl`, and decimal integers such as `-1`), and a
//! [`TokenKind::Percent`] holds a format letter plus its raw body. The parser
//! interprets them according to the command being parsed.

use std::fmt;

use crate::diagnostic::{Diagnostic, DiagnosticCode};
use crate::source::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn is_trivia(&self) -> bool {
        matches!(self.kind, TokenKind::Comment(_) | TokenKind::DocComment(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// A run of letters, digits and `_ . - : $ / \` (the latter two only so that
    /// paths can be diagnosed).
    Word(String),
    /// A double-quoted string with escapes processed. `percent` is true for
    /// the `%S"..."` form.
    String {
        value: String,
        percent: bool,
    },
    /// `%` followed by a format letter (upper-cased) and its raw body, e.g.
    /// `%B0101` → (`B`, `0101`), `%D1.6.1` → (`D`, `1.6.1`).
    Percent {
        format: char,
        body: String,
    },

    LeftBrace,    // {
    RightBrace,   // }
    LeftBracket,  // [
    RightBracket, // ]

    Comma,     // ,
    Semicolon, // ;
    Bang,      // !

    Equals,       // =
    Less,         // <
    Greater,      // >
    LessEqual,    // <=
    GreaterEqual, // >=
    NotEqual,     // <>

    Comment(String),
    DocComment(String),
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Word(w) => write!(f, "`{w}`"),
            TokenKind::String {
                value,
                percent: false,
            } => write!(f, "string \"{value}\""),
            TokenKind::String {
                value,
                percent: true,
            } => write!(f, "string %S\"{value}\""),
            TokenKind::Percent { format, body } => write!(f, "`%{format}{body}`"),
            TokenKind::LeftBrace => f.write_str("`{`"),
            TokenKind::RightBrace => f.write_str("`}`"),
            TokenKind::LeftBracket => f.write_str("`[`"),
            TokenKind::RightBracket => f.write_str("`]`"),
            TokenKind::Comma => f.write_str("`,`"),
            TokenKind::Semicolon => f.write_str("`;`"),
            TokenKind::Bang => f.write_str("`!`"),
            TokenKind::Equals => f.write_str("`=`"),
            TokenKind::Less => f.write_str("`<`"),
            TokenKind::Greater => f.write_str("`>`"),
            TokenKind::LessEqual => f.write_str("`<=`"),
            TokenKind::GreaterEqual => f.write_str("`>=`"),
            TokenKind::NotEqual => f.write_str("`<>`"),
            TokenKind::Comment(_) => f.write_str("comment"),
            TokenKind::DocComment(_) => f.write_str("documentation comment"),
            TokenKind::Eof => f.write_str("end of file"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn lex(input: &str) -> LexOutput {
    Lexer::new(input).lex()
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b':' | b'$' | b'\\')
}

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: crate::source::bom_len(src),
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn lex(mut self) -> LexOutput {
        while let Some(c) = self.src[self.pos..].chars().next() {
            let start = self.pos;
            match c {
                c if c.is_whitespace() => self.pos += c.len_utf8(),
                '/' if self.peek(1) == Some(b'/') => self.line_comment(),
                '/' if self.peek(1) == Some(b'*') => self.block_comment(),
                '"' => self.string(start, false),
                '%' => self.percent(),
                '{' => self.single(TokenKind::LeftBrace),
                '}' => self.single(TokenKind::RightBrace),
                '[' => self.single(TokenKind::LeftBracket),
                ']' => self.single(TokenKind::RightBracket),
                ',' => self.single(TokenKind::Comma),
                ';' => self.single(TokenKind::Semicolon),
                '!' => self.single(TokenKind::Bang),
                '=' => self.single(TokenKind::Equals),
                '<' => match self.peek(1) {
                    Some(b'=') => self.double(TokenKind::LessEqual),
                    Some(b'>') => self.double(TokenKind::NotEqual),
                    _ => self.single(TokenKind::Less),
                },
                '>' => match self.peek(1) {
                    Some(b'=') => self.double(TokenKind::GreaterEqual),
                    _ => self.single(TokenKind::Greater),
                },
                c if c.is_ascii() && (is_word_byte(c as u8) || c == '/') => self.word(),
                other => {
                    self.pos += other.len_utf8();
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::InvalidCharacter,
                        format!("invalid character `{}`", other.escape_debug()),
                        Span::new(start, self.pos),
                    ));
                }
            }
        }
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::empty(self.src.len()),
        });
        LexOutput {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn peek(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.pos + ahead).copied()
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.pos),
        });
    }

    fn single(&mut self, kind: TokenKind) {
        let start = self.pos;
        self.pos += 1;
        self.push(kind, start);
    }

    fn double(&mut self, kind: TokenKind) {
        let start = self.pos;
        self.pos += 2;
        self.push(kind, start);
    }

    fn line_comment(&mut self) {
        let start = self.pos;
        let end = self.src[self.pos..]
            .find('\n')
            .map_or(self.src.len(), |i| self.pos + i);
        let text = self.src[start + 2..end].trim_end_matches('\r').to_string();
        self.pos = end;
        self.push(TokenKind::Comment(text), start);
    }

    fn block_comment(&mut self) {
        let start = self.pos;
        let is_doc = self.src[start..].starts_with("/**") && !self.src[start..].starts_with("/**/");
        let body_start = if is_doc { start + 3 } else { start + 2 };
        let (text, end) = match self.src[body_start..].find("*/") {
            Some(i) => (
                self.src[body_start..body_start + i].to_string(),
                body_start + i + 2,
            ),
            None => {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::UnterminatedComment,
                    "unterminated block comment",
                    Span::new(start, start + 2),
                ));
                (self.src[body_start..].to_string(), self.src.len())
            }
        };
        self.pos = end;
        let kind = if is_doc {
            TokenKind::DocComment(text)
        } else {
            TokenKind::Comment(text)
        };
        self.push(kind, start);
    }

    fn word(&mut self) {
        let start = self.pos;
        while let Some(b) = self.peek(0) {
            let continues =
                is_word_byte(b) || (b == b'/' && !matches!(self.peek(1), Some(b'/' | b'*')));
            if !continues {
                break;
            }
            self.pos += 1;
        }
        let text = self.src[start..self.pos].to_string();
        self.push(TokenKind::Word(text), start);
    }

    /// Lexes a quoted string starting at the opening quote (`self.pos`).
    fn string(&mut self, start: usize, percent: bool) {
        self.pos += 1; // opening quote
        let mut value = String::new();
        loop {
            let Some(c) = self.src[self.pos..].chars().next() else {
                self.unterminated_string(start);
                break;
            };
            match c {
                '"' => {
                    self.pos += 1;
                    break;
                }
                '\n' | '\r' => {
                    self.unterminated_string(start);
                    break;
                }
                '\\' => {
                    let escape_start = self.pos;
                    self.pos += 1;
                    let Some(e) = self.src[self.pos..].chars().next() else {
                        self.unterminated_string(start);
                        break;
                    };
                    self.pos += e.len_utf8();
                    match e {
                        '"' => value.push('"'),
                        '\\' => value.push('\\'),
                        'n' => value.push('\n'),
                        'r' => value.push('\r'),
                        't' => value.push('\t'),
                        other => {
                            value.push(other);
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::InvalidEscape,
                                format!(
                                    "invalid escape `\\{}`; supported escapes are \\\", \\\\, \\n, \\r and \\t",
                                    other.escape_debug()
                                ),
                                Span::new(escape_start, self.pos),
                            ));
                        }
                    }
                }
                other => {
                    value.push(other);
                    self.pos += other.len_utf8();
                }
            }
        }
        self.push(TokenKind::String { value, percent }, start);
    }

    fn unterminated_string(&mut self, start: usize) {
        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::UnterminatedString,
            "unterminated string literal",
            Span::new(start, self.pos),
        ));
    }

    fn percent(&mut self) {
        let start = self.pos;
        self.pos += 1;
        let Some(letter) = self.peek(0).filter(u8::is_ascii_alphabetic) else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::InvalidFormat,
                "expected a format letter (`B`, `D`, `X` or `S`) after `%`",
                Span::new(start, self.pos),
            ));
            return;
        };
        self.pos += 1;
        let format = (letter as char).to_ascii_uppercase();

        if format == 'S' {
            // `%S"text"`, optionally with whitespace before the quote.
            let mut lookahead = self.pos;
            while matches!(self.bytes.get(lookahead), Some(b' ' | b'\t')) {
                lookahead += 1;
            }
            if self.bytes.get(lookahead) == Some(&b'"') {
                self.pos = lookahead;
                self.string(start, true);
                return;
            }
        }

        let body_start = self.pos;
        while matches!(self.peek(0), Some(b) if b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            self.pos += 1;
        }
        let body = self.src[body_start..self.pos].to_string();
        self.push(TokenKind::Percent { format, body }, start);
    }
}
