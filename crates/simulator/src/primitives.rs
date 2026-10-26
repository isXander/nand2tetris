//! Native implementations of the built-in chips.
//!
//! A primitive receives pin values in interface order (as declared by the
//! built-in HDL interface) and must not share state with other instances.

use std::collections::HashMap;

/// A built-in chip implementation.
///
/// `eval` computes outputs from the current inputs and *committed* state.
/// `tick` captures the next state from the settled inputs without changing
/// any output; `tock` publishes the captured state.
pub trait Primitive: Send {
    fn eval(&mut self, inputs: &[u64], outputs: &mut [u64]);

    fn tick(&mut self, _inputs: &[u64]) {}

    fn tock(&mut self) {}

    /// Reads script-visible state (`Name[]` uses index 0).
    fn read_state(&self, _index: usize) -> Option<u64> {
        None
    }

    /// Debug write of script-visible state. Returns whether it is supported.
    fn write_state(&mut self, _index: usize, _value: u64) -> bool {
        false
    }

    /// Replaces program memory (ROM). Returns whether it is supported.
    fn load_program(&mut self, _words: &[u16]) -> bool {
        false
    }

    /// Host keyboard input (Keyboard and Memory).
    fn set_key(&mut self, _key: u16) {}
}

pub type Factory = fn() -> Box<dyn Primitive>;

/// Maps built-in implementation symbols to factories.
#[derive(Clone)]
pub struct BuiltinRegistry {
    factories: HashMap<String, Factory>,
}

impl Default for BuiltinRegistry {
    fn default() -> Self {
        Self::standard()
    }
}

impl BuiltinRegistry {
    pub fn empty() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    /// Implementations for every chip in [`n2t_core::builtins::BuiltinLibrary::standard`].
    pub fn standard() -> Self {
        let mut r = Self::empty();
        r.register("Nand", || comb(|i, o| o[0] = !(i[0] & i[1]) & 1));
        r.register("Not", || comb(|i, o| o[0] = !i[0] & 1));
        r.register("And", || comb(|i, o| o[0] = i[0] & i[1]));
        r.register("Or", || comb(|i, o| o[0] = i[0] | i[1]));
        r.register("Xor", || comb(|i, o| o[0] = i[0] ^ i[1]));
        r.register("Mux", || {
            comb(|i, o| o[0] = if i[2] == 0 { i[0] } else { i[1] })
        });
        r.register("DMux", || {
            comb(|i, o| {
                o[0] = if i[1] == 0 { i[0] } else { 0 };
                o[1] = if i[1] == 1 { i[0] } else { 0 };
            })
        });
        r.register("Not16", || comb(|i, o| o[0] = !i[0] & 0xFFFF));
        r.register("And16", || comb(|i, o| o[0] = i[0] & i[1]));
        r.register("Or16", || comb(|i, o| o[0] = i[0] | i[1]));
        r.register("Mux16", || {
            comb(|i, o| o[0] = if i[2] == 0 { i[0] } else { i[1] })
        });
        r.register("Or8Way", || comb(|i, o| o[0] = u64::from(i[0] & 0xFF != 0)));
        r.register("Mux4Way16", || comb(|i, o| o[0] = i[(i[4] & 3) as usize]));
        r.register("Mux8Way16", || comb(|i, o| o[0] = i[(i[8] & 7) as usize]));
        r.register("DMux4Way", || {
            comb(|i, o| {
                o.iter_mut().for_each(|v| *v = 0);
                o[(i[1] & 3) as usize] = i[0];
            })
        });
        r.register("DMux8Way", || {
            comb(|i, o| {
                o.iter_mut().for_each(|v| *v = 0);
                o[(i[1] & 7) as usize] = i[0];
            })
        });
        r.register("HalfAdder", || {
            comb(|i, o| {
                o[0] = i[0] ^ i[1];
                o[1] = i[0] & i[1];
            })
        });
        r.register("FullAdder", || {
            comb(|i, o| {
                let sum = i[0] + i[1] + i[2];
                o[0] = sum & 1;
                o[1] = sum >> 1;
            })
        });
        r.register("Add16", || comb(|i, o| o[0] = (i[0] + i[1]) & 0xFFFF));
        r.register("Inc16", || comb(|i, o| o[0] = (i[0] + 1) & 0xFFFF));
        r.register("ALU", || comb(alu));

        r.register("DFF", || Box::new(Dff::default()));
        r.register("Bit", || Box::new(Reg::new(1)));
        r.register("Register", || Box::new(Reg::new(16)));
        r.register("ARegister", || Box::new(Reg::immediate(16)));
        r.register("DRegister", || Box::new(Reg::immediate(16)));
        r.register("PC", || Box::new(Pc::default()));
        r.register("RAM8", || Box::new(Ram::new(8)));
        r.register("RAM64", || Box::new(Ram::new(64)));
        r.register("RAM512", || Box::new(Ram::new(512)));
        r.register("RAM4K", || Box::new(Ram::new(4096)));
        r.register("RAM16K", || Box::new(Ram::new(16384)));
        r.register("Screen", || Box::new(Ram::new(8192)));
        r.register("ROM32K", || Box::new(Rom::default()));
        r.register("Keyboard", || Box::new(Keyboard::default()));
        r.register("Memory", || Box::new(Memory::default()));
        r
    }

