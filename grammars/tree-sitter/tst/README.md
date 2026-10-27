# tree-sitter-tst

A [tree-sitter](https://tree-sitter.github.io/) grammar for Nand to Tetris test script files (`.tst`).
The [Zed extension](../../../editors/zed/README.md) uses it for highlighting, outline, brackets,
indentation and text objects; the queries are in [`queries`](queries) and [`editors/zed/languages/tst`](../../../editors/zed/languages/tst).

```tst
load Xor.hdl,
output-file Xor.out,
output-list a%B3.1.3 b%B3.1.3 out%B3.1.3;
repeat 2 { set a 1, eval, output; }
```

## Nodes

| Node | Matches |
|---|---|
| `command` | A command and its `terminator` (`,`, `;` or `!`) (`body`, `terminator`) |
| `load_command`, `output_file_command`, `compare_to_command` | File commands (`file`) |
| `output_list_command`, `output_item` | `output-list` and its items (`variable`, `format`) |
| `set_command`, `breakpoint_command`, `echo_command` | Commands with arguments |
| `simple_command` | `eval`, `output`, `tick`, `tock`, `clear-echo`, `clear-breakpoints` (and `ticktock`, `vmstep`) |
| `builtin_call` | `ROM32K load Max.hack` (`chip`, `method`, `argument`) |
| `repeat_block`, `while_block`, `block`, `condition` | Loops |
| `variable`, `index` | `name`, `RAM16K[3]`, `PC[]` |
| `number`, `radix_number`, `string`, `percent_string`, `format`, `file_name` | Literals |
| `line_comment`, `block_comment` | Comments |

Commands are matched case-insensitively, as in the reference simulator, and appear in the tree as their lower-case spelling (e.g. `"load"`). The grammar follows the repository's [test-script specification](../../../nand2tetris-tst-spec.md). Rules the parser in `n2t-core` enforces, such as no nested loops and a positive repeat count, are not enforced here.

## Development

`src/` is generated from `grammar.js` and is committed, because Zed builds the parser
from it. After editing the grammar, regenerate it from the repository root:

```sh
bun run grammars:generate tst
```

This uses `tree-sitter-cli` 0.25 with ABI 14, and needs Node.js.
