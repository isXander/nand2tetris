# Nand2Tetris for Zed

Language support for [Nand to Tetris](https://www.nand2tetris.org/) hardware projects:
`.hdl` chip definitions and `.tst` test scripts.

## Features

From the tree-sitter grammars (`tree-sitter-hdl`, `tree-sitter-tst`):

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

The scripts use [Bun](https://bun.sh) and run from the repository root.

Zed fetches the grammars from git, so `extension.toml` must point at a commit that
contains them. After committing:

```sh
bun run zed:pin-grammars           # pin to HEAD on GitHub (push first)
bun run zed:pin-grammars --local   # pin to HEAD in this checkout
```

Then open the command palette, run **zed: install dev extension** and choose `editors/zed`.

After editing a `grammar.js`, regenerate the parser and commit `src/`:

```sh
bun run zed:generate-grammars        # both grammars
bun run zed:generate-grammars hdl    # just one
```

Generating needs Node.js as well as Bun: tree-sitter evaluates `grammar.js` by piping
it to `node`, which Bun's Node compatibility mode does not support.
