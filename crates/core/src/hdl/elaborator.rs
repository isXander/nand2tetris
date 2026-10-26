//! Recursive chip compilation: source resolution, semantic analysis,
//! netlist construction and hierarchy-wide validation.
//!
//! The [`Elaborator`] owns one project's resolution context (a
//! [`SourceProvider`] plus a [`BuiltinLibrary`]) and a cache of compiled
//! [`ChipUnit`]s. Compiling a chip compiles its children on demand. Cached
//! units are revalidated against the provider on every top-level request, so
//! edited sources, new local files shadowing built-ins and editor overlays
//! invalidate exactly the affected units and their dependants.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::builtins::{BuiltinChip, BuiltinLibrary};
use crate::diagnostic::{
    Diagnostic, DiagnosticCode, Location, RelatedInformation, Severity, has_errors,
};
use crate::hdl::ParsedHdl;
use crate::hdl::ast::{ChipDecl, ChipImpl};
use crate::hdl::definition::*;
use crate::hdl::interface::{ChipInterface, ChipOrigin, PinDirection};
use crate::hdl::resolve::{SourceFile, SourceLookup, SourceProvider};
use crate::hdl::semantic::{
    Actual, AnalysisOptions, ChipAnalysis, InterfaceResolver, ResolutionProblem, ResolvedChip,
    analyze,
};

/// How many of a child's errors are attached to a parent's "chip has errors"
/// diagnostic as related information.
const MAX_RELATED_CHILD_ERRORS: usize = 5;

#[derive(Debug, Clone, Default)]
pub struct ElaboratorOptions {
    /// Options for semantic analysis. `expected_name` is ignored: it is always
    /// derived from the chip name being compiled.
    pub analysis: AnalysisOptions,
}

impl ElaboratorOptions {
    /// Options suited to editor tooling: stubs are warnings and still yield an
    /// (all-zero) definition.
    pub fn editor() -> Self {
        Self {
            analysis: AnalysisOptions {
                empty_implementation: Severity::Warning,
                ..AnalysisOptions::default()
            },
        }
    }
}

/// Where a compiled unit came from.
#[derive(Debug, Clone)]
pub enum UnitOrigin {
    /// A project-local source.
    Source(SourceFile),
    /// The built-in library (no project source exists).
    Builtin(Arc<BuiltinChip>),
    /// Neither a project source nor a built-in exists.
    Missing { searched: Vec<String> },
    /// A project source exists but could not be read.
    Unreadable { id: String, message: String },
}

/// Everything known about one chip after compilation.
#[derive(Debug, Clone)]
pub struct ChipUnit {
    pub name: String,
    pub origin: UnitOrigin,
    /// The parsed source (for built-ins, the parsed interface text).
    pub parsed: Option<Arc<ParsedHdl>>,
    /// The chip's interface, available whenever the declaration parsed, even
    /// if the chip has errors.
    pub interface: Option<Arc<ChipInterface>>,
    pub analysis: Option<ChipAnalysis>,
    /// The validated definition; present only when there are no errors.
    pub definition: Option<Arc<ChipDefinition>>,
    /// All diagnostics for this unit's own source: lexical, syntax, semantic
    /// and elaboration, in that order. Spans refer to the unit's source.
    pub diagnostics: Vec<Diagnostic>,
    /// Names of the chips this unit's parts refer to.
    pub dependencies: Vec<String>,
}

impl ChipUnit {
    pub fn source(&self) -> Option<&SourceFile> {
        match &self.origin {
            UnitOrigin::Source(file) => Some(file),
            _ => None,
        }
    }

    /// The identity of the source the diagnostics refer to.
    pub fn source_id(&self) -> Option<&str> {
        match &self.origin {
            UnitOrigin::Source(file) => Some(&file.id),
            UnitOrigin::Unreadable { id, .. } => Some(id),
            _ => None,
        }
    }

    /// The source text: the project file or the built-in interface text.
    pub fn text(&self) -> Option<&str> {
        match &self.origin {
            UnitOrigin::Source(file) => Some(&file.text),
            UnitOrigin::Builtin(chip) => Some(&chip.source),
            _ => None,
        }
    }

    pub fn decl(&self) -> Option<&ChipDecl> {
        self.parsed.as_ref().and_then(|p| p.chip.as_ref())
    }

