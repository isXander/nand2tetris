import { copyFile } from "node:fs/promises";
import { join } from "node:path";

const extensionDir = join(import.meta.dir, "..");
for (const language of ["hdl", "tst"]) {
  await copyFile(
    join(extensionDir, "..", "..", "grammars", "tree-sitter", language, "queries", "highlights.scm"),
    join(extensionDir, "languages", language, "highlights.scm"),
  );
}