    pub fn register(&mut self, symbol: &str, factory: Factory) {
        self.factories.insert(symbol.to_string(), factory);
    }

    pub fn create(&self, symbol: &str) -> Option<Box<dyn Primitive>> {
        self.factories.get(symbol).map(|f| f())
    }

    pub fn contains(&self, symbol: &str) -> bool {
        self.factories.contains_key(symbol)
    }

    pub fn symbols(&self) -> impl Iterator<Item = &str> {
        self.factories.keys().map(String::as_str)
    }
}

// -------------------------------------------------------------------------
// Combinational
// -------------------------------------------------------------------------

struct Comb(fn(&[u64], &mut [u64]));

impl Primitive for Comb {
    fn eval(&mut self, inputs: &[u64], outputs: &mut [u64]) {
        (self.0)(inputs, outputs)
    }
}

fn comb(f: fn(&[u64], &mut [u64])) -> Box<dyn Primitive> {
    Box::new(Comb(f))
}

/// The Hack ALU: inputs x, y, zx, nx, zy, ny, f, no; outputs out, zr, ng.
fn alu(i: &[u64], o: &mut [u64]) {
    let mut x = i[0] & 0xFFFF;
    let mut y = i[1] & 0xFFFF;
    if i[2] != 0 {
        x = 0;
    }
    if i[3] != 0 {
        x = !x & 0xFFFF;
    }
    if i[4] != 0 {
        y = 0;
    }
    if i[5] != 0 {
        y = !y & 0xFFFF;
    }
    let mut out = if i[6] != 0 { (x + y) & 0xFFFF } else { x & y };
    if i[7] != 0 {
        out = !out & 0xFFFF;
    }
    o[0] = out;
    o[1] = u64::from(out == 0);
    o[2] = (out >> 15) & 1;
}

// -------------------------------------------------------------------------
// Sequential
// -------------------------------------------------------------------------

#[derive(Default)]
struct Dff {
    state: u64,
    pending: Option<u64>,
}

impl Primitive for Dff {
    fn eval(&mut self, _inputs: &[u64], outputs: &mut [u64]) {
        outputs[0] = self.state;
    }

    fn tick(&mut self, inputs: &[u64]) {
        self.pending = Some(inputs[0]);
    }

    fn tock(&mut self) {
        if let Some(p) = self.pending.take() {
            self.state = p;
        }
    }
}

/// Bit / Register / ARegister / DRegister: inputs in, load.
struct Reg {
    mask: u64,
    state: u64,
    pending: Option<u64>,
    /// Whether debug writes are immediate (ARegister, DRegister) or pending
    /// until the next tock (Register).
    immediate: bool,
}

impl Reg {
    fn new(width: u32) -> Self {
        Self {
            mask: (1 << width) - 1,
            state: 0,
            pending: None,
            immediate: false,
        }
    }

    fn immediate(width: u32) -> Self {
        Self {
            immediate: true,
            ..Self::new(width)
        }
    }
}

impl Primitive for Reg {
    fn eval(&mut self, _inputs: &[u64], outputs: &mut [u64]) {
        outputs[0] = self.state;
    }

    fn tick(&mut self, inputs: &[u64]) {
        if inputs[1] != 0 {
            self.pending = Some(inputs[0] & self.mask);
        }
    }

    fn tock(&mut self) {
        if let Some(p) = self.pending.take() {
            self.state = p;
        }
    }

    fn read_state(&self, index: usize) -> Option<u64> {
        (index == 0).then_some(self.state)
    }

    fn write_state(&mut self, index: usize, value: u64) -> bool {
        if index != 0 {
            return false;
        }
        if self.immediate {
            self.state = value & self.mask;
            self.pending = None;
        } else {
            self.pending = Some(value & self.mask);
        }
        true
    }
}

/// PC: inputs in, load, inc, reset.
#[derive(Default)]
struct Pc {
    state: u64,
    pending: Option<u64>,
}

impl Primitive for Pc {
    fn eval(&mut self, _inputs: &[u64], outputs: &mut [u64]) {
        outputs[0] = self.state;
    }

