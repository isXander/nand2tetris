# tree-sitter-hdl

A [tree-sitter](https://tree-sitter.github.io/) grammar for Nand to Tetris HDL files (`.hdl`).
The [Zed extension](../../../editors/zed/README.md) uses it for highlighting, outline, brackets,
indentation and text objects; the queries are in [`queries`](queries) and [`editors/zed/languages/hdl`](../../../editors/zed/languages/hdl).

```hdl
CHIP Xor {
    IN a, b;
    OUT out;

    PARTS:
    Not(in=a, out=nota);
    And(a=a, b=notb, out=w1);
}
```

## Nodes

| Node | Matches |
|---|---|
| `chip_definition` | `CHIP Name { ... }` (`name`, `body`) |
| `input_section`, `output_section` | `IN ...;`, `OUT ...;` |
| `pin_declaration` | `name` or `name[width]` (`name`, `width`) |
| `parts_section` | `PARTS:` followed by parts |
| `part` | `Chip(...);` (`chip`, `connections`) |
| `connection` | `pin=signal` (`pin`, `signal`) |
| `pin_reference` | `name` with an optional `selector` (`[i]` or `[i..j]`) |
| `builtin_section`, `clocked_clause` | `BUILTIN Name;` and `CLOCKED pins;` |
| `boolean`, `number`, `identifier` | Literals and names |
| `line_comment`, `block_comment` | Comments, including `/** documentation */` |

The grammar follows the repository's [HDL specification](../../../nand2tetris-hdl-spec.md), including optional `IN`/`OUT` sections. It is deliberately permissive where the language server reports errors with better messages (for example, it accepts `_` in identifiers).

## Development

`src/` is generated from `grammar.js` and is committed, because Zed builds the parser
from it. After editing the grammar, regenerate it from the repository root:

```sh
bun run grammars:generate hdl
```

This uses `tree-sitter-cli` 0.25 with ABI 14, and needs Node.js.