    pub fn has_errors(&self) -> bool {
        has_errors(&self.diagnostics)
    }
}

enum Loaded {
    Unit(Arc<ChipUnit>),
    Cycle {
        chain: Vec<String>,
        interface: Option<Arc<ChipInterface>>,
    },
}

pub struct Elaborator<P> {
    provider: P,
    builtins: Arc<BuiltinLibrary>,
    options: ElaboratorOptions,
    cache: HashMap<String, Arc<ChipUnit>>,
    /// Chips currently being compiled, outermost first.
    stack: Vec<(String, Option<Arc<ChipInterface>>)>,
    /// Units confirmed fresh during the current top-level request.
    verified: HashSet<String>,
    /// Units whose freshness is being checked (cycle guard).
    checking: HashSet<String>,
    depth: usize,
}

impl<P: SourceProvider> Elaborator<P> {
    pub fn new(provider: P, builtins: Arc<BuiltinLibrary>) -> Self {
        Self::with_options(provider, builtins, ElaboratorOptions::default())
    }

    pub fn with_options(
        provider: P,
        builtins: Arc<BuiltinLibrary>,
        options: ElaboratorOptions,
    ) -> Self {
        Self {
            provider,
            builtins,
            options,
            cache: HashMap::new(),
            stack: Vec::new(),
            verified: HashSet::new(),
            checking: HashSet::new(),
            depth: 0,
        }
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Mutable access to the provider (e.g. to update editor overlays).
    /// Changes are picked up automatically by the next request.
    pub fn provider_mut(&mut self) -> &mut P {
        &mut self.provider
    }

    pub fn builtins(&self) -> &Arc<BuiltinLibrary> {
        &self.builtins
    }

    pub fn options(&self) -> &ElaboratorOptions {
        &self.options
    }

    /// Changing options discards the cache.
    pub fn set_options(&mut self, options: ElaboratorOptions) {
        self.options = options;
        self.clear();
    }

    /// Discards every cached unit.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.verified.clear();
    }

    /// Discards one cached unit (dependants are revalidated on next use).
    pub fn invalidate(&mut self, chip_name: &str) {
        self.cache.remove(chip_name);
        self.verified.clear();
    }

    /// The cached unit for a chip, without revalidation.
    pub fn cached(&self, chip_name: &str) -> Option<&Arc<ChipUnit>> {
        self.cache.get(chip_name)
    }

    /// Compiles a chip (and, recursively, its children).
    pub fn compile(&mut self, chip_name: &str) -> Arc<ChipUnit> {
        self.begin();
        let result = match self.load(chip_name) {
            Loaded::Unit(unit) => unit,
            Loaded::Cycle { .. } => unreachable!("cycles cannot occur at the top level"),
        };
        self.end();
        result
    }

    /// Compiles a chip and returns its definition, or the failed unit.
    pub fn definition(&mut self, chip_name: &str) -> Result<Arc<ChipDefinition>, Arc<ChipUnit>> {
        let unit = self.compile(chip_name);
        unit.definition.clone().ok_or(unit)
    }

    /// The interface of a chip, if its declaration can be parsed.
    pub fn interface(&mut self, chip_name: &str) -> Option<Arc<ChipInterface>> {
        self.compile(chip_name).interface.clone()
    }

    /// Compiles a given source text as chip `expected_name` without caching
    /// it. Children are resolved (and cached) through the provider as usual.
    /// Useful for files outside the project or for one-off analysis.
    pub fn compile_file(&mut self, expected_name: Option<&str>, file: SourceFile) -> ChipUnit {
        self.begin();
        let name = expected_name.map(str::to_string).unwrap_or_else(|| {
            let parsed = crate::hdl::parse(&file.text);
            parsed.chip.map(|c| c.name.name).unwrap_or_default()
        });
        let unit = self.build_source(&name, expected_name.is_some(), file);
        self.end();
        unit
    }

    fn begin(&mut self) {
        if self.depth == 0 {
            self.verified.clear();
        }
        self.depth += 1;
    }

    fn end(&mut self) {
        self.depth -= 1;
    }

    // ---------------------------------------------------------------------
    // Loading and caching
    // ---------------------------------------------------------------------

