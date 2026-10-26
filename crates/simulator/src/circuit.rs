//! A runnable circuit instantiated from a [`ChipDefinition`].
//!
//! The definition hierarchy is flattened: every bit of every net and pin is
//! merged with the bits it is wired to (union-find), producing one shared
//! *wire* per electrically connected group. Only built-in primitives remain;
//! they are evaluated in dependency order. Constants are two reserved wires.

use std::fmt;
use std::sync::Arc;

use n2t_core::hdl::definition::{ChipDefinition, Implementation, InputSource, NetKind};

use crate::primitives::{BuiltinRegistry, Primitive};

const FALSE_WIRE: u32 = 0;
const TRUE_WIRE: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimError {
    /// A built-in implementation symbol has no native implementation.
    UnsupportedBuiltin { chip: String, symbol: String },
    /// `tick` twice, or `tock` before `tick`.
    Phase(String),
    /// The circuit is too large to instantiate.
    TooLarge,
}

impl fmt::Display for SimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SimError::UnsupportedBuiltin { chip, symbol } => write!(
                f,
                "chip `{chip}` uses built-in implementation `{symbol}`, which this simulator does not provide"
            ),
            SimError::Phase(message) => f.write_str(message),
            SimError::TooLarge => f.write_str("the circuit is too large to simulate"),
        }
    }
}

impl std::error::Error for SimError {}

/// Simulated time: a cycle counter plus the clock phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Time {
    pub cycle: u64,
    /// `true` between `tick` and `tock`.
    pub after_tick: bool,
}

impl fmt::Display for Time {
    /// `0`, `0+`, `1`, …
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            self.cycle,
            if self.after_tick { "+" } else { "" }
        )
    }
}

/// One built-in primitive instance in the flattened circuit.
pub struct PrimitiveInstance {
    /// The HDL chip name of the instance (e.g. `RAM16K`).
    pub chip_name: String,
    pub symbol: String,
    /// Part indices from the root chip down to this instance.
    pub path: Vec<usize>,
    pub sequential: bool,
    implementation: Box<dyn Primitive>,
    /// Input pins on which some output depends combinationally.
    combinational: Vec<usize>,
    inputs: Vec<Vec<u32>>,
    outputs: Vec<Vec<u32>>,
}

/// A readable (and possibly writable) group of wires of the root chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinHandle {
    pub name: String,
    pub kind: NetKind,
    pub width: u32,
    wires: Vec<u32>,
}

pub struct Circuit {
    definition: Arc<ChipDefinition>,
    values: Vec<bool>,
    primitives: Vec<PrimitiveInstance>,
    order: Vec<usize>,
    /// Whether some primitives could not be ordered (only possible with
    /// unusual custom built-ins); evaluation then iterates to a fixed point.
    cyclic: bool,
    pins: Vec<PinHandle>,
    time: Time,
    scratch_in: Vec<u64>,
    scratch_out: Vec<u64>,
}

/// Upper bound on flattened bits, to fail cleanly rather than exhaust memory.
const MAX_BITS: usize = 50_000_000;

/// Bit ids of an instance's input pins, output pins and nets.
type InstanceBits = (Vec<Vec<u32>>, Vec<Vec<u32>>, Vec<Vec<u32>>);

struct Builder<'r> {
    registry: &'r BuiltinRegistry,
    parent: Vec<u32>,
    primitives: Vec<PrimitiveInstance>,
}

