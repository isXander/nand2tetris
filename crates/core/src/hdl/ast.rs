//! Location-aware syntax tree for `.hdl` sources.
//!
//! The tree is purely syntactic: whether a right-hand name denotes a
//! top-level pin or an internal signal is decided by semantic analysis.

use crate::source::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ChipDecl {
    pub name: Ident,
    /// The documentation comment preceding `CHIP`, if any.
    pub doc: Option<String>,
    pub inputs: Vec<PinDecl>,
    pub outputs: Vec<PinDecl>,
    /// `None` when the source omitted (or failed to parse) the implementation.
    pub implementation: Option<ChipImpl>,
    /// Span of the whole declaration, from `CHIP` to the closing brace.
    pub span: Span,
    /// Span of the `IN` section (keyword through `;`), if present.
    pub inputs_span: Option<Span>,
    /// Span of the `OUT` section (keyword through `;`), if present.
    pub outputs_span: Option<Span>,
}

impl ChipDecl {
    /// All declared pins, inputs first.
    pub fn pins(&self) -> impl Iterator<Item = &PinDecl> {
        self.inputs.iter().chain(self.outputs.iter())
    }
}

#[derive(Debug, Clone)]
pub enum ChipImpl {
    Composite {
        /// Span of the `PARTS` keyword and its colon.
        keyword_span: Span,
        parts: Vec<PartDecl>,
    },
    Builtin {
        keyword_span: Span,
        name: Ident,
        /// `None` when no `CLOCKED` clause is present; `Some(vec![])` for `CLOCKED;`.
        clocked: Option<Vec<Ident>>,
        /// Span of the `CLOCKED` clause, if present.
        clocked_span: Option<Span>,
    },
}

#[derive(Debug, Clone)]
pub struct PartDecl {
    pub chip_name: Ident,
    pub connections: Vec<PinConnection>,
    pub doc: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PinConnection {
    /// The formal pin of the part being instantiated (left of `=`).
    pub pin: PinRef,
    /// The containing chip's signal or a constant (right of `=`).
    pub signal: SignalExpr,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PinDecl {
    pub name: Ident,
    /// The declared bus width, `None` for a single-bit pin (`name`).
    pub width: Option<Width>,
    pub doc: Option<String>,
    pub span: Span,
}

impl PinDecl {
    /// The effective declared width: `1` when no width was written.
    pub fn width(&self) -> u32 {
        self.width.as_ref().map_or(1, |w| w.value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Width {
    pub value: u32,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PinRef {
    pub name: Ident,
    pub selection: Option<Selector>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum SignalExpr {
    Reference(PinRef),
    Constant { value: bool, span: Span },
}

impl SignalExpr {
    pub fn span(&self) -> Span {
        match self {
            SignalExpr::Reference(r) => r.span,
            SignalExpr::Constant { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selector {
    pub kind: BitSelection,
    /// Span including the brackets.
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitSelection {
    Index(u32),
    /// Inclusive interval `start..end`.
    Range {
        start: u32,
        end: u32,
    },
}

impl BitSelection {
    pub fn start(&self) -> u32 {
        match *self {
            BitSelection::Index(i) => i,
            BitSelection::Range { start, .. } => start,
        }
    }

    pub fn end(&self) -> u32 {
        match *self {
            BitSelection::Index(i) => i,
            BitSelection::Range { end, .. } => end,
        }
    }
}
