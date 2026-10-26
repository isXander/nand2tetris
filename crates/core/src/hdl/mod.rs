//! The `.hdl` front-end: lexer, parser, semantic analysis and elaboration.
//!
//! Typical use goes through an [`Elaborator`], which resolves and compiles a
//! chip and all of its children:
//!
//! ```no_run
//! use std::sync::Arc;
//! use n2t_core::builtins::BuiltinLibrary;
//! use n2t_core::hdl::{Elaborator, resolve::FsSourceProvider};
//!
//! let mut elaborator = Elaborator::new(
//!     FsSourceProvider::new("projects/01"),
//!     Arc::new(BuiltinLibrary::standard()),
//! );
//! let unit = elaborator.compile("Xor");
//! for diagnostic in &unit.diagnostics {
//!     eprintln!("{diagnostic}");
//! }
//! if let Some(definition) = &unit.definition {
//!     // hand the definition to a simulator
//! #   let _ = definition;
//! }
//! ```

pub mod ast;
pub mod definition;
pub mod elaborator;
pub mod interface;
pub mod lexer;
pub mod parser;
pub mod resolve;
pub mod semantic;

pub use definition::ChipDefinition;
pub use elaborator::{ChipUnit, Elaborator, ElaboratorOptions, UnitOrigin};
pub use interface::{ChipInterface, PinDirection};

use crate::diagnostic::Diagnostic;

/// The result of lexing and parsing one HDL source.
#[derive(Debug, Clone)]
pub struct ParsedHdl {
    /// Every token, including comment trivia, ending with EOF.
    pub tokens: Vec<lexer::Token>,
    pub chip: Option<ast::ChipDecl>,
    /// Lexical diagnostics followed by syntax diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

impl ParsedHdl {
    /// The index of the token (including trivia) covering a byte offset.
    pub fn token_at(&self, offset: usize) -> Option<usize> {
        let i = self.tokens.partition_point(|t| t.span.end < offset);
        (i < self.tokens.len() && self.tokens[i].span.contains(offset)).then_some(i)
    }
}

/// Lexes and parses an HDL source.
pub fn parse(text: &str) -> ParsedHdl {
    let lexed = lexer::lex(text);
    let parsed = parser::parse(&lexed.tokens);
    let mut diagnostics = lexed.diagnostics;
    diagnostics.extend(parsed.diagnostics);
    ParsedHdl {
        tokens: lexed.tokens,
        chip: parsed.chip,
        diagnostics,
    }
}
