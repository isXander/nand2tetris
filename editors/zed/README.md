# Nand2Tetris for Zed

Language support for [Nand to Tetris](https://www.nand2tetris.org/) hardware projects:
`.hdl` chip definitions and `.tst` test scripts.

## Features

From the tree-sitter grammars ([`tree-sitter-hdl`](../../grammars/tree-sitter/hdl/README.md),
[`tree-sitter-tst`](../../grammars/tree-sitter/tst/README.md)):

- Syntax highlighting, including doc comments, chip types, formal pins vs. signals,
  case-insensitive script commands, output formats and the `time` variable
- Bracket matching, auto-indent and comment toggling
- Outline (chip, pins, parts; script loads, output lists, loops)
- Text objects (`class` = chip body, `function` = part / loop body, comments)
- Gutter run buttons: next to `CHIP Name` (test, check or evaluate the chip) and
  next to a script's `load` (run the script)

From the `n2t` language server:

- Diagnostics for syntax errors, unknown chips/pins, width mismatches, conflicting
  drivers, combinational loops, stubs and more, plus script errors such as unknown
  variables, writes to output pins and out-of-range values
- Hover documentation for chips, pins, signals and script variables
- Go to definition, including into built-in chips (opened as read-only interface files)
- Find references and rename for pins and signals, across the chips in the folder
- Completion: chip names (with a pin snippet), a part's pins, signals after `=`,
  script commands, chip files after `load`, and variables of the loaded chip
- Inlay hints showing inferred internal bus widths
- Semantic tokens (chips, formal pins, top-level pins, internal signals, built-ins),
  for Zed versions that use LSP semantic highlighting

## The `n2t` binary

The extension looks for the language server in this order:

1. `lsp.n2t.binary.path` in your Zed settings
2. `n2t` on your `PATH`
3. The latest GitHub release of `isXander/nand2tetris`, using the asset
   `n2t-<arch>-<target>.tar.gz` (`.zip` on Windows), where `<arch>` is
   `aarch64` or `x86_64` and `<target>` is `apple-darwin`, `unknown-linux-gnu`
   or `pc-windows-msvc`. The archive must contain the `n2t` binary at its root.

To install from source:

```sh
cargo install --path crates/cli
```

Settings example:

```json
{
  "lsp": {
    "n2t": {
      "binary": {
        "path": "/path/to/n2t",
        "arguments": ["lsp"]
      }
    }
  }
}
```

The run buttons and tasks call `n2t` directly, so it also needs to be on your `PATH`
for those.

## Development

Requirements: Rust installed through [rustup](https://rustup.rs) (Zed compiles the
extension with it) and [Bun](https://bun.sh) for the scripts, which run from the
repository root.

Zed fetches the grammars from git, so `extension.toml` must point at a commit that
contains them. After committing:

```sh
bun run zed:pin-grammars           # pin to HEAD on GitHub (push first)
bun run zed:pin-grammars --local   # pin to HEAD in this checkout
```

Then open the command palette, run **zed: install dev extension** and choose
`editors/zed`. Zed clones the grammars into `grammars/` and builds
`extension.wasm`; both are gitignored. If something fails, **zed: open log** shows
the build output.

The canonical parsers and highlighting queries live in [`grammars/`](../../grammars/README.md).
After editing shared highlighting queries, run `bun run zed:sync-grammars` to copy
them into Zed’s required `languages/` directory. `bun run build:zed` also does this.
Outline, runnable, indentation and other Zed-specific queries stay in this extension.

After changing the Rust code or the queries, rebuild from **zed: extensions** (the
Rebuild button on the dev extension). After editing a `grammar.js`, regenerate the
parser, commit `src/` and re-pin before rebuilding:

```sh
bun run grammars:generate        # both grammars
bun run grammars:generate hdl    # just one
```

Generating needs Node.js as well as Bun: tree-sitter evaluates `grammar.js` by piping
it to `node`, which Bun's Node compatibility mode does not support.

`.cargo/config.toml` keeps this crate's build output in `editors/zed/target` rather
than the workspace's `target/`, because that is where Zed looks for the compiled wasm.
