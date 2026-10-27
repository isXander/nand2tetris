import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import { runProcess } from "./process";
import { TASK_TYPE, createTask } from "./tasks";
import type { ScriptTests } from "./testing";

function targetUri(uri: unknown): vscode.Uri | undefined {
  return uri instanceof vscode.Uri ? uri : vscode.window.activeTextEditor?.document.uri;
}

function sibling(uri: vscode.Uri, extension: string): vscode.Uri {
  const parsed = path.parse(uri.fsPath);
  return vscode.Uri.file(path.join(parsed.dir, parsed.name + extension));
}

/** `nand2tetris.runTest`: runs a `.tst` (or the `.tst` next to a `.hdl`). */
export async function runTest(tests: ScriptTests, arg: unknown): Promise<void> {
  const uri = targetUri(arg);
  if (!uri) return;
  await vscode.workspace.saveAll(false);
  let script = uri;
  if (uri.fsPath.endsWith(".hdl")) {
    script = sibling(uri, ".tst");
    if (!fs.existsSync(script.fsPath)) {
      void vscode.window.showWarningMessage(`No test script ${path.basename(script.fsPath)} next to this chip.`);
      return;
    }
  }
  await tests.runFiles([script]);
  void vscode.commands.executeCommand("testing.showMostRecentOutput");
}

/** Reads `output-file`/`compare-to` names from a script, defaulting to its stem. */
function companionFiles(script: vscode.Uri): { out: vscode.Uri; cmp: vscode.Uri } {
  let text = "";
  try {
    text = fs.readFileSync(script.fsPath, "utf8");
  } catch {
    // fall back to the default names
  }
  const dir = path.dirname(script.fsPath);
  const named = (command: string, extension: string) => {
    const match = new RegExp(`\\b${command}\\s+"?([^\\s,;!"]+)`, "i").exec(text);
    return vscode.Uri.file(path.join(dir, match?.[1] ?? path.parse(script.fsPath).name + extension));
  };
  return { out: named("output-file", ".out"), cmp: named("compare-to", ".cmp") };
}

/** `nand2tetris.compareOutput`: diffs the expected `.cmp` against the generated `.out`. */
export async function compareOutput(arg: unknown): Promise<void> {
  const uri = targetUri(arg);
  if (!uri) return;
  let out: vscode.Uri;
  let cmp: vscode.Uri;
  if (uri.fsPath.endsWith(".tst")) {
    ({ out, cmp } = companionFiles(uri));
  } else {
    const script = sibling(uri, ".tst");
    ({ out, cmp } = fs.existsSync(script.fsPath)
      ? companionFiles(script)
      : { out: sibling(uri, ".out"), cmp: sibling(uri, ".cmp") });
  }
  for (const file of [cmp, out]) {
    if (!fs.existsSync(file.fsPath)) {
      void vscode.window.showWarningMessage(
        `${path.basename(file.fsPath)} does not exist${file === out ? " yet; run the test first" : ""}.`,
      );
      return;
    }
  }
  const title = `${path.basename(cmp.fsPath)} (expected) ↔ ${path.basename(out.fsPath)} (actual)`;
  await vscode.commands.executeCommand("vscode.diff", cmp, out, title);
}

/** Input pin names declared in a chip's `IN` section. */
function inputPins(text: string): string[] {
  const section = /\bIN\b([^;]*);/.exec(text.replace(/\/\/.*$|\/\*[\s\S]*?\*\//gm, ""));
  return (section?.[1] ?? "")
    .split(",")
    .map((pin) => pin.trim().replace(/\[.*$/, ""))
    .filter((pin) => /^[A-Za-z_]\w*$/.test(pin));
}

/** `nand2tetris.evalChip`: sets inputs, evaluates, and prints the chip's pins. */
export async function evalChip(n2t: () => string | undefined, output: vscode.OutputChannel, arg: unknown): Promise<void> {
  const uri = targetUri(arg);
  if (!uri?.fsPath.endsWith(".hdl")) return;
  const binary = n2t();
  if (!binary) return;
  await vscode.workspace.saveAll(false);

  const document = await vscode.workspace.openTextDocument(uri);
  const defaults = inputPins(document.getText()).map((pin) => `${pin}=0`).join(" ");
  const input = await vscode.window.showInputBox({
    title: `Evaluate ${path.basename(uri.fsPath)}`,
    prompt: "Input values (decimal, 0b… or 0x…), optionally followed by --cycles N",
    value: defaults,
  });
  if (input === undefined) return;

  const args = ["eval", uri.fsPath, ...input.split(/\s+/).filter(Boolean)];
  const result = await runProcess(binary, args, path.dirname(uri.fsPath));
  output.appendLine(`$ n2t ${args.slice(1).map((a) => path.basename(a)).join(" ")}`);
  output.appendLine((result.stdout + result.stderr).trimEnd() || result.error?.message || "");
  output.appendLine("");
  output.show(true);
}

/** `nand2tetris.checkFolder`: runs `n2t check` on a folder as a task. */
export async function checkFolder(n2t: () => string | undefined, arg: unknown): Promise<void> {
  const binary = n2t();
  if (!binary) return;
  let folder = targetUri(arg);
  if (folder && !fs.statSync(folder.fsPath, { throwIfNoEntry: false })?.isDirectory()) {
    folder = vscode.Uri.joinPath(folder, "..");
  }
  folder ??= vscode.workspace.workspaceFolders?.[0]?.uri;
  if (!folder) return;
  const scope = vscode.workspace.getWorkspaceFolder(folder) ?? vscode.TaskScope.Workspace;
  await vscode.tasks.executeTask(
    createTask(binary, { type: TASK_TYPE, command: "check", path: folder.fsPath }, scope, folder.fsPath),
  );
}