    fn load(&mut self, name: &str) -> Loaded {
        if let Some(pos) = self.stack.iter().position(|(n, _)| n == name) {
            let mut chain: Vec<String> = self.stack[pos..].iter().map(|(n, _)| n.clone()).collect();
            chain.push(name.to_string());
            return Loaded::Cycle {
                chain,
                interface: self.stack[pos].1.clone(),
            };
        }
        if self.cache.contains_key(name) && self.is_fresh(name) {
            return Loaded::Unit(self.cache[name].clone());
        }
        let unit = Arc::new(self.build(name));
        self.cache.insert(name.to_string(), unit.clone());
        self.verified.insert(name.to_string());
        Loaded::Unit(unit)
    }

    fn is_fresh(&mut self, name: &str) -> bool {
        if self.verified.contains(name) || self.checking.contains(name) {
            return true;
        }
        let Some(unit) = self.cache.get(name).cloned() else {
            return false;
        };
        self.checking.insert(name.to_string());
        let source_fresh = match (&unit.origin, self.provider.load_chip(name)) {
            (UnitOrigin::Source(old), SourceLookup::Found(new)) => {
                old.id == new.id && (Arc::ptr_eq(&old.text, &new.text) || old.text == new.text)
            }
            (
                UnitOrigin::Builtin(_) | UnitOrigin::Missing { .. },
                SourceLookup::NotFound { .. },
            ) => true,
            (
                UnitOrigin::Unreadable { id, message },
                SourceLookup::Error {
                    id: new_id,
                    message: new_message,
                },
            ) => *id == new_id && *message == new_message,
            _ => false,
        };
        let fresh = source_fresh && unit.dependencies.iter().all(|d| self.is_fresh(d));
        self.checking.remove(name);
        if fresh {
            self.verified.insert(name.to_string());
        }
        fresh
    }

    fn build(&mut self, name: &str) -> ChipUnit {
        match self.provider.load_chip(name) {
            SourceLookup::Found(file) => self.build_source(name, true, file),
            SourceLookup::Error { id, message } => ChipUnit {
                name: name.to_string(),
                diagnostics: vec![Diagnostic::new(
                    Severity::Error,
                    DiagnosticCode::SourceUnavailable,
                    format!("cannot read `{id}`: {message}"),
                    None,
                )],
                origin: UnitOrigin::Unreadable { id, message },
                parsed: None,
                interface: None,
                analysis: None,
                definition: None,
                dependencies: Vec::new(),
            },
            SourceLookup::NotFound { searched } => match self.builtins.get(name).cloned() {
                Some(chip) => self.build_builtin(chip),
                None => ChipUnit {
                    name: name.to_string(),
                    diagnostics: vec![Diagnostic::new(
                        Severity::Error,
                        DiagnosticCode::UnknownChip,
                        format!(
                            "unknown chip `{name}`: no `{name}.hdl` in the project and no built-in chip of that name"
                        ),
                        None,
                    )],
                    origin: UnitOrigin::Missing { searched },
                    parsed: None,
                    interface: None,
                    analysis: None,
                    definition: None,
                    dependencies: Vec::new(),
                },
            },
        }
    }

    fn build_builtin(&mut self, chip: Arc<BuiltinChip>) -> ChipUnit {
        let definition = builtin_definition(&chip.interface, &chip.symbol, chip.clocked.as_deref());
        ChipUnit {
            name: chip.name.clone(),
            parsed: Some(chip.parsed.clone()),
            interface: Some(chip.interface.clone()),
            analysis: None,
            definition: Some(Arc::new(definition)),
            diagnostics: Vec::new(),
            dependencies: Vec::new(),
            origin: UnitOrigin::Builtin(chip),
        }
    }

