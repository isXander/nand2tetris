//! Semantic analysis of a single chip declaration.
//!
//! Analysis needs only the *interfaces* of child chips, supplied through an
//! [`InterfaceResolver`]. It resolves binding directions, infers internal
//! signal widths, validates selectors, widths and drivers, and records symbol
//! references for editor tooling. Whole-hierarchy checks (combinational
//! cycles, built-in registry validation) are performed by the elaborator.

use std::collections::HashMap;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, DiagnosticCode, RelatedInformation, Severity};
use crate::hdl::ast::*;
use crate::hdl::definition::BitRange;
use crate::hdl::interface::{ChipInterface, ChipOrigin, PinDirection};
use crate::source::Span;

/// The default maximum bus width, matching the 16-bit Hack platform.
pub const DEFAULT_MAX_WIDTH: u32 = 16;

#[derive(Debug, Clone)]
pub struct AnalysisOptions {
    /// The chip name implied by the file name (`Foo` for `Foo.hdl`). When set,
    /// a differing `CHIP` name is an error.
    pub expected_name: Option<String>,
    /// Maximum declared bus width.
    pub max_width: u32,
    /// Severity for a composite chip with an empty `PARTS:` section (a stub).
    /// The baseline runner treats stubs as failures; editors may prefer a warning.
    pub empty_implementation: Severity,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            expected_name: None,
            max_width: DEFAULT_MAX_WIDTH,
            empty_implementation: Severity::Error,
        }
    }
}

/// Supplies child chip interfaces during analysis.
pub trait InterfaceResolver {
    fn resolve(&mut self, chip_name: &str) -> ResolvedChip;
}

impl<F: FnMut(&str) -> ResolvedChip> InterfaceResolver for F {
    fn resolve(&mut self, chip_name: &str) -> ResolvedChip {
        self(chip_name)
    }
}

/// The outcome of resolving a child chip. An interface may be available even
/// when the chip has problems (e.g. its implementation has errors), so that
/// the parent's bindings can still be checked.
#[derive(Debug, Clone)]
pub struct ResolvedChip {
    pub interface: Option<Arc<ChipInterface>>,
    pub problem: Option<ResolutionProblem>,
}

impl ResolvedChip {
    pub fn found(interface: Arc<ChipInterface>) -> Self {
        Self {
            interface: Some(interface),
            problem: None,
        }
    }

