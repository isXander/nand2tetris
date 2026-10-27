import * as vscode from "vscode";
import {
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions,
} from "vscode-languageclient/node";

export const HDL = "nand2tetris-hdl";
export const TST = "nand2tetris-tst";

export function createClient(command: string, output: vscode.LogOutputChannel): LanguageClient {
  const server = { command, args: ["lsp"] };
  const serverOptions: ServerOptions = { run: server, debug: server };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: HDL },
      { scheme: "file", language: TST },
      { scheme: "untitled", language: HDL },
      { scheme: "untitled", language: TST },
    ],
    synchronize: {
      // Chips resolve against files on disk; let the server know when they change.
      fileEvents: vscode.workspace.createFileSystemWatcher("**/*.{hdl,tst,cmp}"),
    },
    outputChannel: output,
  };
  // The id must match the `nand2tetris.trace.server` setting prefix.
  return new LanguageClient("nand2tetris", "Nand2Tetris Language Server", serverOptions, clientOptions);
}
