# n2t-lsp

A [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
server for Nand to Tetris `.hdl` and `.tst` files, built on
[`n2t-core`](../core/README.md). It is run with `n2t lsp`, which talks over
stdin/stdout. The [VS Code](../../editors/code/README.md) and
[Zed](../../editors/zed/README.md) extensions start it for you.

## Features

| Feature | `.hdl` | `.tst` |
|---|---|---|
| Diagnostics | Syntax, semantic and elaboration errors and warnings | Syntax errors and checks against the loaded chip |
| Hover | Chip interfaces and docs, pins, signals | Variables, chips, command documentation |
| Go to definition | Chips, pins, signals, built-in implementations | Loaded chips and their pins |
| Find references / highlights | Pins and signals, across the folder's chips | — |
| Rename | Pins and internal signals, updating other chips' bindings | — |
| Completion | Chip names (with pin snippets), a part's pins, signals after `=`, sections, built-in names | Commands, `.hdl` files after `load`, the loaded chip's variables |
| Document symbols | Chip, pins and parts | — |
| Inlay hints | Inferred internal bus widths | — |
| Semantic tokens | Chips, formal pins, top-level pins, internal signals, built-ins | Chips, pins, `time`, built-in state |

Go to definition on a built-in chip opens its interface text, written to a cache
directory under the system temp folder.

## How it works

Each folder containing an open document is a separate project: chips resolve
against the `.hdl` files in that folder, then the built-in library. Unsaved editor
buffers take precedence over the files on disk. Editing a chip re-publishes
diagnostics for every open document in its folder, because chips that use it and
scripts that load it may change too.

Documents are synchronised in full on every change. Compiled chips are cached and
revalidated against their sources on each request, so only changed chips and the
chips that use them are recompiled.

## Use as a library

```rust
fn main() -> n2t_lsp::Result<()> {
    n2t_lsp::run_stdio()
}
```

`n2t_lsp::run` accepts any `lsp_server::Connection`, for example an in-memory one
in tests.
