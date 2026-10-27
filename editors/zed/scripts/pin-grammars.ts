#!/usr/bin/env bun
/**
 * Points the grammar entries in `editors/zed/extension.toml` at a commit that
 * contains them.
 *
 *   bun run zed:pin-grammars            # HEAD on the GitHub repository (push first)
 *   bun run zed:pin-grammars --local    # HEAD in this checkout (for dev installs)
 */
import { $ } from "bun";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const GITHUB_REPOSITORY = "https://github.com/isXander/nand2tetris";

const extensionDir = join(import.meta.dir, "..");
const manifestPath = join(extensionDir, "extension.toml");

const git = async (...args: string[]) => {
  const result = await $`git ${args}`.cwd(extensionDir).nothrow().quiet();
  if (result.exitCode !== 0) {
    console.error(`git ${args.join(" ")} failed: ${result.stderr.toString().trim()}`);
    console.error("the grammars must be committed before they can be pinned");
    process.exit(1);
  }
  return result.text().trim();
};

const root = await git("rev-parse", "--show-toplevel");
const rev = await git("rev-parse", "HEAD");
const repository = process.argv.includes("--local")
  ? pathToFileURL(root).href
  : GITHUB_REPOSITORY;

const manifest = await Bun.file(manifestPath).text();
const grammarPaths = [...manifest.matchAll(/^path = "(grammars\/tree-sitter\/[^"]+)"$/gm)]
  .map((match) => match[1]!);
for (const path of grammarPaths) {
  // Zed fetches a committed snapshot; uncommitted moves cannot be pinned yet.
  await git("cat-file", "-e", `${rev}:${path}/src/parser.c`);
}

let inGrammar = false;
const lines = manifest.split("\n").map((line) => {
  if (line.startsWith("[")) {
    inGrammar = line.startsWith("[grammars.");
  }
  if (inGrammar && line.startsWith("repository = ")) {
    return `repository = "${repository}"`;
  }
  if (inGrammar && line.startsWith("rev = ")) {
    return `rev = "${rev}"`;
  }
  return line;
});
await Bun.write(manifestPath, lines.join("\n"));

console.log(`grammars pinned to ${rev} (${repository})`);
