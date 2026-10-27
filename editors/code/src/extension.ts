import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";
import { findN2t, requireN2t } from "./binary";
import { createClient } from "./client";
import { checkFolder, compareOutput, evalChip, runTest } from "./commands";
import { N2tTaskProvider, TASK_TYPE } from "./tasks";
import { ScriptTests } from "./testing";

let client: LanguageClient | undefined;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const output = vscode.window.createOutputChannel("Nand2Tetris", { log: true });
  const evalOutput = vscode.window.createOutputChannel("Nand2Tetris: Evaluate");
  context.subscriptions.push(output, evalOutput);

  // Resolved on each use so that changes to `nand2tetris.path` apply immediately.
  const n2t = () => requireN2t(context);
  const n2tQuiet = () => findN2t(context);

  const tests = new ScriptTests(n2t);
  context.subscriptions.push(
    tests,
    vscode.tasks.registerTaskProvider(TASK_TYPE, new N2tTaskProvider(n2tQuiet)),
    vscode.commands.registerCommand("nand2tetris.runTest", (uri) => runTest(tests, uri)),
    vscode.commands.registerCommand("nand2tetris.compareOutput", (uri) => compareOutput(uri)),
    vscode.commands.registerCommand("nand2tetris.evalChip", (uri) => evalChip(n2t, evalOutput, uri)),
    vscode.commands.registerCommand("nand2tetris.checkFolder", (uri) => checkFolder(n2t, uri)),
    vscode.commands.registerCommand("nand2tetris.restartServer", () => restartServer(n2t, output)),
    vscode.commands.registerCommand("nand2tetris.showOutput", () => output.show()),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("nand2tetris.path") || event.affectsConfiguration("nand2tetris.server")) {
        void restartServer(n2t, output);
      }
    }),
  );

  await startServer(n2t, output);
}

async function startServer(n2t: () => string | undefined, output: vscode.LogOutputChannel): Promise<void> {
  if (!vscode.workspace.getConfiguration("nand2tetris").get<boolean>("server.enable", true)) return;
  const command = n2t();
  if (!command) return;
  client = createClient(command, output);
  try {
    await client.start();
  } catch (error) {
    output.appendLine(`Failed to start the language server: ${String(error)}`);
    void vscode.window.showErrorMessage(`Nand2Tetris: the language server failed to start (${command}).`, "Show Output").then(
      (choice) => choice && output.show(),
    );
  }
}

async function restartServer(n2t: () => string | undefined, output: vscode.LogOutputChannel): Promise<void> {
  const previous = client;
  client = undefined;
  await previous?.stop().catch(() => undefined);
  await startServer(n2t, output);
}

export async function deactivate(): Promise<void> {
  await client?.stop();
}