    pub fn not_found(searched: Vec<String>) -> Self {
        Self {
            interface: None,
            problem: Some(ResolutionProblem::NotFound { searched }),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ResolutionProblem {
    /// Neither a project source nor a built-in exists.
    NotFound { searched: Vec<String> },
    /// The chip exists but could not be compiled.
    HasErrors { related: Vec<RelatedInformation> },
    /// Including the chip would recurse structurally; `chain` starts and ends
    /// with the same chip name.
    Cycle { chain: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    Input(usize),
    Output(usize),
    Internal,
}

#[derive(Debug, Clone)]
pub struct Signal {
    pub name: String,
    pub kind: SignalKind,
    /// `None` for an internal signal whose width could not be inferred
    /// (it has no driver).
    pub width: Option<u32>,
    /// Pin declaration span, or the first use of an internal signal.
    pub declaration: Span,
    /// Right-hand-side spans where a part output drives this signal.
    pub drivers: Vec<Span>,
    /// Right-hand-side spans where this signal feeds a part input.
    pub readers: Vec<Span>,
    /// Uses whose direction is unknown because the part's chip is unresolved.
    pub unresolved_uses: Vec<Span>,
}

#[derive(Debug, Clone)]
pub struct AnalyzedPart {
    pub chip_name: String,
    pub chip_name_span: Span,
    pub span: Span,
    pub interface: Option<Arc<ChipInterface>>,
    pub bindings: Vec<AnalyzedBinding>,
}

#[derive(Debug, Clone)]
pub struct AnalyzedBinding {
    pub span: Span,
    /// The resolved formal pin, or `None` if it is unknown.
    pub formal: Option<FormalPin>,
    pub actual: Actual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormalPin {
    pub direction: PinDirection,
    pub index: usize,
    pub bits: BitRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Actual {
    /// A signal of the containing chip. `bits` is `None` when the selection is
    /// invalid or the width is unknown.
    Signal {
        signal: usize,
        bits: Option<BitRange>,
    },
    Constant(bool),
    /// The binding is invalid in a way that leaves no usable actual.
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolTarget {
    /// A chip type name.
    Chip(String),
    /// A formal pin of a part's chip type.
    FormalPin {
        chip: String,
        direction: PinDirection,
        index: usize,
    },
    /// A signal of the analysed chip (index into [`ChipAnalysis::signals`]).
    Signal(usize),
    /// The `BUILTIN` implementation symbol.
    BuiltinImplementation(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolReference {
    pub span: Span,
    pub target: SymbolTarget,
    /// Whether this reference is the declaration of its target.
    pub is_declaration: bool,
}

#[derive(Debug, Clone)]
pub struct ChipAnalysis {
    pub interface: Arc<ChipInterface>,
    pub signals: Vec<Signal>,
    pub parts: Vec<AnalyzedPart>,
    pub references: Vec<SymbolReference>,
    pub diagnostics: Vec<Diagnostic>,
}

impl ChipAnalysis {
    pub fn has_errors(&self) -> bool {
        crate::diagnostic::has_errors(&self.diagnostics)
    }

    pub fn signal(&self, name: &str) -> Option<(usize, &Signal)> {
        self.signals
            .iter()
            .enumerate()
            .find(|(_, s)| s.name == name)
    }

    /// The innermost symbol reference at a byte offset.
    pub fn reference_at(&self, offset: usize) -> Option<&SymbolReference> {
        self.references
            .iter()
            .filter(|r| r.span.contains(offset))
            .min_by_key(|r| r.span.len())
    }

    /// All references to `target`, in source order.
    pub fn references_to<'a>(
        &'a self,
        target: &'a SymbolTarget,
    ) -> impl Iterator<Item = &'a SymbolReference> + 'a {
        self.references.iter().filter(move |r| &r.target == target)
    }
}

/// Analyses a parsed chip declaration.
pub fn analyze(
    decl: &ChipDecl,
    origin: ChipOrigin,
    resolver: &mut dyn InterfaceResolver,
    options: &AnalysisOptions,
) -> ChipAnalysis {
    let interface = Arc::new(ChipInterface::from_decl(decl, origin));
    let mut analyzer = Analyzer {
        decl,
        options,
        resolver,
        interface: interface.clone(),
        diagnostics: Vec::new(),
        signals: Vec::new(),
        by_name: HashMap::new(),
        references: Vec::new(),
        resolved: HashMap::new(),
        parts: Vec::new(),
        drivers: HashMap::new(),
    };
    analyzer.run();
    let Analyzer {
        signals,
        parts,
        mut references,
        diagnostics,
        ..
    } = analyzer;
    references.sort_by_key(|r| (r.span.start, r.span.end));
    ChipAnalysis {
        interface,
        signals,
        parts,
        references,
        diagnostics,
    }
}

/// Identity of one bit of one part output, used to tell genuine conflicting
/// drivers apart from repeated identical bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DriverBit {
    part: usize,
    pin: usize,
    bit: u32,
    span: Span,
}

impl DriverBit {
    fn same_source(&self, other: &DriverBit) -> bool {
        self.part == other.part && self.pin == other.pin && self.bit == other.bit
    }
}

struct Analyzer<'a> {
    decl: &'a ChipDecl,
    options: &'a AnalysisOptions,
    resolver: &'a mut dyn InterfaceResolver,
    interface: Arc<ChipInterface>,
    diagnostics: Vec<Diagnostic>,
    signals: Vec<Signal>,
    by_name: HashMap<String, usize>,
    references: Vec<SymbolReference>,
    resolved: HashMap<String, ResolvedChip>,
    parts: Vec<AnalyzedPart>,
    /// Per-signal, per-bit driver.
    drivers: HashMap<usize, Vec<Option<DriverBit>>>,
}

impl<'a> Analyzer<'a> {
    fn run(&mut self) {
        self.check_interface();
        match &self.decl.implementation {
            Some(ChipImpl::Builtin { name, clocked, .. }) => {
                self.references.push(SymbolReference {
                    span: name.span,
                    target: SymbolTarget::BuiltinImplementation(name.name.clone()),
                    is_declaration: false,
                });
                if let Some(clocked) = clocked {
                    self.check_clocked(clocked);
                }
            }
            Some(ChipImpl::Composite {
                keyword_span,
                parts,
            }) => {
                if parts.is_empty() {
                    self.diagnostics.push(Diagnostic::new(
                        self.options.empty_implementation,
                        DiagnosticCode::EmptyImplementation,
                        format!(
                            "chip `{}` has no parts; it is an unimplemented stub",
                            self.decl.name.name
                        ),
                        Some(*keyword_span),
                    ));
                }
                self.check_parts(parts);
                if !parts.is_empty() {
                    self.check_usage();
                }
            }
            None => {}
        }
    }

    fn error(&mut self, code: DiagnosticCode, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }

    fn warning(&mut self, code: DiagnosticCode, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(Diagnostic::warning(code, message, span));
    }

    // ---------------------------------------------------------------------
    // Interface
    // ---------------------------------------------------------------------

    fn check_interface(&mut self) {
        let decl = self.decl;
        if let Some(expected) = &self.options.expected_name
            && !decl.name.name.is_empty()
            && decl.name.name != *expected
        {
            self.error(
                DiagnosticCode::ChipNameMismatch,
                format!(
                    "chip `{}` must be declared in `{}.hdl`, but this file is `{}.hdl`",
                    decl.name.name, decl.name.name, expected
                ),
                decl.name.span,
            );
        }

        let mut first: HashMap<&str, Span> = HashMap::new();
        for pin in decl.pins() {
            if let Some(&previous) = first.get(pin.name.name.as_str()) {
                let d = Diagnostic::error(
                    DiagnosticCode::DuplicatePin,
                    format!("pin `{}` is declared more than once", pin.name.name),
                    pin.name.span,
                )
                .with_related_span("first declared here", previous);
                self.diagnostics.push(d);
            } else {
                first.insert(&pin.name.name, pin.name.span);
            }
            if let Some(width) = pin.width {
                if width.value == 0 {
                    self.error(
                        DiagnosticCode::InvalidWidth,
                        "bus width must be at least 1 (`name[N]` declares N bits, not index N)",
                        width.span,
                    );
                } else if width.value > self.options.max_width {
                    self.error(
                        DiagnosticCode::InvalidWidth,
                        format!(
                            "bus width {} exceeds the maximum supported width of {}",
                            width.value, self.options.max_width
                        ),
                        width.span,
                    );
                }
            }
        }

        let interface = self.interface.clone();
        for (i, pin) in interface.inputs.iter().enumerate() {
            self.add_declared_signal(&pin.name, SignalKind::Input(i), pin.width);
        }
        for (i, pin) in interface.outputs.iter().enumerate() {
            self.add_declared_signal(&pin.name, SignalKind::Output(i), pin.width);
        }
        // Declaration references for every declared pin name (including
        // duplicates, which point at the first declaration's signal).
        for pin in decl.pins() {
            if let Some(&id) = self.by_name.get(&pin.name.name) {
                self.references.push(SymbolReference {
                    span: pin.name.span,
                    target: SymbolTarget::Signal(id),
                    is_declaration: self.signals[id].declaration == pin.span,
                });
            }
        }
    }

    fn add_declared_signal(&mut self, name: &str, kind: SignalKind, width: u32) {
        let declaration = self
            .decl
            .pins()
            .find(|p| p.name.name == name)
            .map_or(Span::default(), |p| p.span);
        let id = self.signals.len();
        self.signals.push(Signal {
            name: name.to_string(),
            kind,
            width: Some(width),
            declaration,
            drivers: Vec::new(),
            readers: Vec::new(),
            unresolved_uses: Vec::new(),
        });
        self.by_name.insert(name.to_string(), id);
    }

    fn add_internal_signal(&mut self, name: &Ident, width: Option<u32>) -> usize {
        let id = self.signals.len();
        self.signals.push(Signal {
            name: name.name.clone(),
            kind: SignalKind::Internal,
            width,
            declaration: name.span,
            drivers: Vec::new(),
            readers: Vec::new(),
            unresolved_uses: Vec::new(),
        });
        self.by_name.insert(name.name.clone(), id);
        id
    }

    fn check_clocked(&mut self, clocked: &[Ident]) {
        let mut seen: HashMap<&str, Span> = HashMap::new();
        for pin in clocked {
            match self.by_name.get(&pin.name) {
                Some(&id) => self.references.push(SymbolReference {
                    span: pin.span,
                    target: SymbolTarget::Signal(id),
                    is_declaration: false,
                }),
                None => self.error(
                    DiagnosticCode::UnknownClockedPin,
                    format!(
                        "`CLOCKED` names `{}`, which is not a declared pin",
                        pin.name
                    ),
                    pin.span,
                ),
            }
            if let Some(&previous) = seen.get(pin.name.as_str()) {
                let d = Diagnostic::warning(
                    DiagnosticCode::DuplicateClockedPin,
                    format!("pin `{}` is listed as clocked more than once", pin.name),
                    pin.span,
                )
                .with_related_span("first listed here", previous);
                self.diagnostics.push(d);
            } else {
                seen.insert(&pin.name, pin.span);
            }
        }
    }

    // ---------------------------------------------------------------------
    // Parts
    // ---------------------------------------------------------------------

    fn resolve_chip(&mut self, name: &str) -> ResolvedChip {
        if let Some(r) = self.resolved.get(name) {
            return r.clone();
        }
        let r = self.resolver.resolve(name);
        self.resolved.insert(name.to_string(), r.clone());
        r
    }

    fn check_parts(&mut self, parts: &'a [PartDecl]) {
        // Pass 0: resolve chips and formal pins.
        for part in parts {
            let resolved = self.resolve_chip(&part.chip_name.name);
            self.references.push(SymbolReference {
                span: part.chip_name.span,
                target: SymbolTarget::Chip(part.chip_name.name.clone()),
                is_declaration: false,
            });
            if let Some(problem) = &resolved.problem {
                self.report_resolution_problem(&part.chip_name, problem);
            }
            let interface = resolved.interface.clone();
            let bindings = part
                .connections
                .iter()
                .map(|c| AnalyzedBinding {
                    span: c.span,
                    formal: interface
                        .as_ref()
                        .and_then(|i| self.resolve_formal(i, &c.pin)),
                    actual: Actual::Invalid,
                })
                .collect();
            self.parts.push(AnalyzedPart {
                chip_name: part.chip_name.name.clone(),
                chip_name_span: part.chip_name.span,
                span: part.span,
                interface,
                bindings,
            });
        }

        // Pass 1: output bindings define internal signals and drivers. This
        // happens before reading so that signals may be used before the part
        // that drives them.
        for (p, part) in parts.iter().enumerate() {
            for (b, conn) in part.connections.iter().enumerate() {
                if let Some(formal) = self.parts[p].bindings[b].formal
                    && formal.direction == PinDirection::Output
                {
                    let actual = self.bind_output(p, formal, conn);
                    self.parts[p].bindings[b].actual = actual;
                }
            }
        }

        // Pass 2: input bindings read signals or constants.
        for (p, part) in parts.iter().enumerate() {
            let mut assigned: HashMap<usize, Vec<Option<Span>>> = HashMap::new();
            for (b, conn) in part.connections.iter().enumerate() {
                match self.parts[p].bindings[b].formal {
                    Some(formal) if formal.direction == PinDirection::Input => {
                        self.check_input_overlap(p, formal, conn, &mut assigned);
                        let actual = self.bind_input(formal, conn);
                        self.parts[p].bindings[b].actual = actual;
                    }
                    Some(_) => {}
                    None => {
                        let actual = self.bind_unresolved(conn);
                        self.parts[p].bindings[b].actual = actual;
                    }
                }
            }

            // Unconnected input bits default to zero.
            if let Some(interface) = self.parts[p].interface.clone() {
                for (index, pin) in interface.inputs.iter().enumerate() {
                    let bits = assigned.get(&index);
                    let missing: Vec<u32> = (0..pin.width)
                        .filter(|&bit| bits.is_none_or(|v| v[bit as usize].is_none()))
                        .collect();
                    if missing.is_empty() {
                        continue;
                    }
                    let message = if missing.len() as u32 == pin.width {
                        format!(
                            "input `{}` of `{}` is not connected; it defaults to false",
                            pin.name, interface.name
                        )
                    } else {
                        format!(
                            "bits {} of input `{}` of `{}` are not connected; they default to false",
                            describe_bits(&missing),
                            pin.name,
                            interface.name
                        )
                    };
                    self.warning(
                        DiagnosticCode::UnconnectedPartInput,
                        message,
                        part.chip_name.span,
                    );
                }
            }
        }
    }

    fn report_resolution_problem(&mut self, name: &Ident, problem: &ResolutionProblem) {
        let d = match problem {
            ResolutionProblem::NotFound { searched } => {
                let mut d = Diagnostic::error(
                    DiagnosticCode::UnknownChip,
                    format!(
                        "unknown chip `{}`: no `{}.hdl` in the project and no built-in chip of that name",
                        name.name, name.name
                    ),
                    name.span,
                );
                for location in searched {
                    d = d.with_related(format!("searched {location}"), None);
                }
                d
            }
            ResolutionProblem::HasErrors { related } => {
                let mut d = Diagnostic::error(
                    DiagnosticCode::ChipHasErrors,
                    format!("chip `{}` has errors and cannot be used", name.name),
                    name.span,
                );
                d.related.extend(related.iter().cloned());
                d
            }
            ResolutionProblem::Cycle { chain } => Diagnostic::error(
                DiagnosticCode::RecursiveChip,
                format!(
                    "chip `{}` recursively includes itself: {}",
                    name.name,
                    chain.join(" -> ")
                ),
                name.span,
            ),
        };
        self.diagnostics.push(d);
    }

    fn resolve_formal(&mut self, interface: &ChipInterface, pin: &PinRef) -> Option<FormalPin> {
        let Some(lookup) = interface.pin(&pin.name.name) else {
            let available = interface
                .inputs
                .iter()
                .chain(&interface.outputs)
                .map(|p| format!("`{}`", p.name))
                .collect::<Vec<_>>()
                .join(", ");
            self.error(
                DiagnosticCode::UnknownPin,
                format!(
                    "chip `{}` has no pin named `{}` (available: {})",
                    interface.name,
                    pin.name.name,
                    if available.is_empty() {
                        "none".to_string()
                    } else {
                        available
                    }
                ),
                pin.name.span,
            );
            return None;
        };
        self.references.push(SymbolReference {
            span: pin.name.span,
            target: SymbolTarget::FormalPin {
                chip: interface.name.clone(),
                direction: lookup.direction,
                index: lookup.index,
            },
            is_declaration: false,
        });
        let bits = self.selection_bits(pin, lookup.info.width)?;
        Some(FormalPin {
            direction: lookup.direction,
            index: lookup.index,
            bits,
        })
    }

    /// Validates an optional selector against `width`.
    fn selection_bits(&mut self, pin: &PinRef, width: u32) -> Option<BitRange> {
        let Some(selector) = pin.selection else {
            return Some(BitRange::full(width));
        };
        let (start, end) = (selector.kind.start(), selector.kind.end());
        if start > end {
            self.error(
                DiagnosticCode::InvalidRange,
                format!("invalid range `{start}..{end}`: the start must not exceed the end"),
                selector.span,
            );
            return None;
        }
        if end >= width {
            let valid = if width == 1 {
                "it has a single bit, index 0".to_string()
            } else {
                format!("valid indices are 0..{}", width - 1)
            };
            self.error(
                DiagnosticCode::SelectorOutOfRange,
                format!(
                    "selector is out of range for `{}`, which is {width} bit{} wide; {valid}",
                    pin.name.name,
                    if width == 1 { "" } else { "s" }
                ),
                selector.span,
            );
            return None;
        }
        Some(BitRange::new(start, end - start + 1))
    }

    fn reference_signal(&mut self, span: Span, id: usize) {
        self.references.push(SymbolReference {
            span,
            target: SymbolTarget::Signal(id),
            is_declaration: false,
        });
    }

    fn check_width(&mut self, formal_width: u32, actual_width: u32, conn: &PinConnection) -> bool {
        if formal_width == actual_width {
            return true;
        }
        let formal = &conn.pin.name.name;
        let actual = match &conn.signal {
            SignalExpr::Reference(r) => r.name.name.as_str(),
            SignalExpr::Constant { .. } => "constant",
        };
        self.error(
            DiagnosticCode::WidthMismatch,
            format!(
                "width mismatch: `{formal}` selects {formal_width} bit{} but `{actual}` provides {actual_width}",
                if formal_width == 1 { "" } else { "s" }
            ),
            conn.span,
        );
        false
    }

    fn bind_output(&mut self, part: usize, formal: FormalPin, conn: &PinConnection) -> Actual {
        let r = match &conn.signal {
            SignalExpr::Constant { span, .. } => {
                self.error(
                    DiagnosticCode::ConstantAsOutput,
                    format!(
                        "output pin `{}` cannot be connected to a constant",
                        conn.pin.name.name
                    ),
                    *span,
                );
                return Actual::Invalid;
            }
            SignalExpr::Reference(r) => r,
        };

        let id = match self.by_name.get(&r.name.name) {
            Some(&id) => id,
            None => self.add_internal_signal(&r.name, Some(formal.bits.width)),
        };
        self.reference_signal(r.name.span, id);
        self.signals[id].drivers.push(r.name.span);

        let signal = &self.signals[id];
        match signal.kind {
            SignalKind::Input(_) => {
                self.error(
                    DiagnosticCode::DrivesTopLevelInput,
                    format!(
                        "top-level input `{}` cannot be driven by part output `{}`",
                        r.name.name, conn.pin.name.name
                    ),
                    r.span,
                );
                Actual::Signal {
                    signal: id,
                    bits: None,
                }
            }
            SignalKind::Output(_) => {
                let width = signal.width.unwrap_or(1);
                let Some(bits) = self.selection_bits(r, width) else {
                    return Actual::Signal {
                        signal: id,
                        bits: None,
                    };
                };
                if !self.check_width(formal.bits.width, bits.width, conn) {
                    return Actual::Signal {
                        signal: id,
                        bits: None,
                    };
                }
                self.record_driver(id, bits, part, formal, r.span);
                Actual::Signal {
                    signal: id,
                    bits: Some(bits),
                }
            }
            SignalKind::Internal => {
                if let Some(selector) = r.selection {
                    self.internal_slice_error(&r.name.name, selector.span);
                    return Actual::Signal {
                        signal: id,
                        bits: None,
                    };
                }
                let width = match signal.width {
                    Some(width) => width,
                    None => {
                        self.signals[id].width = Some(formal.bits.width);
                        formal.bits.width
                    }
                };
                if width != formal.bits.width {
                    // A second driver with a different width: report as a conflict.
                    self.record_driver(id, BitRange::full(formal.bits.width), part, formal, r.span);
                    return Actual::Signal {
                        signal: id,
                        bits: None,
                    };
                }
                let bits = BitRange::full(width);
                self.record_driver(id, bits, part, formal, r.span);
                Actual::Signal {
                    signal: id,
                    bits: Some(bits),
                }
            }
        }
    }

    fn internal_slice_error(&mut self, name: &str, span: Span) {
        self.error(
            DiagnosticCode::InternalSignalSliced,
            format!(
                "internal signal `{name}` cannot be indexed or sliced; select the bits from the driving part's output pin instead"
            ),
            span,
        );
    }

    fn record_driver(
        &mut self,
        signal: usize,
        bits: BitRange,
        part: usize,
        formal: FormalPin,
        span: Span,
    ) {
        let width = self.signals[signal].width.unwrap_or(0).max(bits.end());
        let slots = self.drivers.entry(signal).or_default();
        if slots.len() < width as usize {
            slots.resize(width as usize, None);
        }
        let mut conflict = None;
        let mut duplicate = None;
        for (offset, bit) in bits.bits().enumerate() {
            let new = DriverBit {
                part,
                pin: formal.index,
                bit: formal.bits.start + offset as u32,
                span,
            };
            match &slots[bit as usize] {
                None => slots[bit as usize] = Some(new),
                Some(existing) if existing.same_source(&new) => {
                    duplicate.get_or_insert(existing.span);
                }
                Some(existing) => {
                    conflict.get_or_insert(existing.span);
                }
            }
        }
        let name = self.signals[signal].name.clone();
        if let Some(previous) = conflict {
            let d = Diagnostic::error(
                DiagnosticCode::ConflictingDrivers,
                format!(
                    "`{name}` has more than one driver; each signal bit may only be driven once"
                ),
                span,
            )
            .with_related_span("also driven here", previous);
            self.diagnostics.push(d);
        } else if let Some(previous) = duplicate {
            let d = Diagnostic::warning(
                DiagnosticCode::DuplicateBinding,
                format!("`{name}` is connected to the same part output more than once"),
                span,
            )
            .with_related_span("first connected here", previous);
            self.diagnostics.push(d);
        }
    }

    fn check_input_overlap(
        &mut self,
        part: usize,
        formal: FormalPin,
        conn: &PinConnection,
        assigned: &mut HashMap<usize, Vec<Option<Span>>>,
    ) {
        let width = self.parts[part]
            .interface
            .as_ref()
            .and_then(|i| i.inputs.get(formal.index))
            .map_or(formal.bits.end(), |p| p.width);
        let slots = assigned
            .entry(formal.index)
            .or_insert_with(|| vec![None; width as usize]);
        let mut previous = None;
        for bit in formal.bits.bits() {
            match slots[bit as usize] {
                Some(span) => {
                    previous.get_or_insert(span);
                }
                None => slots[bit as usize] = Some(conn.span),
            }
        }
        if let Some(previous) = previous {
            let d = Diagnostic::error(
                DiagnosticCode::OverlappingInputBinding,
                format!(
                    "input `{}` is connected more than once (overlapping bits)",
                    conn.pin.name.name
                ),
                conn.pin.span,
            )
            .with_related_span("previously connected here", previous);
            self.diagnostics.push(d);
        }
    }

    fn bind_input(&mut self, formal: FormalPin, conn: &PinConnection) -> Actual {
        let r = match &conn.signal {
            SignalExpr::Constant { value, .. } => return Actual::Constant(*value),
            SignalExpr::Reference(r) => r,
        };

        let id = match self.by_name.get(&r.name.name) {
            Some(&id) => id,
            None => {
                let id = self.add_internal_signal(&r.name, None);
                self.error(
                    DiagnosticCode::UndrivenSignal,
                    format!(
                        "signal `{}` is not declared and is not driven by any part output",
                        r.name.name
                    ),
                    r.name.span,
                );
                id
            }
        };
        self.reference_signal(r.name.span, id);
        self.signals[id].readers.push(r.name.span);

        let signal = &self.signals[id];
        match signal.kind {
            SignalKind::Input(_) => {
                let width = signal.width.unwrap_or(1);
                let Some(bits) = self.selection_bits(r, width) else {
                    return Actual::Signal {
                        signal: id,
                        bits: None,
                    };
                };
                if !self.check_width(formal.bits.width, bits.width, conn) {
                    return Actual::Signal {
                        signal: id,
                        bits: None,
                    };
                }
                Actual::Signal {
                    signal: id,
                    bits: Some(bits),
                }
            }
            SignalKind::Output(_) => {
                self.error(
                    DiagnosticCode::ReadsTopLevelOutput,
                    format!(
                        "top-level output `{}` cannot be used as a part input; connect the driving part output to an additional internal signal and use that instead",
                        r.name.name
                    ),
                    r.span,
                );
                Actual::Signal {
                    signal: id,
                    bits: None,
                }
            }
            SignalKind::Internal => {
                if let Some(selector) = r.selection {
                    self.internal_slice_error(&r.name.name, selector.span);
                    return Actual::Signal {
                        signal: id,
                        bits: None,
                    };
                }
                match signal.width {
                    Some(width) => {
                        if !self.check_width(formal.bits.width, width, conn) {
                            return Actual::Signal {
                                signal: id,
                                bits: None,
                            };
                        }
                        Actual::Signal {
                            signal: id,
                            bits: Some(BitRange::full(width)),
                        }
                    }
                    // Already reported as undriven.
                    None => Actual::Signal {
                        signal: id,
                        bits: None,
                    },
                }
            }
        }
    }

    /// Records references for a binding whose formal pin is unknown.
    fn bind_unresolved(&mut self, conn: &PinConnection) -> Actual {
        match &conn.signal {
            SignalExpr::Constant { value, .. } => Actual::Constant(*value),
            SignalExpr::Reference(r) => {
                let id = match self.by_name.get(&r.name.name) {
                    Some(&id) => id,
                    None => self.add_internal_signal(&r.name, None),
                };
                self.reference_signal(r.name.span, id);
                self.signals[id].unresolved_uses.push(r.name.span);
                Actual::Signal {
                    signal: id,
                    bits: None,
                }
            }
        }
    }

    // ---------------------------------------------------------------------
    // Usage lints
    // ---------------------------------------------------------------------

    fn check_usage(&mut self) {
        for id in 0..self.signals.len() {
            let signal = &self.signals[id];
            if !signal.unresolved_uses.is_empty() {
                continue;
            }
            match signal.kind {
                SignalKind::Input(_) => {
                    if signal.readers.is_empty() {
                        let (name, span) = (signal.name.clone(), signal.declaration);
                        self.warning(
                            DiagnosticCode::UnusedInput,
                            format!("input `{name}` is never used"),
                            span,
                        );
                    }
                }
                SignalKind::Output(_) => {
                    let width = signal.width.unwrap_or(1);
                    let slots = self.drivers.get(&id);
                    // Connected only by invalid bindings, which were already reported.
                    if slots.is_none() && !signal.drivers.is_empty() {
                        continue;
                    }
                    let missing: Vec<u32> = (0..width)
                        .filter(|&b| {
                            slots.is_none_or(|s| s.get(b as usize).is_none_or(Option::is_none))
                        })
                        .collect();
                    if missing.is_empty() {
                        continue;
                    }
                    let (name, span) = (signal.name.clone(), signal.declaration);
                    let message = if missing.len() as u32 == width {
                        format!("output `{name}` is never driven; it will always be 0")
                    } else {
                        format!(
                            "bits {} of output `{name}` are never driven; they will always be 0",
                            describe_bits(&missing)
                        )
                    };
                    self.warning(DiagnosticCode::UndrivenOutput, message, span);
                }
                SignalKind::Internal => {
                    if signal.readers.is_empty() && !signal.drivers.is_empty() {
                        let (name, span) = (signal.name.clone(), signal.drivers[0]);
                        self.warning(
                            DiagnosticCode::UnusedSignal,
                            format!("internal signal `{name}` is never used"),
                            span,
                        );
                    }
                }
            }
        }
    }
}

/// Formats a sorted list of bit indices as compact ranges, e.g. `0..3, 7`.
fn describe_bits(bits: &[u32]) -> String {
    let mut parts = Vec::new();
    let mut i = 0;
    while i < bits.len() {
        let start = bits[i];
        let mut end = start;
        while i + 1 < bits.len() && bits[i + 1] == end + 1 {
            i += 1;
            end = bits[i];
        }
        parts.push(if start == end {
            start.to_string()
        } else {
            format!("{start}..{end}")
        });
        i += 1;
    }
    parts.join(", ")
}