    fn build_source(&mut self, name: &str, check_name: bool, file: SourceFile) -> ChipUnit {
        let parsed = Arc::new(crate::hdl::parse(&file.text));
        let mut diagnostics = parsed.diagnostics.clone();
        let mut unit = ChipUnit {
            name: name.to_string(),
            origin: UnitOrigin::Source(file.clone()),
            parsed: Some(parsed.clone()),
            interface: None,
            analysis: None,
            definition: None,
            diagnostics: Vec::new(),
            dependencies: Vec::new(),
        };
        let Some(decl) = parsed.chip.as_ref() else {
            unit.diagnostics = diagnostics;
            return unit;
        };

        let origin = ChipOrigin::Source {
            id: file.id.clone(),
        };
        let interface = Arc::new(ChipInterface::from_decl(decl, origin.clone()));
        let mut options = self.options.analysis.clone();
        options.expected_name = check_name.then(|| name.to_string());

        self.stack.push((name.to_string(), Some(interface.clone())));
        let mut resolver = UnitResolver {
            elaborator: self,
            units: HashMap::new(),
            order: Vec::new(),
        };
        let analysis = analyze(decl, origin, &mut resolver, &options);
        let UnitResolver { units, order, .. } = resolver;
        self.stack.pop();

        diagnostics.extend(analysis.diagnostics.iter().cloned());
        unit.dependencies = order;
        unit.interface = Some(analysis.interface.clone());

        if !has_errors(&diagnostics) {
            unit.definition = self.elaborate(decl, &analysis, &units, &mut diagnostics);
        }
        unit.analysis = Some(analysis);
        unit.diagnostics = diagnostics;
        unit
    }

    // ---------------------------------------------------------------------
    // Definition construction
    // ---------------------------------------------------------------------

    fn elaborate(
        &self,
        decl: &ChipDecl,
        analysis: &ChipAnalysis,
        units: &HashMap<String, Arc<ChipUnit>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<Arc<ChipDefinition>> {
        let interface = &analysis.interface;
        match decl.implementation.as_ref()? {
            ChipImpl::Builtin { name, clocked, .. } => {
                let Some(chip) = self.builtins.by_symbol(&name.name) else {
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::UnknownBuiltin,
                        format!(
                            "no built-in implementation named `{}` is available",
                            name.name
                        ),
                        name.span,
                    ));
                    return None;
                };
                if !chip.interface.same_signature(interface) {
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::BuiltinInterfaceMismatch,
                        format!(
                            "the interface of `{}` does not match built-in `{}`, which is `{}`",
                            interface.name, chip.symbol, chip.interface
                        ),
                        decl.name.span,
                    ));
                    return None;
                }
                let declared: Option<Vec<String>> = clocked
                    .as_ref()
                    .map(|pins| pins.iter().map(|p| p.name.clone()).collect());
                let as_set = |c: &Option<Vec<String>>| {
                    c.as_ref()
                        .map(|v| v.iter().cloned().collect::<HashSet<_>>())
                };
                if as_set(&declared) != as_set(&chip.clocked) {
                    let expected = match &chip.clocked {
                        None => "no `CLOCKED` clause".to_string(),
                        Some(pins) if pins.is_empty() => "`CLOCKED;`".to_string(),
                        Some(pins) => format!("`CLOCKED {};`", pins.join(", ")),
                    };
                    diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::BuiltinClockMismatch,
                        format!(
                            "clocked pins differ from the built-in `{}` interface, which declares {expected}",
                            chip.symbol
                        ),
                        name.span,
                    ));
                }
                Some(Arc::new(builtin_definition(
                    interface,
                    &name.name,
                    declared.as_deref(),
                )))
            }
            ChipImpl::Composite { .. } => {
                let composite = self.build_netlist(analysis, units)?;
                let dependencies = match combinational_dependencies(interface, &composite) {
                    Ok(deps) => deps,
                    Err(cycles) => {
                        for cycle in cycles {
                            diagnostics.push(cycle_diagnostic(&composite, &cycle));
                        }
                        return None;
                    }
                };
                let sequential = composite.parts.iter().any(|p| p.chip.sequential);
                Some(Arc::new(ChipDefinition {
                    name: interface.name.clone(),
                    interface: interface.clone(),
                    implementation: Implementation::Composite(composite),
                    combinational_dependencies: dependencies,
                    sequential,
                }))
            }
        }
    }

    fn build_netlist(
        &self,
        analysis: &ChipAnalysis,
        units: &HashMap<String, Arc<ChipUnit>>,
    ) -> Option<CompositeImplementation> {
        // Signal ids map one-to-one onto net ids: inputs, outputs, internals.
        let nets = analysis
            .signals
            .iter()
            .map(|s| Net {
                name: s.name.clone(),
                width: s.width.unwrap_or(1),
                kind: match s.kind {
                    crate::hdl::semantic::SignalKind::Input(i) => NetKind::Input(i),
                    crate::hdl::semantic::SignalKind::Output(i) => NetKind::Output(i),
                    crate::hdl::semantic::SignalKind::Internal => NetKind::Internal,
                },
            })
            .collect();

        let mut parts = Vec::with_capacity(analysis.parts.len());
        for part in &analysis.parts {
            let chip = units.get(&part.chip_name)?.definition.clone()?;
            let mut inputs = Vec::new();
            let mut outputs = Vec::new();
            for binding in &part.bindings {
                let formal = binding.formal?;
                match (formal.direction, binding.actual) {
                    (PinDirection::Input, Actual::Constant(value)) => {
                        inputs.push(InputConnection {
                            pin: formal.index,
                            pin_bits: formal.bits,
                            source: InputSource::Constant(value),
                        })
                    }
                    (
                        PinDirection::Input,
                        Actual::Signal {
                            signal,
                            bits: Some(bits),
                        },
                    ) => inputs.push(InputConnection {
                        pin: formal.index,
                        pin_bits: formal.bits,
                        source: InputSource::Net { net: signal, bits },
                    }),
                    (
                        PinDirection::Output,
                        Actual::Signal {
                            signal,
                            bits: Some(bits),
                        },
                    ) => outputs.push(OutputConnection {
                        pin: formal.index,
                        pin_bits: formal.bits,
                        net: signal,
                        net_bits: bits,
                    }),
                    // Only reachable when analysis reported an error.
                    _ => return None,
                }
            }
            parts.push(Part {
                chip,
                span: part.span,
                inputs,
                outputs,
            });
        }
        Some(CompositeImplementation { nets, parts })
    }
}

