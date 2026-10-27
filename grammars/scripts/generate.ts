#!/usr/bin/env bun
/**
 * Regenerates the tree-sitter parsers (`src/`) from each `grammar.js`.
 *
 *   bun run grammars:generate          # every grammar
 *   bun run grammars:generate hdl      # only HDL
 */
import { $ } from "bun";
import { join } from "node:path";

const TREE_SITTER_CLI = "tree-sitter-cli@0.25";
const ABI = "14";
const GRAMMARS = ["hdl", "tst"];

// tree-sitter evaluates grammar.js by piping it to `node -`, which Bun's
// node shim does not support, so a real Node.js must be installed.
if (!Bun.which("node")) {
  console.error("tree-sitter needs Node.js to evaluate grammar.js; install it and retry");
  process.exit(1);
}

const requested = process.argv.slice(2);
const unknown = requested.filter((name) => !GRAMMARS.includes(name));
if (unknown.length > 0) {
  console.error(`unknown grammar(s): ${unknown.join(", ")} (available: ${GRAMMARS.join(", ")})`);
  process.exit(1);
}

for (const name of requested.length > 0 ? requested : GRAMMARS) {
  const dir = join(import.meta.dir, "..", "tree-sitter", name);
  console.log(`generating tree-sitter/${name}`);
  await $`bunx ${TREE_SITTER_CLI} generate --abi ${ABI}`.cwd(dir);
}