impl Builder<'_> {
    fn alloc(&mut self, width: u32) -> Result<Vec<u32>, SimError> {
        let start = self.parent.len();
        if start + width as usize > MAX_BITS {
            return Err(SimError::TooLarge);
        }
        let ids: Vec<u32> = (start as u32..start as u32 + width).collect();
        self.parent.extend(ids.iter().copied());
        Ok(ids)
    }

    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let p = self.parent[x as usize];
            self.parent[x as usize] = self.parent[p as usize];
            x = p;
        }
        x
    }

    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            // Keep constants as roots so that they stay recognisable.
            if rb <= TRUE_WIRE {
                self.parent[ra as usize] = rb;
            } else {
                self.parent[rb as usize] = ra;
            }
        }
    }

    /// Instantiates `def`, returning the bit ids of its input and output pins
    /// and (for the root) of every net.
    fn instantiate(
        &mut self,
        def: &ChipDefinition,
        path: &mut Vec<usize>,
    ) -> Result<InstanceBits, SimError> {
        match &def.implementation {
            Implementation::Builtin(builtin) => {
                let implementation = self.registry.create(&builtin.symbol).ok_or_else(|| {
                    SimError::UnsupportedBuiltin {
                        chip: def.name.clone(),
                        symbol: builtin.symbol.clone(),
                    }
                })?;
                let inputs = def
                    .interface
                    .inputs
                    .iter()
                    .map(|p| self.alloc(p.width))
                    .collect::<Result<Vec<_>, _>>()?;
                let outputs = def
                    .interface
                    .outputs
                    .iter()
                    .map(|p| self.alloc(p.width))
                    .collect::<Result<Vec<_>, _>>()?;
                self.primitives.push(PrimitiveInstance {
                    chip_name: def.name.clone(),
                    symbol: builtin.symbol.clone(),
                    path: path.clone(),
                    sequential: def.sequential,
                    implementation,
                    combinational: {
                        let mut pins: Vec<usize> = def
                            .combinational_dependencies
                            .iter()
                            .flatten()
                            .copied()
                            .collect();
                        pins.sort_unstable();
                        pins.dedup();
                        pins
                    },
                    inputs: inputs.clone(),
                    outputs: outputs.clone(),
                });
                let nets = inputs.iter().chain(&outputs).cloned().collect();
                Ok((inputs, outputs, nets))
            }
            Implementation::Composite(composite) => {
                let nets = composite
                    .nets
                    .iter()
                    .map(|n| self.alloc(n.width))
                    .collect::<Result<Vec<_>, _>>()?;
                for (index, part) in composite.parts.iter().enumerate() {
                    path.push(index);
                    let (child_inputs, child_outputs, _) = self.instantiate(&part.chip, path)?;
                    path.pop();
                    for conn in &part.inputs {
                        for k in 0..conn.pin_bits.width {
                            let pin_bit =
                                child_inputs[conn.pin][(conn.pin_bits.start + k) as usize];
                            let source = match conn.source {
                                InputSource::Net { net, bits } => {
                                    nets[net][(bits.start + k) as usize]
                                }
                                InputSource::Constant(true) => TRUE_WIRE,
                                InputSource::Constant(false) => FALSE_WIRE,
                            };
                            self.union(pin_bit, source);
                        }
                    }
                    for conn in &part.outputs {
                        for k in 0..conn.pin_bits.width {
                            let pin_bit =
                                child_outputs[conn.pin][(conn.pin_bits.start + k) as usize];
                            let net_bit = nets[conn.net][(conn.net_bits.start + k) as usize];
                            self.union(pin_bit, net_bit);
                        }
                    }
                }
                let n_in = def.interface.inputs.len();
                let n_out = def.interface.outputs.len();
                let inputs = nets[..n_in].to_vec();
                let outputs = nets[n_in..n_in + n_out].to_vec();
                Ok((inputs, outputs, nets))
            }
        }
    }
}

impl Circuit {
    /// Builds a fresh circuit with all state and inputs zero, then settles it.
    pub fn new(
        definition: Arc<ChipDefinition>,
        registry: &BuiltinRegistry,
    ) -> Result<Self, SimError> {
        let mut builder = Builder {
            registry,
            parent: vec![FALSE_WIRE, TRUE_WIRE],
            primitives: Vec::new(),
        };
        let (_, _, nets) = builder.instantiate(&definition, &mut Vec::new())?;

        // Compress union-find roots into dense wire indices (constants first).
        let total = builder.parent.len();
        let mut wire_of_root = vec![u32::MAX; total];
        wire_of_root[FALSE_WIRE as usize] = FALSE_WIRE;
        wire_of_root[TRUE_WIRE as usize] = TRUE_WIRE;
        let mut next = 2u32;
        let mut wire = vec![0u32; total];
        for bit in 0..total as u32 {
            let root = builder.find(bit) as usize;
            if wire_of_root[root] == u32::MAX {
                wire_of_root[root] = next;
                next += 1;
            }
            wire[bit as usize] = wire_of_root[root];
        }
        let map = |bits: &Vec<u32>| bits.iter().map(|&b| wire[b as usize]).collect::<Vec<u32>>();

        let mut primitives = builder.primitives;
        for p in &mut primitives {
            p.inputs = p.inputs.iter().map(map).collect();
            p.outputs = p.outputs.iter().map(map).collect();
        }

        let pins = match &definition.implementation {
            Implementation::Composite(composite) => composite
                .nets
                .iter()
                .zip(&nets)
                .map(|(net, bits)| PinHandle {
                    name: net.name.clone(),
                    kind: net.kind,
                    width: net.width,
                    wires: map(bits),
                })
                .collect(),
            Implementation::Builtin(_) => {
                let interface = &definition.interface;
                let kinds = (0..interface.inputs.len())
                    .map(|i| (NetKind::Input(i), &interface.inputs[i]))
                    .chain(
                        (0..interface.outputs.len())
                            .map(|i| (NetKind::Output(i), &interface.outputs[i])),
                    );
                kinds
                    .zip(&nets)
                    .map(|((kind, p), bits)| PinHandle {
                        name: p.name.clone(),
                        kind,
                        width: p.width,
                        wires: map(bits),
                    })
                    .collect()
            }
        };

        let (order, cyclic) = schedule(&primitives, next as usize);
        let max_pins = primitives
            .iter()
            .map(|p| p.inputs.len().max(p.outputs.len()))
            .max()
            .unwrap_or(0);
        let mut values = vec![false; next as usize];
        values[TRUE_WIRE as usize] = true;
        let mut circuit = Self {
            definition,
            values,
            primitives,
            order,
            cyclic,
            pins,
            time: Time::default(),
            scratch_in: vec![0; max_pins],
            scratch_out: vec![0; max_pins],
        };
        circuit.eval();
        Ok(circuit)
    }

