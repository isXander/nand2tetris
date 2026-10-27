# n2t-simulator

The hardware simulator: runs validated chip definitions from
[`n2t-core`](../core/README.md) and executes `.tst` test scripts against them.

## Modules

| Module | Contents |
|---|---|
| `circuit` | `Circuit`: builds a fresh circuit from a `ChipDefinition` by flattening the chip hierarchy into built-in primitives connected by shared wires, evaluates them in dependency order, and implements the two-phase clock. |
| `primitives` | `BuiltinRegistry` and native implementations of the standard built-in chips: gates, adders, `ALU`, `DFF`, `Bit`, registers, `PC`, `RAM8`–`RAM16K`, `ROM32K`, `Screen`, `Keyboard` and `Memory`. |
| `runner` | `ScriptRunner`: executes a script with its loops, terminators, pauses, breakpoints, output files and comparison, returning a structured `Outcome`. |
| `format` | Fixed-column `.out` formatting and the exact `.cmp` comparator (`*` matches any one character). |

## Example

Simulate a chip directly:

```rust
use std::sync::Arc;

use n2t_core::builtins::BuiltinLibrary;
use n2t_core::hdl::Elaborator;
use n2t_core::hdl::resolve::FsSourceProvider;
use n2t_simulator::{BuiltinRegistry, Circuit};

let mut elaborator = Elaborator::new(
    FsSourceProvider::new("projects/03/a"),
    Arc::new(BuiltinLibrary::standard()),
);
let definition = elaborator.definition("Bit").expect("Bit.hdl has errors");

let mut bit = Circuit::new(definition, &BuiltinRegistry::standard()).unwrap();
bit.set("in", 1);
bit.set("load", 1);
bit.tick().unwrap(); // capture
assert_eq!(bit.get("out"), Some(0));
bit.tock().unwrap(); // publish
assert_eq!(bit.get("out"), Some(1));
```

Run a test script:

```rust
use std::path::Path;
use std::sync::Arc;

use n2t_core::builtins::BuiltinLibrary;
use n2t_simulator::{BuiltinRegistry, Outcome, RunnerOptions, ScriptRunner};

let mut runner = ScriptRunner::open(
    Path::new("projects/03/a/Bit.tst"),
    Arc::new(BuiltinLibrary::standard()),
    BuiltinRegistry::standard(),
    RunnerOptions::default(),
)
.unwrap();

match runner.run() {
    Outcome::Passed { compared_lines } => println!("passed ({compared_lines} lines)"),
    Outcome::FailedComparison { mismatch, .. } => println!("{mismatch}"),
    other => println!("{other:?}"),
}
```

`run()` stops at `!` pauses and breakpoints with `Outcome::Suspended`; call it again to
continue. `step()` stops at the end of each `;` simulation step.

## Semantics

- **Clock.** `tick` settles the circuit, then every clocked primitive captures its next
  state from the same settled values. `tock` publishes the captured state. Two
  `DFF`s in series therefore do not pass a value through in one cycle. `tick` twice
  in a row, or `tock` before `tick`, is an error.
- **Memory.** RAM and `Screen` reads are combinational and use committed storage;
  writes are captured at `tick` and committed at `tock`.
- **Script state.** `set Register[] v` takes effect at the next `tock`. `ARegister[]`,
  `DRegister[]`, `PC[]` and RAM/`Screen` cells change immediately. `ROM32K[]` and
  `Keyboard[]` are read-only; `ROM32K load prog.hack` loads a program.
- **Outcomes.** A run ends as `Passed`, `CompletedUnchecked` (no `compare-to`),
  `FailedComparison`, `FailedError`, `Suspended`, `Cancelled` or `LimitExceeded`. A
  compare file that is never compared against, or has expected lines left over,
  fails.
- **Not supported:** loading `.asm` programs into `ROM32K` (it needs an assembler;
  load the `.hack` file instead) and the CPU-emulator and VM-emulator commands
  `ticktock` and `vmstep`. These report errors.