/// Resolves child interfaces during analysis by recursively compiling them.
struct UnitResolver<'e, P> {
    elaborator: &'e mut Elaborator<P>,
    units: HashMap<String, Arc<ChipUnit>>,
    order: Vec<String>,
}

impl<P: SourceProvider> InterfaceResolver for UnitResolver<'_, P> {
    fn resolve(&mut self, chip_name: &str) -> ResolvedChip {
        if !self.order.iter().any(|n| n == chip_name) {
            self.order.push(chip_name.to_string());
        }
        match self.elaborator.load(chip_name) {
            Loaded::Unit(unit) => {
                self.units.insert(chip_name.to_string(), unit.clone());
                resolved_from_unit(&unit)
            }
            Loaded::Cycle { chain, interface } => ResolvedChip {
                interface,
                problem: Some(ResolutionProblem::Cycle { chain }),
            },
        }
    }
}

/// The elaborator can serve as a resolver for ad-hoc analysis (e.g. by an
/// editor analysing an unsaved buffer).
impl<P: SourceProvider> InterfaceResolver for Elaborator<P> {
    fn resolve(&mut self, chip_name: &str) -> ResolvedChip {
        let unit = self.compile(chip_name);
        resolved_from_unit(&unit)
    }
}

pub(crate) fn resolved_from_unit(unit: &ChipUnit) -> ResolvedChip {
    if unit.definition.is_some() {
        return ResolvedChip {
            interface: unit.interface.clone(),
            problem: None,
        };
    }
    if let UnitOrigin::Missing { searched } = &unit.origin {
        return ResolvedChip::not_found(searched.clone());
    }
    let source = unit.source_id().map(str::to_string);
    let related = unit
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .take(MAX_RELATED_CHILD_ERRORS)
        .map(|d| RelatedInformation {
            message: d.message.clone(),
            location: d.span.map(|span| Location {
                source: source.clone(),
                span,
            }),
        })
        .collect();
    ResolvedChip {
        interface: unit.interface.clone(),
        problem: Some(ResolutionProblem::HasErrors { related }),
    }
}

