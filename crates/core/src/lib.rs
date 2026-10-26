//! Front-end for the Nand to Tetris toolchain: lexing, parsing, semantic
//! analysis and elaboration of `.hdl` chips and `.tst` test scripts.
//!
//! This crate performs no simulation. It produces validated, immutable
//! [`hdl::ChipDefinition`]s for a simulator, plus diagnostics, symbol
//! information and syntax trees for the CLI and language server.

pub mod builtins;
pub mod diagnostic;
pub mod hdl;
pub mod source;
pub mod tst;

pub use diagnostic::{Diagnostic, DiagnosticCode, Severity};
pub use source::{LineIndex, Position, Span};
