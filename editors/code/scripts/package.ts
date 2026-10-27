#!/usr/bin/env bun
/**
 * Packages the extension as a `.vsix`.
 *
 *   bun run package                 # universal package; uses `n2t` from PATH or settings
 *   bun run package --with-server   # bundles `n2t` for this platform (platform-specific package)
 */
import { $ } from "bun";
import { rm } from "node:fs/promises";
import { join } from "node:path";

const extensionDir = join(import.meta.dir, "..");
const withServer = process.argv.includes("--with-server");

/** VS Code's platform identifier for this machine, e.g. `darwin-arm64`. */
function vscodeTarget(): string {
  const arch = process.arch === "arm64" ? "arm64" : process.arch === "x64" ? "x64" : undefined;
  const os = ({ darwin: "darwin", linux: "linux", win32: "win32" } as Record<string, string>)[process.platform];
  if (!arch || !os) throw new Error(`unsupported platform ${process.platform}-${process.arch}`);
  return `${os}-${arch}`;
}

await rm(join(extensionDir, "server"), { recursive: true, force: true });
const args = ["package", "--no-dependencies", "--skip-license"];
if (withServer) {
  await $`bun scripts/bundle-server.ts`.cwd(extensionDir);
  args.push("--target", vscodeTarget());
}
await $`bunx vsce ${args}`.cwd(extensionDir);
