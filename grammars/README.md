# Shared language grammars

- [`textmate/`](textmate): TextMate grammars for `.hdl`, `.tst`, `.cmp` and `.out`,
  plus language configurations and a VS Code-compatible bundle manifest.
  VS Code copies the bundle assets during its build. IDEA packages this directory
  directly and registers it with the bundled TextMate plugin.
- [`tree-sitter/`](tree-sitter/README.md): one package containing the
  [HDL](tree-sitter/hdl/README.md) and [TST](tree-sitter/tst/README.md)
  parser sources, generated C parsers
  (ABI 14), and highlighting queries. Zed references these directories in its
  `extension.toml`; its other editor-specific queries stay in `editors/zed/languages`.

Run scripts from the repository root:

```sh
bun run grammars:generate          # both parsers; requires Bun and Node.js
bun run grammars:generate hdl      # just HDL
bun run zed:sync-grammars          # copy shared highlights into Zed's language folders
bun run build:code                # copy shared TextMate assets and bundle VS Code
bun run build:idea                # package IDEA with the shared TextMate bundle
```

Edit sources here, rather than the copies in editor build directories.
Generated tree-sitter `src/` files are committed. After committing parser changes,
run `bun run zed:pin-grammars` and push the referenced commit before installing
from GitHub, or use `bun run zed:pin-grammars --local` for a local dev extension.
The grammar move needs a new commit and pin before Zed can fetch the new paths.
