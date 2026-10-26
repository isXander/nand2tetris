//! The built-in chip *interface* library.
//!
//! This module describes built-in chips: their HDL interfaces, clock
//! sensitivity and script-visible state/method APIs. It does not contain any
//! executable behaviour; simulators resolve [`BuiltinChip::symbol`] in their
//! own registry of implementations.

use std::collections::HashMap;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, has_errors};
use crate::hdl::ParsedHdl;
use crate::hdl::ast::ChipImpl;
use crate::hdl::interface::{ChipInterface, ChipOrigin};

/// How script-visible state of a built-in may be accessed (`Name[]` / `Name[i]`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptState {
    pub shape: StateShape,
    /// Width of each state word.
    pub width: u32,
    pub write: WritePolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateShape {
    /// `Name[]`
    Scalar,
    /// `Name[i]` with `0 <= i < len`.
    Indexed { len: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritePolicy {
    /// `set` is rejected.
    ReadOnly,
    /// The write takes effect immediately and the parent is re-evaluated.
    Immediate,
    /// The write becomes a pending state, published by the next `tock`.
    NextTock,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinMethod {
    pub name: String,
    pub parameters: Vec<MethodParameter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodParameter {
    /// A companion file basename with one of the given extensions (without dot).
    FileName { extensions: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct BuiltinChip {
    /// The HDL chip name (`CHIP Name`).
    pub name: String,
    /// The implementation symbol (`BUILTIN Symbol;`).
    pub symbol: String,
    /// HDL text of the interface, usable as a virtual document by editors.
    pub source: Arc<str>,
    pub parsed: Arc<ParsedHdl>,
    pub interface: Arc<ChipInterface>,
    /// Clocked pin names; `None` when the chip has no `CLOCKED` clause.
    pub clocked: Option<Vec<String>>,
    pub script_state: Option<ScriptState>,
    pub methods: Vec<BuiltinMethod>,
}

impl BuiltinChip {
    /// Builds a descriptor from HDL interface text containing a `BUILTIN`
    /// implementation.
    pub fn from_hdl(
        source: &str,
        script_state: Option<ScriptState>,
        methods: Vec<BuiltinMethod>,
    ) -> Result<Self, Vec<Diagnostic>> {
        let parsed = crate::hdl::parse(source);
        if has_errors(&parsed.diagnostics) {
            return Err(parsed.diagnostics);
        }
        let decl = parsed.chip.as_ref().expect("no errors implies a chip");
        let Some(ChipImpl::Builtin { name, clocked, .. }) = &decl.implementation else {
            return Err(vec![Diagnostic::new(
                crate::diagnostic::Severity::Error,
                crate::diagnostic::DiagnosticCode::MissingImplementation,
                "built-in chip descriptors must use a `BUILTIN` implementation",
                Some(decl.span),
            )]);
        };
        let interface = Arc::new(ChipInterface::from_decl(decl, ChipOrigin::Builtin));
        Ok(Self {
            name: decl.name.name.clone(),
            symbol: name.name.clone(),
            clocked: clocked
                .as_ref()
                .map(|pins| pins.iter().map(|p| p.name.clone()).collect()),
            source: Arc::from(source),
            interface,
            parsed: Arc::new(parsed),
            script_state,
            methods,
        })
    }

    pub fn method(&self, name: &str) -> Option<&BuiltinMethod> {
        self.methods
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }
}

/// A set of built-in chip descriptors, keyed by chip name and by
/// implementation symbol.
#[derive(Debug, Clone, Default)]
pub struct BuiltinLibrary {
    chips: Vec<Arc<BuiltinChip>>,
    by_name: HashMap<String, usize>,
    by_symbol: HashMap<String, usize>,
}

impl BuiltinLibrary {
    pub fn empty() -> Self {
        Self::default()
    }

    /// The standard Hack chip set supplied with the Nand to Tetris tools
    /// (every project 1–5 chip except `CPU` and `Computer`).
    pub fn standard() -> Self {
        let mut library = Self::empty();
        for (source, state, methods) in standard_chips() {
            let chip = BuiltinChip::from_hdl(source, state, methods).unwrap_or_else(|e| {
                panic!("invalid standard built-in descriptor: {e:?}\n{source}")
            });
            library.register(chip);
        }
        library
    }

    /// Adds (or replaces) a descriptor.
    pub fn register(&mut self, chip: BuiltinChip) {
        let chip = Arc::new(chip);
        let index = match self.by_name.get(&chip.name) {
            Some(&i) => {
                let old = std::mem::replace(&mut self.chips[i], chip.clone());
                self.by_symbol.remove(&old.symbol);
                i
            }
            None => {
                self.chips.push(chip.clone());
                self.chips.len() - 1
            }
        };
        self.by_name.insert(chip.name.clone(), index);
        self.by_symbol.insert(chip.symbol.clone(), index);
    }

    /// Looks up a built-in by HDL chip name.
    pub fn get(&self, name: &str) -> Option<&Arc<BuiltinChip>> {
        self.by_name.get(name).map(|&i| &self.chips[i])
    }

    /// Looks up a built-in by implementation symbol.
    pub fn by_symbol(&self, symbol: &str) -> Option<&Arc<BuiltinChip>> {
        self.by_symbol.get(symbol).map(|&i| &self.chips[i])
    }

    pub fn iter(&self) -> impl Iterator<Item = &Arc<BuiltinChip>> {
        self.chips.iter()
    }

    pub fn len(&self) -> usize {
        self.chips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chips.is_empty()
    }
}

fn scalar(write: WritePolicy) -> Option<ScriptState> {
    Some(ScriptState {
        shape: StateShape::Scalar,
        width: 16,
        write,
    })
}

fn memory(len: u32, write: WritePolicy) -> Option<ScriptState> {
    Some(ScriptState {
        shape: StateShape::Indexed { len },
        width: 16,
        write,
    })
}

type StandardChip = (&'static str, Option<ScriptState>, Vec<BuiltinMethod>);

fn standard_chips() -> Vec<StandardChip> {
    use WritePolicy::*;
    let rom_load = BuiltinMethod {
        name: "load".into(),
        parameters: vec![MethodParameter::FileName {
            extensions: vec!["hack".into(), "asm".into()],
        }],
    };
    vec![
        // ---- Project 1: Boolean logic ----
        (
            "/** Nand gate: out = not (a and b) */\nCHIP Nand {\n    IN a, b;\n    OUT out;\n    BUILTIN Nand;\n}\n",
            None,
            vec![],
        ),
        (
            "/** Not gate: out = not in */\nCHIP Not {\n    IN in;\n    OUT out;\n    BUILTIN Not;\n}\n",
            None,
            vec![],
        ),
        (
            "/** And gate: out = a and b */\nCHIP And {\n    IN a, b;\n    OUT out;\n    BUILTIN And;\n}\n",
            None,
            vec![],
        ),
        (
            "/** Or gate: out = a or b */\nCHIP Or {\n    IN a, b;\n    OUT out;\n    BUILTIN Or;\n}\n",
            None,
            vec![],
        ),
        (
            "/** Exclusive-or gate: out = (a != b) */\nCHIP Xor {\n    IN a, b;\n    OUT out;\n    BUILTIN Xor;\n}\n",
            None,
            vec![],
        ),
        (
            "/** Multiplexor: out = a if sel == 0, b otherwise */\nCHIP Mux {\n    IN a, b, sel;\n    OUT out;\n    BUILTIN Mux;\n}\n",
            None,
            vec![],
        ),
        (
            "/** Demultiplexor: [a, b] = [in, 0] if sel == 0, [0, in] otherwise */\nCHIP DMux {\n    IN in, sel;\n    OUT a, b;\n    BUILTIN DMux;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 16-bit Not: for i = 0..15: out[i] = not in[i] */\nCHIP Not16 {\n    IN in[16];\n    OUT out[16];\n    BUILTIN Not16;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 16-bit And: for i = 0..15: out[i] = a[i] and b[i] */\nCHIP And16 {\n    IN a[16], b[16];\n    OUT out[16];\n    BUILTIN And16;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 16-bit Or: for i = 0..15: out[i] = a[i] or b[i] */\nCHIP Or16 {\n    IN a[16], b[16];\n    OUT out[16];\n    BUILTIN Or16;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 16-bit multiplexor: out = a if sel == 0, b otherwise */\nCHIP Mux16 {\n    IN a[16], b[16], sel;\n    OUT out[16];\n    BUILTIN Mux16;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 8-way Or gate: out = in[0] or in[1] or ... or in[7] */\nCHIP Or8Way {\n    IN in[8];\n    OUT out;\n    BUILTIN Or8Way;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 4-way 16-bit multiplexor: out = a, b, c or d for sel = 00, 01, 10, 11 */\nCHIP Mux4Way16 {\n    IN a[16], b[16], c[16], d[16], sel[2];\n    OUT out[16];\n    BUILTIN Mux4Way16;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 8-way 16-bit multiplexor: out = a, b, ..., h for sel = 000, 001, ..., 111 */\nCHIP Mux8Way16 {\n    IN a[16], b[16], c[16], d[16], e[16], f[16], g[16], h[16], sel[3];\n    OUT out[16];\n    BUILTIN Mux8Way16;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 4-way demultiplexor: routes in to a, b, c or d for sel = 00, 01, 10, 11 */\nCHIP DMux4Way {\n    IN in, sel[2];\n    OUT a, b, c, d;\n    BUILTIN DMux4Way;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 8-way demultiplexor: routes in to a, b, ..., h for sel = 000, ..., 111 */\nCHIP DMux8Way {\n    IN in, sel[3];\n    OUT a, b, c, d, e, f, g, h;\n    BUILTIN DMux8Way;\n}\n",
            None,
            vec![],
        ),
        // ---- Project 2: Boolean arithmetic ----
        (
            "/** Computes the sum of two bits. */\nCHIP HalfAdder {\n    IN a, b;\n    OUT sum, carry;\n    BUILTIN HalfAdder;\n}\n",
            None,
            vec![],
        ),
        (
            "/** Computes the sum of three bits. */\nCHIP FullAdder {\n    IN a, b, c;\n    OUT sum, carry;\n    BUILTIN FullAdder;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 16-bit adder: adds two 16-bit two's complement values; overflow is ignored. */\nCHIP Add16 {\n    IN a[16], b[16];\n    OUT out[16];\n    BUILTIN Add16;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 16-bit incrementer: out = in + 1 */\nCHIP Inc16 {\n    IN in[16];\n    OUT out[16];\n    BUILTIN Inc16;\n}\n",
            None,
            vec![],
        ),
        (
            "/**\n * The Hack ALU. Computes one of several functions of x and y according\n * to the control bits zx, nx, zy, ny, f and no; zr is 1 when out == 0 and\n * ng is 1 when out < 0.\n */\nCHIP ALU {\n    IN x[16], y[16], zx, nx, zy, ny, f, no;\n    OUT out[16], zr, ng;\n    BUILTIN ALU;\n}\n",
            None,
            vec![],
        ),
        // ---- Project 3: Memory ----
        (
            "/** Data flip-flop: out(t) = in(t-1) */\nCHIP DFF {\n    IN in;\n    OUT out;\n    BUILTIN DFF;\n    CLOCKED in;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 1-bit register: if load(t) then out(t+1) = in(t) else out(t+1) = out(t) */\nCHIP Bit {\n    IN in, load;\n    OUT out;\n    BUILTIN Bit;\n    CLOCKED in, load;\n}\n",
            None,
            vec![],
        ),
        (
            "/** 16-bit register: if load(t) then out(t+1) = in(t) else out(t+1) = out(t) */\nCHIP Register {\n    IN in[16], load;\n    OUT out[16];\n    BUILTIN Register;\n    CLOCKED in, load;\n}\n",
            scalar(NextTock),
            vec![],
        ),
        (
            "/** The CPU's A register; identical in behaviour to Register. */\nCHIP ARegister {\n    IN in[16], load;\n    OUT out[16];\n    BUILTIN ARegister;\n    CLOCKED in, load;\n}\n",
            scalar(Immediate),
            vec![],
        ),
        (
            "/** The CPU's D register; identical in behaviour to Register. */\nCHIP DRegister {\n    IN in[16], load;\n    OUT out[16];\n    BUILTIN DRegister;\n    CLOCKED in, load;\n}\n",
            scalar(Immediate),
            vec![],
        ),
        (
            "/**\n * 16-bit program counter:\n * if reset(t) out(t+1) = 0, else if load(t) out(t+1) = in(t),\n * else if inc(t) out(t+1) = out(t) + 1, else out(t+1) = out(t).\n */\nCHIP PC {\n    IN in[16], load, inc, reset;\n    OUT out[16];\n    BUILTIN PC;\n    CLOCKED in, load, inc, reset;\n}\n",
            scalar(Immediate),
            vec![],
        ),
        (
            "/** Memory of 8 16-bit registers. Reads are combinational; writes are clocked. */\nCHIP RAM8 {\n    IN in[16], load, address[3];\n    OUT out[16];\n    BUILTIN RAM8;\n    CLOCKED in, load;\n}\n",
            memory(8, Immediate),
            vec![],
        ),
        (
            "/** Memory of 64 16-bit registers. Reads are combinational; writes are clocked. */\nCHIP RAM64 {\n    IN in[16], load, address[6];\n    OUT out[16];\n    BUILTIN RAM64;\n    CLOCKED in, load;\n}\n",
            memory(64, Immediate),
            vec![],
        ),
        (
            "/** Memory of 512 16-bit registers. Reads are combinational; writes are clocked. */\nCHIP RAM512 {\n    IN in[16], load, address[9];\n    OUT out[16];\n    BUILTIN RAM512;\n    CLOCKED in, load;\n}\n",
            memory(512, Immediate),
            vec![],
        ),
        (
            "/** Memory of 4K 16-bit registers. Reads are combinational; writes are clocked. */\nCHIP RAM4K {\n    IN in[16], load, address[12];\n    OUT out[16];\n    BUILTIN RAM4K;\n    CLOCKED in, load;\n}\n",
            memory(4096, Immediate),
            vec![],
        ),
        (
            "/** Memory of 16K 16-bit registers. Reads are combinational; writes are clocked. */\nCHIP RAM16K {\n    IN in[16], load, address[14];\n    OUT out[16];\n    BUILTIN RAM16K;\n    CLOCKED in, load;\n}\n",
            memory(16384, Immediate),
            vec![],
        ),
        // ---- Project 5: Computer architecture ----
        (
            "/** Read-only memory of 32K 16-bit instructions. out = ROM32K[address] */\nCHIP ROM32K {\n    IN address[15];\n    OUT out[16];\n    BUILTIN ROM32K;\n}\n",
            memory(32768, ReadOnly),
            vec![rom_load],
        ),
        (
            "/**\n * The Hack screen: 8K 16-bit words memory-mapping a 256 x 512 black and\n * white display. Reads are combinational; writes are clocked.\n */\nCHIP Screen {\n    IN in[16], load, address[13];\n    OUT out[16];\n    BUILTIN Screen;\n    CLOCKED in, load;\n}\n",
            memory(8192, Immediate),
            vec![],
        ),
        (
            "/** The Hack keyboard: out is the scan code of the currently pressed key, or 0. */\nCHIP Keyboard {\n    OUT out[16];\n    BUILTIN Keyboard;\n}\n",
            scalar(ReadOnly),
            vec![],
        ),
        (
            "/**\n * The Hack data memory: RAM16K at 0..16383, Screen at 16384..24575 and\n * Keyboard at 24576. Reads are combinational; writes are clocked.\n */\nCHIP Memory {\n    IN in[16], load, address[15];\n    OUT out[16];\n    BUILTIN Memory;\n    CLOCKED in, load;\n}\n",
            None,
            vec![],
        ),
    ]
}
