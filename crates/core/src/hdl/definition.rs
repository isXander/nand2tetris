//! Immutable, validated chip definitions produced by the elaborator.
//!
//! A [`ChipDefinition`] is the construction plan a simulator instantiates. It
//! contains no runtime state and no syntax; child parts refer to their own
//! (shared, immutable) definitions. Two instances of the same definition must
//! own independent state.

use std::fmt;
use std::sync::Arc;

use crate::hdl::interface::{ChipInterface, PinDirection};
use crate::source::Span;

/// A contiguous run of bits within a pin or net, least-significant bit first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BitRange {
    pub start: u32,
    pub width: u32,
}

impl BitRange {
    pub const fn new(start: u32, width: u32) -> Self {
        Self { start, width }
    }

    /// All bits of a `width`-bit bus.
    pub const fn full(width: u32) -> Self {
        Self { start: 0, width }
    }

    /// Exclusive end bit.
    pub const fn end(&self) -> u32 {
        self.start + self.width
    }

    /// Inclusive last bit.
    pub const fn last(&self) -> u32 {
        self.start + self.width - 1
    }

    pub fn bits(&self) -> std::ops::Range<u32> {
        self.start..self.end()
    }

    pub const fn overlaps(&self, other: &BitRange) -> bool {
        self.start < other.end() && other.start < self.end()
    }

    /// A mask of the selected bits, for widths up to 64.
    pub fn mask(&self) -> u64 {
        let ones = if self.width >= 64 {
            u64::MAX
        } else {
            (1u64 << self.width) - 1
        };
        ones << self.start
    }
}

impl fmt::Display for BitRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.width == 1 {
            write!(f, "[{}]", self.start)
        } else {
            write!(f, "[{}..{}]", self.start, self.last())
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChipDefinition {
    pub name: String,
    pub interface: Arc<ChipInterface>,
    pub implementation: Implementation,
    /// For each output pin (by index), the sorted input pin indices on which it
    /// depends *combinationally* (without crossing a clocked boundary).
    pub combinational_dependencies: Vec<Vec<usize>>,
    /// Whether this chip or any descendant holds clocked state.
    pub sequential: bool,
}

impl ChipDefinition {
    /// Whether output pin `output` combinationally depends on input pin `input`.
    pub fn depends_on(&self, output: usize, input: usize) -> bool {
        self.combinational_dependencies
            .get(output)
            .is_some_and(|deps| deps.binary_search(&input).is_ok())
    }

    pub fn as_composite(&self) -> Option<&CompositeImplementation> {
        match &self.implementation {
            Implementation::Composite(c) => Some(c),
            Implementation::Builtin(_) => None,
        }
    }

    pub fn as_builtin(&self) -> Option<&BuiltinImplementation> {
        match &self.implementation {
            Implementation::Builtin(b) => Some(b),
            Implementation::Composite(_) => None,
        }
    }

    /// Visits this definition and every descendant part definition (depth
    /// first, parents before children). Shared definitions are visited once
    /// per occurrence, mirroring the instance tree.
    pub fn visit_instances(&self, visit: &mut dyn FnMut(&ChipDefinition, &[usize])) {
        fn go(
            def: &ChipDefinition,
            path: &mut Vec<usize>,
            visit: &mut dyn FnMut(&ChipDefinition, &[usize]),
        ) {
            visit(def, path);
            if let Implementation::Composite(c) = &def.implementation {
                for (i, part) in c.parts.iter().enumerate() {
                    path.push(i);
                    go(&part.chip, path, visit);
                    path.pop();
                }
            }
        }
        go(self, &mut Vec::new(), visit);
    }
}

#[derive(Debug, Clone)]
pub enum Implementation {
    Builtin(BuiltinImplementation),
    Composite(CompositeImplementation),
}

#[derive(Debug, Clone)]
pub struct BuiltinImplementation {
    /// The symbolic implementation name from `BUILTIN Name;`, resolved by the
    /// simulator's executable built-in registry.
    pub symbol: String,
    /// Clock-sensitive pins from `CLOCKED`; `None` when no clause is present.
    pub clocked: Option<Vec<ClockedPin>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockedPin {
    pub direction: PinDirection,
    pub index: usize,
}

/// A composite chip's netlist.
///
/// Nets are ordered: first one net per top-level input (net `i` is input `i`),
/// then one per top-level output, then internal signals. Every net starts at
/// zero; undriven bits (including unconnected outputs) stay zero.
#[derive(Debug, Clone)]
pub struct CompositeImplementation {
    pub nets: Vec<Net>,
    pub parts: Vec<Part>,
}

impl CompositeImplementation {
    pub fn input_net(&self, input: usize) -> usize {
        input
    }

    pub fn output_net(&self, interface: &ChipInterface, output: usize) -> usize {
        interface.inputs.len() + output
    }

    pub fn internal_nets(&self) -> impl Iterator<Item = (usize, &Net)> {
        self.nets
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == NetKind::Internal)
    }

    pub fn net_by_name(&self, name: &str) -> Option<(usize, &Net)> {
        self.nets.iter().enumerate().find(|(_, n)| n.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    pub name: String,
    pub width: u32,
    pub kind: NetKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetKind {
    Input(usize),
    Output(usize),
    Internal,
}

#[derive(Debug, Clone)]
pub struct Part {
    pub chip: Arc<ChipDefinition>,
    /// Span of the part statement in the containing chip's source.
    pub span: Span,
    /// Connections into the part's input pins. Unconnected input bits are zero.
    pub inputs: Vec<InputConnection>,
    /// Connections from the part's output pins into nets of the containing chip.
    pub outputs: Vec<OutputConnection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputConnection {
    /// Index into the part chip's `interface.inputs`.
    pub pin: usize,
    pub pin_bits: BitRange,
    pub source: InputSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputSource {
    /// Bits `bits` of net `net` (same width as `pin_bits`).
    Net { net: usize, bits: BitRange },
    /// Every selected pin bit is set to this constant.
    Constant(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputConnection {
    /// Index into the part chip's `interface.outputs`.
    pub pin: usize,
    pub pin_bits: BitRange,
    pub net: usize,
    /// Same width as `pin_bits`.
    pub net_bits: BitRange,
}
