# n2t-cli

The `n2t` command: checks chips and scripts, runs tests, evaluates chips and starts the
language server.

```sh
cargo install --path crates/cli
```

## Commands

### `n2t check <paths>...`

Reports errors and warnings in `.hdl` and `.tst` files. Directories are searched
recursively, and each file's chips resolve against the file's own folder.

| Option | Description |
|---|---|
| `--allow-stubs` | Report empty `PARTS:` stubs as warnings instead of errors |
| `--deny-warnings` | Fail if there are any warnings |

```console
$ n2t check projects/01/Mux.hdl
error[hdl-undriven-signal]: signal `in` is not declared and is not driven by any part output
 --> projects/01/Mux.hdl:8:11
  |
8 |     And(a=in, b=sel, out=y);
  |           ^^

warning[hdl-unused-input]: input `b` is never used
 --> projects/01/Mux.hdl:2:11
  |
2 |     IN a, b, sel;
  |           ^

checked 1 file: 1 error, 1 warning
```

### `n2t test <paths>...`

Runs `.tst` scripts (directories are searched recursively) and prints `PASS`, `DONE`
(completed without a compare file) or `FAIL` for each, with the reason for each failure.

| Option | Default | Description |
|---|---|---|
| `--max-commands <N>` | `10000000` | Stop a script after `N` commands (guards against infinite loops) |
| `--require-compare` | | Treat scripts without `compare-to` as failures |
| `--strict-pauses` | | Treat `!` pauses and breakpoints as failures instead of continuing |

### `n2t eval <chip.hdl> [name=value]... [--cycles N]`

Sets inputs (decimal, `0b…` or `0x…`), evaluates the chip, optionally runs `N`
clock cycles, and prints every pin.

```console
$ n2t eval Mux16.hdl a=0x00ff b=0xff00 sel=1
IN  a    0000000011111111  (255)
IN  b    1111111100000000  (-256)
IN  sel  1  (1)
OUT out  1111111100000000  (-256)
```

### `n2t builtins`

Lists the built-in chips and their interfaces.

### `n2t lsp`

Runs the [language server](../lsp/README.md) on stdin/stdout.

## Exit status

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Errors were found, or a test failed |
| `2` | The command could not run (for example, a path does not exist) |

Colour is used when stderr is a terminal; set `NO_COLOR` to disable it.