    pub fn definition(&self) -> &Arc<ChipDefinition> {
        &self.definition
    }

    pub fn time(&self) -> Time {
        self.time
    }

    /// Every top-level pin and internal net of the root chip.
    pub fn pins(&self) -> &[PinHandle] {
        &self.pins
    }

    pub fn pin(&self, name: &str) -> Option<&PinHandle> {
        self.pins.iter().find(|p| p.name == name)
    }

    pub fn input(&self, index: usize) -> Option<&PinHandle> {
        self.pins.iter().find(|p| p.kind == NetKind::Input(index))
    }

    pub fn output(&self, index: usize) -> Option<&PinHandle> {
        self.pins.iter().find(|p| p.kind == NetKind::Output(index))
    }

    /// Reads a pin as an unsigned bit pattern.
    pub fn read(&self, pin: &PinHandle) -> u64 {
        pin.wires.iter().enumerate().fold(0, |acc, (i, &w)| {
            acc | (u64::from(self.values[w as usize]) << i)
        })
    }

    /// Writes a top-level input pin. Calculated pins are not re-evaluated
    /// until [`Circuit::eval`] or a clock action.
    pub fn write(&mut self, pin: &PinHandle, value: u64) {
        debug_assert!(matches!(pin.kind, NetKind::Input(_)));
        for (i, &w) in pin.wires.iter().enumerate() {
            if w > TRUE_WIRE {
                self.values[w as usize] = (value >> i) & 1 == 1;
            }
        }
    }

    /// Convenience: reads a pin by name.
    pub fn get(&self, name: &str) -> Option<u64> {
        self.pin(name).map(|p| self.read(p))
    }

    /// Convenience: writes an input pin by name. Returns false if there is no
    /// such input.
    pub fn set(&mut self, name: &str, value: u64) -> bool {
        match self.pin(name).cloned() {
            Some(pin) if matches!(pin.kind, NetKind::Input(_)) => {
                self.write(&pin, value);
                true
            }
            _ => false,
        }
    }

    /// Settles combinational logic.
    pub fn eval(&mut self) {
        let rounds = if self.cyclic { 64 } else { 1 };
        for _ in 0..rounds {
            let mut changed = false;
            for i in 0..self.order.len() {
                changed |= self.eval_primitive(self.order[i]);
            }
            if !changed {
                break;
            }
        }
    }

    fn gather_inputs(values: &[bool], pins: &[Vec<u32>], out: &mut [u64]) {
        for (slot, wires) in out.iter_mut().zip(pins) {
            *slot = wires
                .iter()
                .enumerate()
                .fold(0, |acc, (i, &w)| acc | (u64::from(values[w as usize]) << i));
        }
    }

    fn eval_primitive(&mut self, index: usize) -> bool {
        let Self {
            values,
            primitives,
            scratch_in,
            scratch_out,
            ..
        } = self;
        let p = &mut primitives[index];
        let n_in = p.inputs.len();
        let n_out = p.outputs.len();
        Self::gather_inputs(values, &p.inputs, &mut scratch_in[..n_in]);
        scratch_out[..n_out].iter_mut().for_each(|v| *v = 0);
        p.implementation
            .eval(&scratch_in[..n_in], &mut scratch_out[..n_out]);
        let mut changed = false;
        for (value, wires) in scratch_out[..n_out].iter().zip(&p.outputs) {
            for (i, &w) in wires.iter().enumerate() {
                if w <= TRUE_WIRE {
                    continue;
                }
                let bit = (value >> i) & 1 == 1;
                if values[w as usize] != bit {
                    values[w as usize] = bit;
                    changed = true;
                }
            }
        }
        changed
    }

