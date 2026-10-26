//! Structured diagnostics shared by the HDL and TST front-ends.

use std::fmt;

use crate::source::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Information,
    Hint,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Information => "info",
            Severity::Hint => "hint",
        })
    }
}

/// A stable, machine-readable diagnostic identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    // ---- shared lexical ----
    InvalidCharacter,
    UnterminatedComment,
    UnterminatedString,
    InvalidEscape,
    IntegerOverflow,

    // ---- HDL lexical / syntax ----
    InvalidIdentifier,
    NonPortableIdentifier,
    UnexpectedToken,
    DuplicateSection,
    SectionOrder,
    MissingImplementation,

    // ---- HDL semantic ----
    ChipNameMismatch,
    DuplicatePin,
    InvalidWidth,
    UnknownChip,
    ChipHasErrors,
    RecursiveChip,
    UnknownPin,
    SelectorOutOfRange,
    InvalidRange,
    InternalSignalSliced,
    WidthMismatch,
    ConflictingDrivers,
    DuplicateBinding,
    OverlappingInputBinding,
    ConstantAsOutput,
    DrivesTopLevelInput,
    ReadsTopLevelOutput,
    UndrivenSignal,
    UnusedSignal,
    UnusedInput,
    UndrivenOutput,
    UnconnectedPartInput,
    EmptyImplementation,
    UnknownClockedPin,
    DuplicateClockedPin,

    // ---- HDL elaboration ----
    CombinationalCycle,
    UnknownBuiltin,
    BuiltinInterfaceMismatch,
    BuiltinClockMismatch,
    SourceUnavailable,

    // ---- TST syntax ----
    MissingTerminator,
    UnknownCommand,
    UnsupportedCommand,
    InvalidFileName,
    InvalidInteger,
    InvalidFormat,
    InvalidRepeatCount,
    NestedLoop,
    EmptyLoopBody,
    InvalidRelation,
    ExpectedValue,

    // ---- TST semantic ----
    ChipLoadFailed,
    NoChipLoaded,
    UnknownVariable,
    AmbiguousVariable,
    ReadOnlyVariable,
    ValueOutOfRange,
    TypeMismatch,
    InvalidIndex,
    NoOutputFile,
    NoOutputList,
    UnknownBuiltinChip,
    AmbiguousBuiltinChip,
    UnknownMethod,
    InvalidArguments,
}

