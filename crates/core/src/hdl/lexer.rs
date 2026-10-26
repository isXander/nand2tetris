//! Lexer for `.hdl` sources.
//!
//! The lexer is lossless with respect to comments: line, block and
//! documentation comments are emitted as trivia tokens so editor tooling can
//! use them. It never fails outright; problems are reported as diagnostics and
//! lexing continues with the next character.

use std::fmt;

use crate::diagnostic::{Diagnostic, DiagnosticCode};
pub use crate::source::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub content: TokenContent,
    pub span: Span,
}

impl Token {
    /// Comments are trivia: they are skipped by the parser.
    pub fn is_trivia(&self) -> bool {
        matches!(
            self.content,
            TokenContent::Comment(_) | TokenContent::DocComment(_)
        )
    }
}

/// The kind and optional content of a given token.
/// For tokens with state, such as DocComment, the data
/// is not the whole string the token makes up (matching the span)
/// but the logical content of the token.
/// For example `// this is a comment` would make `Comment(" this is a comment")`
/// while its span would also include the slashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenContent {
    Keyword(Keyword),
    Comment(String),
    DocComment(String),
    Ident(String),
    Integer(u32),

    // Delimeters
    LeftBrace,    // {
    RightBrace,   // }
    LeftParen,    // (
    RightParen,   // )
    LeftBracket,  // [
    RightBracket, // ]

    // Separators and operators
    Comma,     // ,
    Semicolon, // ;
    Colon,     // :
    Equals,    // =
    DotDot,    // ..
    Eof,       // EOF
}