/// Builds the definition of a built-in-implemented chip. Clocked inputs have
/// no combinational effect; clocked outputs depend on nothing combinationally;
/// every other output depends on every unclocked input.
pub fn builtin_definition(
    interface: &Arc<ChipInterface>,
    symbol: &str,
    clocked: Option<&[String]>,
) -> ChipDefinition {
    let clocked_pins: Option<Vec<ClockedPin>> = clocked.map(|names| {
        names
            .iter()
            .filter_map(|n| interface.pin(n))
            .map(|p| ClockedPin {
                direction: p.direction,
                index: p.index,
            })
            .collect()
    });
    let is_clocked = |direction: PinDirection, index: usize| {
        clocked_pins.as_ref().is_some_and(|c| {
            c.iter()
                .any(|p| p.direction == direction && p.index == index)
        })
    };
    let unclocked_inputs: Vec<usize> = (0..interface.inputs.len())
        .filter(|&i| !is_clocked(PinDirection::Input, i))
        .collect();
    let dependencies = (0..interface.outputs.len())
        .map(|o| {
            if is_clocked(PinDirection::Output, o) {
                Vec::new()
            } else {
                unclocked_inputs.clone()
            }
        })
        .collect();
    ChipDefinition {
        name: interface.name.clone(),
        interface: interface.clone(),
        sequential: clocked_pins.is_some(),
        implementation: Implementation::Builtin(BuiltinImplementation {
            symbol: symbol.to_string(),
            clocked: clocked_pins,
        }),
        combinational_dependencies: dependencies,
    }
}

// -------------------------------------------------------------------------
// Dependency analysis
// -------------------------------------------------------------------------

/// Node layout of a composite chip's instantaneous dependency graph: one node
/// per net bit, followed by one node per part pin (inputs, then outputs).
struct DependencyGraph {
    net_offsets: Vec<usize>,
    part_offsets: Vec<usize>,
    adjacency: Vec<Vec<usize>>,
}

impl DependencyGraph {
    fn new(composite: &CompositeImplementation) -> Self {
        let mut next = 0;
        let net_offsets = composite
            .nets
            .iter()
            .map(|n| {
                let o = next;
                next += n.width as usize;
                o
            })
            .collect();
        let part_offsets = composite
            .parts
            .iter()
            .map(|p| {
                let o = next;
                next += p.chip.interface.inputs.len() + p.chip.interface.outputs.len();
                o
            })
            .collect();
        let mut graph = Self {
            net_offsets,
            part_offsets,
            adjacency: vec![Vec::new(); next],
        };

        for (p, part) in composite.parts.iter().enumerate() {
            for conn in &part.inputs {
                if let InputSource::Net { net, bits } = conn.source {
                    let to = graph.part_input(p, conn.pin);
                    for bit in bits.bits() {
                        let from = graph.net_bit(net, bit);
                        graph.adjacency[from].push(to);
                    }
                }
            }
            for conn in &part.outputs {
                let from = graph.part_output(composite, p, conn.pin);
                for bit in conn.net_bits.bits() {
                    let to = graph.net_bit(conn.net, bit);
                    graph.adjacency[from].push(to);
                }
            }
            for (output, inputs) in part.chip.combinational_dependencies.iter().enumerate() {
                let to = graph.part_output(composite, p, output);
                for &input in inputs {
                    let from = graph.part_input(p, input);
                    graph.adjacency[from].push(to);
                }
            }
        }
        graph
    }

    fn net_bit(&self, net: usize, bit: u32) -> usize {
        self.net_offsets[net] + bit as usize
    }

    fn part_input(&self, part: usize, pin: usize) -> usize {
        self.part_offsets[part] + pin
    }

    fn part_output(&self, composite: &CompositeImplementation, part: usize, pin: usize) -> usize {
        self.part_offsets[part] + composite.parts[part].chip.interface.inputs.len() + pin
    }

    /// Maps a node back to what it represents.
    fn describe(&self, node: usize) -> GraphNode {
        if self.part_offsets.first().is_none_or(|&first| node < first) {
            let net = self.net_offsets.partition_point(|&o| o <= node) - 1;
            GraphNode::NetBit(net)
        } else {
            let part = self.part_offsets.partition_point(|&o| o <= node) - 1;
            GraphNode::PartPin(part)
        }
    }
}

enum GraphNode {
    NetBit(usize),
    PartPin(usize),
}

/// A strongly connected component of instantaneous dependencies.
struct Cycle {
    nets: Vec<usize>,
    parts: Vec<usize>,
}

