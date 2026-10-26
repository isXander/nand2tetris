//! The `.tst` front-end: lexer, parser and static analysis of test scripts
//! for the hardware simulator dialect.
//!
//! Commands, format letters and variable names are matched case-insensitively
//! (with exact-case preference for HDL names); file names are exact.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod semantic;

pub use semantic::{ChipLoader, ScriptAnalysis, analyze};

use crate::diagnostic::Diagnostic;

/// The result of lexing and parsing one test script.
#[derive(Debug, Clone)]
pub struct ParsedScript {
    /// Every token, including comment trivia, ending with EOF.
    pub tokens: Vec<lexer::Token>,
    pub script: ast::Script,
    /// Lexical diagnostics followed by syntax diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Lexes and parses a test script.
pub fn parse(text: &str) -> ParsedScript {
    let lexed = lexer::lex(text);
    let parsed = parser::parse(&lexed.tokens);
    let mut diagnostics = lexed.diagnostics;
    diagnostics.extend(parsed.diagnostics);
    ParsedScript {
        tokens: lexed.tokens,
        script: parsed.script,
        diagnostics,
    }
}

/// Parses and analyses a script, returning all diagnostics in one list.
pub fn check(text: &str, loader: &mut dyn ChipLoader) -> (ParsedScript, ScriptAnalysis) {
    let parsed = parse(text);
    let analysis = analyze(&parsed.script, loader);
    (parsed, analysis)
}