    /// First half of the clock cycle: settle, then let every sequential
    /// primitive capture its next state from the same settled values.
    pub fn tick(&mut self) -> Result<(), SimError> {
        if self.time.after_tick {
            return Err(SimError::Phase(
                "`tick` must be followed by `tock` before the next `tick`".into(),
            ));
        }
        self.eval();
        let Self {
            values,
            primitives,
            scratch_in,
            ..
        } = self;
        for p in primitives.iter_mut().filter(|p| p.sequential) {
            let n_in = p.inputs.len();
            Self::gather_inputs(values, &p.inputs, &mut scratch_in[..n_in]);
            p.implementation.tick(&scratch_in[..n_in]);
        }
        self.eval();
        self.time.after_tick = true;
        Ok(())
    }

    /// Second half of the clock cycle: publish captured state and settle.
    pub fn tock(&mut self) -> Result<(), SimError> {
        if !self.time.after_tick {
            return Err(SimError::Phase("`tock` must follow `tick`".into()));
        }
        for p in self.primitives.iter_mut().filter(|p| p.sequential) {
            p.implementation.tock();
        }
        self.eval();
        self.time.after_tick = false;
        self.time.cycle += 1;
        Ok(())
    }

    /// `tick` followed by `tock`.
    pub fn step(&mut self) -> Result<(), SimError> {
        self.tick()?;
        self.tock()
    }

    // ---------------------------------------------------------------------
    // Built-in instance access
    // ---------------------------------------------------------------------

    pub fn primitives(&self) -> &[PrimitiveInstance] {
        &self.primitives
    }

    /// Indices of primitive instances whose chip name is `chip_name`.
    pub fn instances_of(&self, chip_name: &str) -> Vec<usize> {
        self.primitives
            .iter()
            .enumerate()
            .filter(|(_, p)| p.chip_name == chip_name)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn read_state(&self, instance: usize, index: usize) -> Option<u64> {
        self.primitives
            .get(instance)?
            .implementation
            .read_state(index)
    }

    /// A debug write to built-in state. The circuit is re-evaluated so that
    /// immediate writes become visible.
    pub fn write_state(&mut self, instance: usize, index: usize, value: u64) -> bool {
        let Some(p) = self.primitives.get_mut(instance) else {
            return false;
        };
        let ok = p.implementation.write_state(index, value);
        if ok {
            self.eval();
        }
        ok
    }

    pub fn load_program(&mut self, instance: usize, words: &[u16]) -> bool {
        let Some(p) = self.primitives.get_mut(instance) else {
            return false;
        };
        let ok = p.implementation.load_program(words);
        if ok {
            self.eval();
        }
        ok
    }

    /// Sets the host key code seen by every Keyboard (and Memory) instance.
    pub fn set_key(&mut self, key: u16) {
        for p in &mut self.primitives {
            p.implementation.set_key(key);
        }
        self.eval();
    }
}

/// Orders primitives so that every combinational dependency is evaluated
/// before its readers (Kahn's algorithm).
fn schedule(primitives: &[PrimitiveInstance], wires: usize) -> (Vec<usize>, bool) {
    let mut driver = vec![usize::MAX; wires];
    for (i, p) in primitives.iter().enumerate() {
        for &w in p.outputs.iter().flatten() {
            if w > TRUE_WIRE {
                driver[w as usize] = i;
            }
        }
    }
    let mut indegree = vec![0usize; primitives.len()];
    let mut readers: Vec<Vec<usize>> = vec![Vec::new(); primitives.len()];
    for (i, p) in primitives.iter().enumerate() {
        let mut sources: Vec<usize> = p
            .combinational
            .iter()
            .flat_map(|&pin| p.inputs[pin].iter())
            .filter_map(|&w| (driver[w as usize] != usize::MAX).then_some(driver[w as usize]))
            .filter(|&d| d != i)
            .collect();
        sources.sort_unstable();
        sources.dedup();
        for d in sources {
            readers[d].push(i);
            indegree[i] += 1;
        }
    }
    let mut queue: Vec<usize> = (0..primitives.len())
        .filter(|&i| indegree[i] == 0)
        .collect();
    let mut order = Vec::with_capacity(primitives.len());
    let mut head = 0;
    while head < queue.len() {
        let i = queue[head];
        head += 1;
        order.push(i);
        for &r in &readers[i] {
            indegree[r] -= 1;
            if indegree[r] == 0 {
                queue.push(r);
            }
        }
    }
    let cyclic = order.len() < primitives.len();
    if cyclic {
        let placed: std::collections::HashSet<usize> = order.iter().copied().collect();
        order.extend((0..primitives.len()).filter(|i| !placed.contains(i)));
    }
    (order, cyclic)
}
