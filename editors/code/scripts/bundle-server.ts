#!/usr/bin/env bun
/**
 * Builds `n2t` in release mode and copies it into `server/`, where the
 * extension looks for a bundled language server.
 *
 *   bun run bundle-server                         # host platform
 *   bun run bundle-server --target <rust-triple>  # cross-compile (target must be installed)
 */
import { $ } from "bun";
import { chmod, copyFile, mkdir } from "node:fs/promises";
import { join } from "node:path";

const extensionDir = join(import.meta.dir, "..");
const repoRoot = join(extensionDir, "..", "..");

const targetIndex = process.argv.indexOf("--target");
const target = targetIndex >= 0 ? process.argv[targetIndex + 1] : undefined;
const windows = target ? target.includes("windows") : process.platform === "win32";
const executable = windows ? "n2t.exe" : "n2t";

const targetArgs = target ? ["--target", target] : [];
await $`cargo build --release -p n2t-cli ${targetArgs}`.cwd(repoRoot);

const built = join(repoRoot, "target", ...(target ? [target] : []), "release", executable);
const destination = join(extensionDir, "server", executable);
await mkdir(join(extensionDir, "server"), { recursive: true });
await copyFile(built, destination);
if (!windows) await chmod(destination, 0o755);
console.log(`bundled ${built} -> ${destination}`);