/// Computes each output's combinational input dependencies, or reports every
/// purely combinational cycle.
fn combinational_dependencies(
    interface: &ChipInterface,
    composite: &CompositeImplementation,
) -> Result<Vec<Vec<usize>>, Vec<Cycle>> {
    let graph = DependencyGraph::new(composite);

    let cycles: Vec<Cycle> = strongly_connected_components(&graph.adjacency)
        .into_iter()
        .filter(|scc| scc.len() > 1 || graph.adjacency[scc[0]].contains(&scc[0]))
        .map(|scc| {
            let mut nets = Vec::new();
            let mut parts = Vec::new();
            for node in scc {
                match graph.describe(node) {
                    GraphNode::NetBit(n) => nets.push(n),
                    GraphNode::PartPin(p) => parts.push(p),
                }
            }
            nets.sort_unstable();
            nets.dedup();
            parts.sort_unstable();
            parts.dedup();
            Cycle { nets, parts }
        })
        .collect();
    if !cycles.is_empty() {
        return Err(cycles);
    }

    let output_nets: Vec<(usize, std::ops::Range<usize>)> = (0..interface.outputs.len())
        .map(|o| {
            let net = composite.output_net(interface, o);
            let start = graph.net_offsets[net];
            (o, start..start + composite.nets[net].width as usize)
        })
        .collect();
    let mut dependencies = vec![Vec::new(); interface.outputs.len()];
    let mut visited = vec![false; graph.adjacency.len()];
    let mut stack = Vec::new();
    for input in 0..interface.inputs.len() {
        visited.iter_mut().for_each(|v| *v = false);
        let net = composite.input_net(input);
        for bit in 0..composite.nets[net].width {
            let node = graph.net_bit(net, bit);
            visited[node] = true;
            stack.push(node);
        }
        while let Some(node) = stack.pop() {
            for &next in &graph.adjacency[node] {
                if !visited[next] {
                    visited[next] = true;
                    stack.push(next);
                }
            }
        }
        for (o, range) in &output_nets {
            if range.clone().any(|n| visited[n]) {
                dependencies[*o].push(input);
            }
        }
    }
    Ok(dependencies)
}

fn cycle_diagnostic(composite: &CompositeImplementation, cycle: &Cycle) -> Diagnostic {
    let names: Vec<String> = cycle
        .nets
        .iter()
        .map(|&n| format!("`{}`", composite.nets[n].name))
        .collect();
    let first = cycle
        .parts
        .first()
        .map_or_else(Default::default, |&p| composite.parts[p].span);
    let mut d = Diagnostic::error(
        DiagnosticCode::CombinationalCycle,
        format!(
            "combinational feedback loop through {}; a feedback path must pass through a clocked pin (e.g. a DFF)",
            if names.is_empty() {
                "part pins".to_string()
            } else {
                names.join(", ")
            }
        ),
        first,
    );
    for &p in &cycle.parts {
        let part = &composite.parts[p];
        d = d.with_related_span(
            format!("`{}` is part of the loop", part.chip.name),
            part.span,
        );
    }
    d
}

/// Tarjan's algorithm, iterative. Returns components in reverse topological order.
fn strongly_connected_components(adjacency: &[Vec<usize>]) -> Vec<Vec<usize>> {
    const UNVISITED: usize = usize::MAX;
    let n = adjacency.len();
    let mut index = vec![UNVISITED; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut components = Vec::new();
    let mut counter = 0;
    let mut calls: Vec<(usize, usize)> = Vec::new();

    for root in 0..n {
        if index[root] != UNVISITED {
            continue;
        }
        index[root] = counter;
        low[root] = counter;
        counter += 1;
        stack.push(root);
        on_stack[root] = true;
        calls.push((root, 0));

        while let Some(&(v, edge)) = calls.last() {
            if edge < adjacency[v].len() {
                calls.last_mut().unwrap().1 += 1;
                let w = adjacency[v][edge];
                if index[w] == UNVISITED {
                    index[w] = counter;
                    low[w] = counter;
                    counter += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    calls.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                calls.pop();
                if let Some(&(u, _)) = calls.last() {
                    low[u] = low[u].min(low[v]);
                }
                if low[v] == index[v] {
                    let mut component = Vec::new();
                    loop {
                        let w = stack.pop().unwrap();
                        on_stack[w] = false;
                        component.push(w);
                        if w == v {
                            break;
                        }
                    }
                    components.push(component);
                }
            }
        }
    }
    components
}
