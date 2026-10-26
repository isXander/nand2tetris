//! Public chip interfaces: the information a parent chip needs about a child.

use std::fmt;

use crate::hdl::ast::ChipDecl;
use crate::source::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PinDirection {
    Input,
    Output,
}

impl fmt::Display for PinDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PinDirection::Input => "input",
            PinDirection::Output => "output",
        })
    }
}

/// Where a chip's interface came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChipOrigin {
    /// A project-local `.hdl` source with the given source identity.
    Source { id: String },
    /// The built-in interface library.
    Builtin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinInfo {
    pub name: String,
    pub width: u32,
    /// Declaration span within the chip's source (local or built-in text).
    pub span: Option<Span>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipInterface {
    pub name: String,
    pub inputs: Vec<PinInfo>,
    pub outputs: Vec<PinInfo>,
    pub doc: Option<String>,
    pub origin: ChipOrigin,
    /// Span of the chip name within its source.
    pub name_span: Option<Span>,
}

/// A pin found by name lookup.
#[derive(Debug, Clone, Copy)]
pub struct PinLookup<'a> {
    pub direction: PinDirection,
    pub index: usize,
    pub info: &'a PinInfo,
}

impl ChipInterface {
    /// Builds an interface from a parsed declaration. Duplicate pin names keep
    /// their first declaration; invalid zero widths are treated as width 1 so
    /// that downstream analysis can proceed (the error is reported by
    /// semantic analysis).
    pub fn from_decl(decl: &ChipDecl, origin: ChipOrigin) -> Self {
        let mut seen = std::collections::HashSet::new();
        let mut convert = |pins: &[crate::hdl::ast::PinDecl]| {
            pins.iter()
                .filter(|p| seen.insert(p.name.name.clone()))
                .map(|p| PinInfo {
                    name: p.name.name.clone(),
                    width: p.width().max(1),
                    span: Some(p.span),
                    doc: p.doc.clone(),
                })
                .collect::<Vec<_>>()
        };
        let inputs = convert(&decl.inputs);
        let outputs = convert(&decl.outputs);
        Self {
            name: decl.name.name.clone(),
            inputs,
            outputs,
            doc: decl.doc.clone(),
            origin,
            name_span: Some(decl.name.span),
        }
    }

    pub fn input(&self, name: &str) -> Option<(usize, &PinInfo)> {
        self.inputs.iter().enumerate().find(|(_, p)| p.name == name)
    }

    pub fn output(&self, name: &str) -> Option<(usize, &PinInfo)> {
        self.outputs
            .iter()
            .enumerate()
            .find(|(_, p)| p.name == name)
    }

    /// Looks up a pin by exact name, preferring inputs.
    pub fn pin(&self, name: &str) -> Option<PinLookup<'_>> {
        if let Some((index, info)) = self.input(name) {
            return Some(PinLookup {
                direction: PinDirection::Input,
                index,
                info,
            });
        }
        self.output(name).map(|(index, info)| PinLookup {
            direction: PinDirection::Output,
            index,
            info,
        })
    }

    pub fn pins(&self, direction: PinDirection) -> &[PinInfo] {
        match direction {
            PinDirection::Input => &self.inputs,
            PinDirection::Output => &self.outputs,
        }
    }

    pub fn pin_by_index(&self, direction: PinDirection, index: usize) -> Option<&PinInfo> {
        self.pins(direction).get(index)
    }

    /// Whether two interfaces declare the same pins (names, widths and
    /// directions, in order). Spans, docs and origin are ignored.
    pub fn same_signature(&self, other: &ChipInterface) -> bool {
        fn eq(a: &[PinInfo], b: &[PinInfo]) -> bool {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(x, y)| x.name == y.name && x.width == y.width)
        }
        eq(&self.inputs, &other.inputs) && eq(&self.outputs, &other.outputs)
    }

    /// A compact human-readable signature, e.g. `Mux16(a[16], b[16], sel) -> (out[16])`.
    pub fn signature(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for PinInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.width == 1 {
            f.write_str(&self.name)
        } else {
            write!(f, "{}[{}]", self.name, self.width)
        }
    }
}

impl fmt::Display for ChipInterface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let join = |pins: &[PinInfo]| {
            pins.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        write!(
            f,
            "{}({}) -> ({})",
            self.name,
            join(&self.inputs),
            join(&self.outputs)
        )
    }
}
