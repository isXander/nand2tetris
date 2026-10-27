import { cp, mkdir } from "node:fs/promises";
import { join } from "node:path";

// VSIX packages cannot reference files outside the extension directory.
const extensionDir = join(import.meta.dir, "..");
const sharedDir = join(extensionDir, "..", "..", "grammars", "textmate");
for (const directory of ["syntaxes", "language-configuration"]) {
  await mkdir(join(extensionDir, directory), { recursive: true });
  await cp(join(sharedDir, directory), join(extensionDir, directory), { recursive: true });
}
