# Nand2Tetris tree-sitter grammars

One package contains both language grammars:

- [`hdl/`](hdl/README.md): HDL chip definitions.
- [`tst/`](tst/README.md): test scripts.

Each language has its own `grammar.js`, generated `src/` parser, highlighting
queries and `tree-sitter.json` so Zed can build and load each parser separately.
They share package metadata and generation tooling.

From the repository root:

```sh
bun run grammars:generate       # regenerate both parsers
bun run grammars:generate hdl   # regenerate one parser
bun run build:zed              # sync highlighting queries and build the extension
```

Generation requires Bun and Node.js. Generated parsers use ABI 14 and are committed.
After committing changes, re-pin Zed with `bun run zed:pin-grammars --local` for
local development, or `bun run zed:pin-grammars` for GitHub installs.
