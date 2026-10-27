# nand2tetris

A toolchain for the hardware half of [Nand to Tetris](https://www.nand2tetris.org/):
an HDL compiler, a hardware simulator and test-script runner, a language server,
editor extensions for VS Code, Zed, and IntelliJ IDEA.

```console
$ n2t test projects/01
PASS projects/01/And.tst (5 lines compared)
FAIL projects/01/Xor.tst
error: comparison failure at line 3, column 21:
  expected: |   0   |   1   |   1   |
  actual:   |   0   |   1   |   0   |
 --> projects/01/Xor.tst:6:25
  |
6 | set a 0, set b 1, eval, output;
  |                         ^^^^^^^

1 passed, 1 failed
```

## Features

- Checks chips before they run: unknown chips and
  pins, bus width mismatches, out-of-range and internal-bus slices, conflicting
  drivers, combinational feedback loops, chips that include themselves, and
  unimplemented stubs. It also warns about unused or undriven signals.
- Runs `.tst` scripts with the book's semantics: two-phase `tick`/`tock` clocking,
  exact fixed-column `.out` formatting, `.cmp` comparison with `*` wildcards,
  `repeat`/`while`, breakpoints and built-in chip state (`RAM16K[3]`, `PC[]`, `ROM32K load`).
- Native implementations of every standard built-in chip except `CPU` and `Computer`,
  which the book has you build yourself. A project-local `.hdl` file always takes
  precedence over a built-in.
- Editor support through the language server: diagnostics, hover, go to definition,
  references, rename, completion, inlay hints and semantic highlighting.

## Installation

Install the `n2t` command from source (requires [Rust](https://rustup.rs)):

```sh
cargo install --path crates/cli
```

Then install an editor extension:

- **VS Code:** see [editors/code](editors/code/README.md)
- **Zed:** see [editors/zed](editors/zed/README.md)
- **IntelliJ IDEA:** see [editors/idea](editors/idea/README.md)

## Usage

```sh
n2t check projects/01              # report errors in .hdl and .tst files
n2t test projects/01               # run every .tst script in a folder
n2t test projects/03/a/Bit.tst     # run one script
n2t eval Mux16.hdl a=0x00ff b=0xff00 sel=1
n2t builtins                       # list the built-in chips
n2t lsp                            # language server on stdin/stdout
```

See [crates/cli](crates/cli/README.md) for every option.

Chips are resolved the way the reference Hardware Simulator does it: `Xxx.hdl` in
the same folder as the file being checked or the script being run, otherwise the
built-in chip `Xxx`.

## Repository layout

| Path | Contents |
|---|---|
| [`crates/core`](crates/core/README.md) | `n2t-core`: HDL and TST lexers, parsers, semantic analysis and the HDL elaborator |
| [`crates/simulator`](crates/simulator/README.md) | `n2t-simulator`: circuit simulation, built-in chips and the test-script runner |
| [`crates/lsp`](crates/lsp/README.md) | `n2t-lsp`: the language server |
| [`crates/cli`](crates/cli/README.md) | `n2t-cli`: the `n2t` command |
| [`samples`](samples/README.md) | Example chips and scripts, including the official project tests, used as test fixtures |
| [`editors/code`](editors/code/README.md) | VS Code extension |
| [`editors/zed`](editors/zed/README.md) | Zed extension |
| [`editors/idea`](editors/idea/README.md) | Kotlin/Gradle IntelliJ IDEA extension using LSP4IJ |
| [`grammars`](grammars/README.md) | Shared TextMate and tree-sitter grammars |
| [`nand2tetris-spec.md`](nand2tetris-spec.md) | Integration specification: resolution, elaboration, timing and architecture |
| [`nand2tetris-hdl-spec.md`](nand2tetris-hdl-spec.md) | HDL language specification |
| [`nand2tetris-tst-spec.md`](nand2tetris-tst-spec.md) | Test-script language specification |

The specifications record which behaviour comes from the book, which from the
reference Java simulator, and which is a decision made for this implementation.

## Development

Requirements: [Rust](https://rustup.rs) and [Bun](https://bun.sh). Regenerating the
tree-sitter grammars also needs Node.js. Building the IDEA plugin requires JDK 21
or newer on `PATH`; its Gradle wrapper downloads Gradle.

```sh
bun install            # JavaScript dependencies for the editor extensions
cargo build            # all Rust crates
cargo test --workspace
```

The tests run everything in [`samples/`](samples/README.md). After an intentional
change to diagnostics or messages, `N2T_BLESS=1 cargo test --workspace` rewrites the
golden `.expected` files.

Root `package.json` scripts:

| Script | Does |
|---|---|
| `bun run build:rust` | `cargo build --release` |
| `bun run build:code` | Bundle the VS Code extension |
| `bun run build:zed` | Sync highlighting queries and build the Zed extension's wasm |
| `bun run build:idea` | Package the IDEA plugin ZIP |
| `bun run idea:run` | Launch IDEA with the plugin in a development sandbox |
| `bun run idea:verify` | Verify the IDEA plugin package structure |
| `bun run test` | `cargo test --workspace` |
| `bun run grammars:generate` | Regenerate the shared tree-sitter parsers (`zed:generate-grammars` remains an alias) |
| `bun run zed:sync-grammars` | Copy shared highlighting queries into the Zed extension |
| `bun run zed:pin-grammars` | Point the Zed extension at the current commit's grammars |

## Releasing

Bump `version` in the root `Cargo.toml`, commit, then push a matching tag:

```sh
git tag v0.1.0 && git push origin v0.1.0
```

[`.github/workflows/release.yml`](.github/workflows/release.yml) runs the tests, builds
`n2t` for macOS, Linux and Windows (x86_64 and aarch64), and publishes a GitHub
release with `n2t-<arch>-<target>.tar.gz`/`.zip` archives and `SHA256SUMS.txt`. The
Zed extension downloads `n2t` from the latest release. Tags with a suffix
(`v0.2.0-rc.1`) are published as pre-releases, which the extension ignores.

## License

[MPL-2.0](https://mozilla.org/MPL/2.0/).