impl fmt::Display for TokenContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenContent::Keyword(k) => write!(f, "keyword `{k}`"),
            TokenContent::Comment(_) => f.write_str("comment"),
            TokenContent::DocComment(_) => f.write_str("documentation comment"),
            TokenContent::Ident(name) => write!(f, "identifier `{name}`"),
            TokenContent::Integer(value) => write!(f, "integer `{value}`"),
            TokenContent::LeftBrace => f.write_str("`{`"),
            TokenContent::RightBrace => f.write_str("`}`"),
            TokenContent::LeftParen => f.write_str("`(`"),
            TokenContent::RightParen => f.write_str("`)`"),
            TokenContent::LeftBracket => f.write_str("`[`"),
            TokenContent::RightBracket => f.write_str("`]`"),
            TokenContent::Comma => f.write_str("`,`"),
            TokenContent::Semicolon => f.write_str("`;`"),
            TokenContent::Colon => f.write_str("`:`"),
            TokenContent::Equals => f.write_str("`=`"),
            TokenContent::DotDot => f.write_str("`..`"),
            TokenContent::Eof => f.write_str("end of file"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Keyword {
    Chip,
    In,
    Out,
    Parts,
    Builtin,
    Clocked,
    True,
    False,
}

impl Keyword {
    pub const ALL: [Keyword; 8] = [
        Keyword::Chip,
        Keyword::In,
        Keyword::Out,
        Keyword::Parts,
        Keyword::Builtin,
        Keyword::Clocked,
        Keyword::True,
        Keyword::False,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Keyword::Chip => "CHIP",
            Keyword::In => "IN",
            Keyword::Out => "OUT",
            Keyword::Parts => "PARTS",
            Keyword::Builtin => "BUILTIN",
            Keyword::Clocked => "CLOCKED",
            Keyword::True => "true",
            Keyword::False => "false",
        }
    }

    /// Keywords are case-sensitive.
    pub fn lookup(text: &str) -> Option<Keyword> {
        Keyword::ALL.into_iter().find(|k| k.as_str() == text)
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Tokens and lexical diagnostics. The token list always ends with
/// [`TokenContent::Eof`].
#[derive(Debug, Clone)]
pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

/// Lexes a complete HDL source.
pub fn lex(input: &str) -> LexOutput {
    Lexer::new(input).lex()
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
        while let Some(c) = self.peek_char() {
            let start = self.pos;
            match c {
                c if c.is_whitespace() => {
                    self.pos += c.len_utf8();
                }
                '/' if self.peek_byte(1) == Some(b'/') => self.line_comment(),
                '/' if self.peek_byte(1) == Some(b'*') => self.block_comment(),
                'A'..='Z' | 'a'..='z' | '_' => self.identifier(),
                '0'..='9' => self.integer(),
                '.' if self.peek_byte(1) == Some(b'.') => {
                    self.pos += 2;
                    self.push(TokenContent::DotDot, start);
                }
                '{' => self.single(TokenContent::LeftBrace),
                '}' => self.single(TokenContent::RightBrace),
                '(' => self.single(TokenContent::LeftParen),
                ')' => self.single(TokenContent::RightParen),
                '[' => self.single(TokenContent::LeftBracket),
                ']' => self.single(TokenContent::RightBracket),
                ',' => self.single(TokenContent::Comma),
                ';' => self.single(TokenContent::Semicolon),
                ':' => self.single(TokenContent::Colon),
                '=' => self.single(TokenContent::Equals),
                other => {
                    self.pos += other.len_utf8();
                    let message = if other == '.' {
                        "unexpected `.`; ranges are written with `..`".to_string()
                    } else {
                        format!("invalid character `{}`", other.escape_debug())
                    };
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::InvalidCharacter,
                        message,
                        Span::new(start, self.pos),
                    ));
                }
            }
        }
        let end = self.src.len();
        self.tokens.push(Token {
            content: TokenContent::Eof,
            span: Span::empty(end),
        });
        LexOutput {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_byte(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.pos + ahead).copied()
    }

    fn push(&mut self, content: TokenContent, start: usize) {
        self.tokens.push(Token {
            content,
            span: Span::new(start, self.pos),
        });
    }

    fn single(&mut self, content: TokenContent) {
        let start = self.pos;
        self.pos += 1;
        self.push(content, start);
    }

    fn line_comment(&mut self) {
        let start = self.pos;
        self.pos += 2;
        let end = self.src[self.pos..]
            .find('\n')
            .map_or(self.src.len(), |i| self.pos + i);
        let text = self.src[self.pos..end].trim_end_matches('\r').to_string();
        self.pos = end;
        self.push(TokenContent::Comment(text), start);
    }

    fn block_comment(&mut self) {
        let start = self.pos;
        // `/**/` is an empty ordinary comment, not a documentation comment.
        let is_doc = self.src[start..].starts_with("/**") && !self.src[start..].starts_with("/**/");
        let body_start = if is_doc { start + 3 } else { start + 2 };
        match self.src[body_start..].find("*/") {
            Some(i) => {
                let body_end = body_start + i;
                let text = self.src[body_start..body_end].to_string();
                self.pos = body_end + 2;
                let content = if is_doc {
                    TokenContent::DocComment(text)
                } else {
                    TokenContent::Comment(text)
                };
                self.push(content, start);
            }
            None => {
                self.pos = self.src.len();
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::UnterminatedComment,
                    "unterminated block comment",
                    Span::new(start, (start + 2).min(self.src.len())),
                ));
                let text = self.src[body_start.min(self.pos)..].to_string();
                let content = if is_doc {
                    TokenContent::DocComment(text)
                } else {
                    TokenContent::Comment(text)
                };
                self.push(content, start);
            }
        }
    }

    fn identifier(&mut self) {
        let start = self.pos;
        let mut hyphen = None;
        let mut underscore = None;
        while let Some(&b) = self.bytes.get(self.pos) {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => {}
                b'_' => {
                    underscore.get_or_insert(self.pos);
                }
                // A hyphen only continues an identifier when followed by an
                // identifier character; `a-` followed by anything else is a
                // separate invalid character.
                b'-' if matches!(self.bytes.get(self.pos + 1), Some(c) if c.is_ascii_alphanumeric() || *c == b'_') =>
                {
                    hyphen.get_or_insert(self.pos);
                }
                _ => break,
            }
            self.pos += 1;
        }
        let text = &self.src[start..self.pos];
        let span = Span::new(start, self.pos);

        if let Some(at) = hyphen {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::InvalidIdentifier,
                format!("identifier `{text}` contains a hyphen; identifiers may only contain letters and digits"),
                Span::new(at, at + 1),
            ));
        } else if text.starts_with('_') {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::InvalidIdentifier,
                format!("identifier `{text}` must begin with a letter"),
                span,
            ));
        } else if underscore.is_some() {
            self.diagnostics.push(Diagnostic::warning(
                DiagnosticCode::NonPortableIdentifier,
                format!("identifier `{text}` contains `_`; portable identifiers contain only letters and digits"),
                span,
            ));
        }

        let content = match Keyword::lookup(text) {
            Some(k) => TokenContent::Keyword(k),
            None => TokenContent::Ident(text.to_string()),
        };
        self.push(content, start);
    }

    fn integer(&mut self) {
        let start = self.pos;
        while matches!(self.bytes.get(self.pos), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let text = &self.src[start..self.pos];
        let value = match text.parse::<u32>() {
            Ok(v) => v,
            Err(_) => {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::IntegerOverflow,
                    format!("integer `{text}` is too large"),
                    Span::new(start, self.pos),
                ));
                u32::MAX
            }
        };
        self.push(TokenContent::Integer(value), start);
    }
}
