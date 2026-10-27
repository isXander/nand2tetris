# n2t-core

The front-end of the toolchain: lexing, parsing and semantic analysis of `.hdl` chips
and `.tst` test scripts, and elaboration of chips into validated netlists. It does no
simulation (see [`n2t-simulator`](../simulator/README.md)) and has no dependencies.

Both the CLI and the language server are built on it, so they report the same
diagnostics.

## Modules

| Module | Contents |
|---|---|
| `hdl::lexer`, `hdl::parser`, `hdl::ast` | Lossless lexer (comments kept as trivia), error-recovering parser, location-aware syntax tree. `hdl::parse` runs both. |
| `hdl::semantic` | Analysis of one chip against its children's interfaces: binding directions, inferred internal bus widths, selectors, drivers, usage warnings, and symbol references for editors. |
| `hdl::elaborator` | `Elaborator`: resolves chips (project file first, then built-in), compiles children on demand, caches results and revalidates them against the sources, detects chips that include themselves and combinational loops, and produces a `ChipDefinition`. |
| `hdl::definition` | `ChipDefinition`: the immutable netlist a simulator instantiates (nets, parts, bit-range connections, combinational dependencies). |
| `hdl::resolve` | `SourceProvider` and its implementations: file system (case-exact file names), in memory, and unsaved editor buffers layered over another provider. |
| `builtins` | `BuiltinLibrary`: interfaces, clocked pins, script-visible state and methods of the standard built-in chips. Behaviour lives in the simulator. |
| `tst::lexer`, `tst::parser`, `tst::ast` | Test-script lexer, parser and syntax tree. The tree keeps `,` `;` `!` terminators and doubles as the execution plan. |
| `tst::semantic` | Static checks of a script against the chips it loads: variables, permissions, value widths, condition types, output formats, method calls. |
| `diagnostic`, `source` | `Diagnostic` with stable codes, `Span`, and `LineIndex` for line/column (including UTF-16) conversion. |

## Example

```rust
use std::sync::Arc;

use n2t_core::builtins::BuiltinLibrary;
use n2t_core::hdl::Elaborator;
use n2t_core::hdl::resolve::FsSourceProvider;

let mut elaborator = Elaborator::new(
    FsSourceProvider::new("projects/01"),
    Arc::new(BuiltinLibrary::standard()),
);

// Compiles Xor.hdl and every chip it uses.
let unit = elaborator.compile("Xor");
for diagnostic in &unit.diagnostics {
    eprintln!("{diagnostic}");
}
if let Some(definition) = &unit.definition {
    println!("{} is valid: {}", definition.name, definition.interface);
}

// Scripts are checked against the same elaborator.
let script = std::fs::read_to_string("projects/01/Xor.tst").unwrap();
let (parsed, analysis) = n2t_core::tst::check(&script, &mut elaborator);
let errors = parsed.diagnostics.iter().chain(&analysis.diagnostics);
```

`ElaboratorOptions::editor()` turns empty `PARTS:` stubs into warnings, for editors
where a half-written project is normal.

## Behaviour

The rules implemented here, and where they come from, are in the repository's
specifications: [integration](../../nand2tetris-spec.md), [HDL](../../nand2tetris-hdl-spec.md)
and [test scripts](../../nand2tetris-tst-spec.md). The main decisions beyond the book:

- Bus widths are limited to 16 bits by default (`AnalysisOptions::max_width`).
- Identifiers may contain `_` (with a portability warning); `-` is an error.
- A part input read from a top-level output is an error; connect the part output to
  an extra internal signal instead.
- An internal signal that is read but never driven is an error. Unconnected part
  inputs default to `false`, with a warning.
- A built-in output depends combinationally on every unclocked input unless the
  output itself is clocked. This is how feedback through `DFF`, `Register`, `PC` and
  the RAMs is accepted.