impl DiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        use DiagnosticCode::*;
        match self {
            InvalidCharacter => "invalid-character",
            UnterminatedComment => "unterminated-comment",
            UnterminatedString => "unterminated-string",
            InvalidEscape => "invalid-escape",
            IntegerOverflow => "integer-overflow",

            InvalidIdentifier => "hdl-invalid-identifier",
            NonPortableIdentifier => "hdl-non-portable-identifier",
            UnexpectedToken => "unexpected-token",
            DuplicateSection => "hdl-duplicate-section",
            SectionOrder => "hdl-section-order",
            MissingImplementation => "hdl-missing-implementation",

            ChipNameMismatch => "hdl-chip-name-mismatch",
            DuplicatePin => "hdl-duplicate-pin",
            InvalidWidth => "hdl-invalid-width",
            UnknownChip => "hdl-unknown-chip",
            ChipHasErrors => "hdl-chip-has-errors",
            RecursiveChip => "hdl-recursive-chip",
            UnknownPin => "hdl-unknown-pin",
            SelectorOutOfRange => "hdl-selector-out-of-range",
            InvalidRange => "hdl-invalid-range",
            InternalSignalSliced => "hdl-internal-signal-sliced",
            WidthMismatch => "hdl-width-mismatch",
            ConflictingDrivers => "hdl-conflicting-drivers",
            DuplicateBinding => "hdl-duplicate-binding",
            OverlappingInputBinding => "hdl-overlapping-input-binding",
            ConstantAsOutput => "hdl-constant-as-output",
            DrivesTopLevelInput => "hdl-drives-top-level-input",
            ReadsTopLevelOutput => "hdl-reads-top-level-output",
            UndrivenSignal => "hdl-undriven-signal",
            UnusedSignal => "hdl-unused-signal",
            UnusedInput => "hdl-unused-input",
            UndrivenOutput => "hdl-undriven-output",
            UnconnectedPartInput => "hdl-unconnected-part-input",
            EmptyImplementation => "hdl-empty-implementation",
            UnknownClockedPin => "hdl-unknown-clocked-pin",
            DuplicateClockedPin => "hdl-duplicate-clocked-pin",

            CombinationalCycle => "hdl-combinational-cycle",
            UnknownBuiltin => "hdl-unknown-builtin",
            BuiltinInterfaceMismatch => "hdl-builtin-interface-mismatch",
            BuiltinClockMismatch => "hdl-builtin-clock-mismatch",
            SourceUnavailable => "hdl-source-unavailable",

            MissingTerminator => "tst-missing-terminator",
            UnknownCommand => "tst-unknown-command",
            UnsupportedCommand => "tst-unsupported-command",
            InvalidFileName => "tst-invalid-file-name",
            InvalidInteger => "tst-invalid-integer",
            InvalidFormat => "tst-invalid-format",
            InvalidRepeatCount => "tst-invalid-repeat-count",
            NestedLoop => "tst-nested-loop",
            EmptyLoopBody => "tst-empty-loop-body",
            InvalidRelation => "tst-invalid-relation",
            ExpectedValue => "tst-expected-value",

            ChipLoadFailed => "tst-chip-load-failed",
            NoChipLoaded => "tst-no-chip-loaded",
            UnknownVariable => "tst-unknown-variable",
            AmbiguousVariable => "tst-ambiguous-variable",
            ReadOnlyVariable => "tst-read-only-variable",
            ValueOutOfRange => "tst-value-out-of-range",
            TypeMismatch => "tst-type-mismatch",
            InvalidIndex => "tst-invalid-index",
            NoOutputFile => "tst-no-output-file",
            NoOutputList => "tst-no-output-list",
            UnknownBuiltinChip => "tst-unknown-builtin-chip",
            AmbiguousBuiltinChip => "tst-ambiguous-builtin-chip",
            UnknownMethod => "tst-unknown-method",
            InvalidArguments => "tst-invalid-arguments",
        }
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A location, possibly within a different source than the diagnostic itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// The source identity (see [`crate::hdl::resolve::SourceFile::id`]).
    /// `None` means the same source as the owning diagnostic.
    pub source: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatedInformation {
    pub message: String,
    pub location: Option<Location>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: DiagnosticCode,
    pub message: String,
    /// The span of the source this diagnostic correlates to. May be `None`
    /// if the diagnostic does not associate with a particular location.
    pub span: Option<Span>,
    pub related: Vec<RelatedInformation>,
}

impl Diagnostic {
    pub fn new(
        severity: Severity,
        code: DiagnosticCode,
        message: impl Into<String>,
        span: Option<Span>,
    ) -> Self {
        Self {
            severity,
            code,
            message: message.into(),
            span,
            related: Vec::new(),
        }
    }

    pub fn error(code: DiagnosticCode, message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Error, code, message, Some(span))
    }

    pub fn warning(code: DiagnosticCode, message: impl Into<String>, span: Span) -> Self {
        Self::new(Severity::Warning, code, message, Some(span))
    }

    pub fn with_related(mut self, message: impl Into<String>, location: Option<Location>) -> Self {
        self.related.push(RelatedInformation {
            message: message.into(),
            location,
        });
        self
    }

    /// Adds related information pointing at a span in the same source.
    pub fn with_related_span(self, message: impl Into<String>, span: Span) -> Self {
        self.with_related(message, Some(Location { source: None, span }))
    }

    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}[{}]: {}", self.severity, self.code, self.message)
    }
}

/// Whether any diagnostic in the slice is an error.
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(Diagnostic::is_error)
}
