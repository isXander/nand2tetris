# Nand2Tetris for VS Code

Language support for [Nand to Tetris](https://www.nand2tetris.org/) hardware projects:
`.hdl` chips, `.tst` test scripts and `.cmp`/`.out` comparison tables.

## Features

- **Syntax highlighting** for HDL, test scripts (case-insensitive commands, output
  formats, `time`) and comparison tables (columns, `*` wildcards), plus semantic
  highlighting from the language server (chips, formal pins, top-level pins, internal
  signals, built-ins)
- **Diagnostics**: syntax errors, unknown chips and pins, bus width mismatches,
  conflicting drivers, combinational loops, unimplemented stubs, and script errors
  such as unknown variables, writes to output pins and out-of-range values
- **Hover** documentation for chips, pins, signals and script variables
- **Go to definition**, including into built-in chips
- **Find references** and **rename** for pins and signals across the chips in a folder
- **Completion**: chip names (with a pin snippet), a part's pins, signals after `=`,
  script commands, chip files after `load`, and the loaded chip's variables
- **Inlay hints** for inferred internal bus widths, **outline** and **breadcrumbs**
- **Test Explorer**: every `.tst` script is a test, run from the Testing view, the
  gutter next to its `load` line, the editor's run button or the explorer context
  menu. Comparison failures show an expected/actual diff and jump to the failing
  command. Continuous runs re-test whenever a chip, script or `.cmp` file in the
  folder is saved.
- **Compare Output** (`.out` ↔ `.cmp` side-by-side diff), **Evaluate Chip** (set
  inputs and see the outputs), and `n2t: check` / `n2t: test` **tasks** with a
  `$n2t` problem matcher
- Snippets for chips and test scripts

## Requirements

The extension uses the `n2t` command-line tool. It looks for it in this order:

1. The `nand2tetris.path` setting
2. A binary bundled with the extension (platform-specific packages)
3. `n2t` on `PATH`
4. `~/.cargo/bin/n2t`

To install from source:

```sh
cargo install --path crates/cli
```

## Settings

| Setting | Default | Description |
|---|---|---|
| `nand2tetris.path` | `""` | Path to `n2t` |
| `nand2tetris.server.enable` | `true` | Start the language server |
| `nand2tetris.trace.server` | `off` | Log language server traffic |
| `nand2tetris.test.maxCommands` | `10000000` | Command limit per script |
| `nand2tetris.test.requireCompare` | `false` | Fail scripts without `compare-to` |
| `nand2tetris.test.strictPauses` | `false` | Fail on `!` pauses and breakpoints |

## Development

Requires [Bun](https://bun.sh). From the repository root, run `bun install` once. Then, in `editors/code`:

```sh
bun run build       # bundle src/ into dist/extension.js
bun run watch       # rebuild on change
bun run typecheck
```

Press <kbd>F5</kbd> with `editors/code` open in VS Code to launch an Extension
Development Host.

Packaging:

```sh
bun run package                 # universal .vsix; uses n2t from settings or PATH
bun run package --with-server   # .vsix for this platform, with n2t bundled (needs Rust)
code --install-extension nand2tetris-*.vsix
```

## Shared grammars

The TextMate grammars and language configurations are maintained in
[`grammars/textmate`](../../grammars/textmate), shared with the IDEA plugin.
The build copies them into this extension for packaging; edit the shared source,
then run `bun run build:code` from the repository root.
