# Samples

Example chips and test scripts that double as the repository's test fixtures.

| Folder | Contents | Checked by |
|---|---|---|
| `projects/` | Working implementations of the hardware chips from Nand to Tetris projects 1, 2, 3 and 5, with the official test scripts | Every chip and script must have no diagnostics, and every script must pass |
| `features/` | Small examples of specific HDL and script features: bus constants, a local `BUILTIN` chip, clocked feedback, and most script commands | Same as `projects/` |
| `errors/hdl/`, `errors/tst/` | Files with mistakes; `File.expected` holds the diagnostics each one must produce | `n2t-core` tests |
| `runtime/` | Scripts that fail or stop while running; `Script.tst.expected` holds the outcome | `n2t-simulator` tests |

Each project folder resolves chips the way the Hardware Simulator does: a chip
missing from the folder is the built-in one. So project 2's adders use the built-in
gates, not project 1's. Project 3 is split into `a/` and `b/`, as in the book's first
edition, so that the large RAMs in `b/` are built from the built-in `RAM64` and stay
fast to simulate.

## Running

```sh
cargo test --workspace              # everything
n2t test samples/projects           # or run the scripts directly
n2t check samples/errors/hdl
```

After an intentional change to diagnostics or run-time messages, regenerate the
`.expected` files and review the diff:

```sh
N2T_BLESS=1 cargo test --workspace
git diff samples/
```

## Official test files

The `.tst` and `.cmp` files in `projects/` (and the `.hack` programs in
`projects/05`) come from the official Nand to Tetris
[web IDE](https://github.com/nand2tetris/web-ide/tree/6a077715f74f2bc9c0d1a57f1d67eb312f4fffdf/projects/src),
and accompany the book *The Elements of Computing Systems* by Noam Nisan and
Shimon Schocken (MIT Press). The chip implementations (`.hdl`) are this
repository's own.

Edits made to follow this repository's specifications, each marked with a comment
in the script:

- Every script gains `output-file Xxx.out`, because `output-list` requires an output file.
- `02/ALU.tst` gives `out` an explicit `%B1.16.1`, because the default format is `%B1.1.1`.
- `05/ComputerAdd.tst` writes `ARegister[]` / `DRegister[]` instead of `[0]`.
- `05/Memory.tst` and `Memory.cmp` no longer wait for the user to hold down a key
  (tests run without a keyboard, so the keyboard reads 0).
