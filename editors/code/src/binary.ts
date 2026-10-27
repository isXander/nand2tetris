import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";

const EXECUTABLE = process.platform === "win32" ? "n2t.exe" : "n2t";

function isExecutable(file: string): boolean {
  try {
    const stat = fs.statSync(file);
    if (!stat.isFile()) return false;
    if (process.platform !== "win32") fs.accessSync(file, fs.constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function expand(configured: string): string {
  let file = configured;
  if (file === "~" || file.startsWith("~/") || file.startsWith("~\\")) {
    file = path.join(os.homedir(), file.slice(1));
  }
  if (!path.isAbsolute(file)) {
    const folder = vscode.workspace.workspaceFolders?.[0];
    if (folder) file = path.join(folder.uri.fsPath, file);
  }
  return file;
}

/**
 * Locates the `n2t` executable: the `nand2tetris.path` setting, a binary
 * bundled in the extension's `server/` folder, `n2t` on `PATH`, then
 * `~/.cargo/bin` (which GUI-launched editors often lack on `PATH`).
 */
export function findN2t(context: vscode.ExtensionContext): string | undefined {
  const configured = vscode.workspace.getConfiguration("nand2tetris").get<string>("path")?.trim();
  if (configured) {
    return expand(configured);
  }

  const bundled = context.asAbsolutePath(path.join("server", EXECUTABLE));
  if (isExecutable(bundled)) return bundled;

  for (const dir of (process.env.PATH ?? "").split(path.delimiter)) {
    if (!dir) continue;
    const candidate = path.join(dir, EXECUTABLE);
    if (isExecutable(candidate)) return candidate;
  }

  const cargo = path.join(process.env.CARGO_HOME ?? path.join(os.homedir(), ".cargo"), "bin", EXECUTABLE);
  if (isExecutable(cargo)) return cargo;

  return undefined;
}

/** Like {@link findN2t}, but explains how to fix a missing binary. */
export function requireN2t(context: vscode.ExtensionContext): string | undefined {
  const found = findN2t(context);
  if (found && isExecutable(found)) return found;

  const message = found
    ? `Nand2Tetris: \`${found}\` (from the nand2tetris.path setting) is not an executable file.`
    : "Nand2Tetris: could not find the `n2t` executable. Install it with `cargo install --path crates/cli` from the nand2tetris repository, or set nand2tetris.path.";
  void vscode.window.showErrorMessage(message, "Open Settings").then((choice) => {
    if (choice === "Open Settings") {
      void vscode.commands.executeCommand("workbench.action.openSettings", "nand2tetris.path");
    }
  });
  return undefined;
}