    fn tick(&mut self, i: &[u64]) {
        let next = if i[3] != 0 {
            0
        } else if i[1] != 0 {
            i[0]
        } else if i[2] != 0 {
            (self.state + 1) & 0xFFFF
        } else {
            self.state
        };
        self.pending = Some(next);
    }

    fn tock(&mut self) {
        if let Some(p) = self.pending.take() {
            self.state = p;
        }
    }

    fn read_state(&self, index: usize) -> Option<u64> {
        (index == 0).then_some(self.state)
    }

    fn write_state(&mut self, index: usize, value: u64) -> bool {
        if index != 0 {
            return false;
        }
        self.state = value & 0xFFFF;
        self.pending = None;
        true
    }
}

/// RAMn and Screen: inputs in, load, address. Reads use committed storage.
struct Ram {
    words: Vec<u16>,
    pending: Option<(usize, u16)>,
}

impl Ram {
    fn new(len: usize) -> Self {
        Self {
            words: vec![0; len],
            pending: None,
        }
    }
}

impl Primitive for Ram {
    fn eval(&mut self, inputs: &[u64], outputs: &mut [u64]) {
        outputs[0] = self.words.get(inputs[2] as usize).copied().unwrap_or(0) as u64;
    }

    fn tick(&mut self, inputs: &[u64]) {
        if inputs[1] != 0 {
            self.pending = Some((inputs[2] as usize, inputs[0] as u16));
        }
    }

    fn tock(&mut self) {
        if let Some((address, value)) = self.pending.take()
            && let Some(word) = self.words.get_mut(address)
        {
            *word = value;
        }
    }

    fn read_state(&self, index: usize) -> Option<u64> {
        self.words.get(index).map(|&w| w as u64)
    }

    fn write_state(&mut self, index: usize, value: u64) -> bool {
        match self.words.get_mut(index) {
            Some(word) => {
                *word = value as u16;
                true
            }
            None => false,
        }
    }
}

/// ROM32K: input address; read-only from scripts, loaded by method.
struct Rom {
    words: Vec<u16>,
}

impl Default for Rom {
    fn default() -> Self {
        Self {
            words: vec![0; 32768],
        }
    }
}

impl Primitive for Rom {
    fn eval(&mut self, inputs: &[u64], outputs: &mut [u64]) {
        outputs[0] = self.words.get(inputs[0] as usize).copied().unwrap_or(0) as u64;
    }

    fn read_state(&self, index: usize) -> Option<u64> {
        self.words.get(index).map(|&w| w as u64)
    }

    fn load_program(&mut self, words: &[u16]) -> bool {
        if words.len() > self.words.len() {
            return false;
        }
        self.words.iter_mut().for_each(|w| *w = 0);
        self.words[..words.len()].copy_from_slice(words);
        true
    }
}

#[derive(Default)]
struct Keyboard {
    key: u16,
}

impl Primitive for Keyboard {
    fn eval(&mut self, _inputs: &[u64], outputs: &mut [u64]) {
        outputs[0] = self.key as u64;
    }

    fn read_state(&self, index: usize) -> Option<u64> {
        (index == 0).then_some(self.key as u64)
    }

    fn set_key(&mut self, key: u16) {
        self.key = key;
    }
}

/// Memory: inputs in, load, address[15]. RAM16K at 0..16383, Screen at
/// 16384..24575, Keyboard at 24576.
struct Memory {
    ram: Vec<u16>,
    screen: Vec<u16>,
    key: u16,
    pending: Option<(usize, u16)>,
}

impl Default for Memory {
    fn default() -> Self {
        Self {
            ram: vec![0; 16384],
            screen: vec![0; 8192],
            key: 0,
            pending: None,
        }
    }
}

impl Memory {
    fn cell(&mut self, address: usize) -> Option<&mut u16> {
        match address {
            0..16384 => self.ram.get_mut(address),
            16384..24576 => self.screen.get_mut(address - 16384),
            _ => None,
        }
    }
}

impl Primitive for Memory {
    fn eval(&mut self, inputs: &[u64], outputs: &mut [u64]) {
        let address = inputs[2] as usize;
        outputs[0] = match address {
            24576 => self.key as u64,
            _ => self.cell(address).map_or(0, |w| *w as u64),
        };
    }

    fn tick(&mut self, inputs: &[u64]) {
        if inputs[1] != 0 {
            self.pending = Some((inputs[2] as usize, inputs[0] as u16));
        }
    }

    fn tock(&mut self) {
        if let Some((address, value)) = self.pending.take()
            && let Some(cell) = self.cell(address)
        {
            *cell = value;
        }
    }

    fn set_key(&mut self, key: u16) {
        self.key = key;
    }
}
