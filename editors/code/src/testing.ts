import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as vscode from "vscode";
import { runProcess } from "./process";

const EXCLUDE = "**/{node_modules,target,.git}/**";

/** One `.tst` script, run with `n2t test`. */
export class ScriptTests implements vscode.Disposable {
  readonly controller: vscode.TestController;
  private readonly disposables: vscode.Disposable[] = [];

  constructor(private readonly n2t: () => string | undefined) {
    this.controller = vscode.tests.createTestController("nand2tetris", "Nand2Tetris");
    this.controller.resolveHandler = async (item) => {
      if (!item) await this.discover();
    };
    this.controller.refreshHandler = async () => {
      this.controller.items.replace([]);
      await this.discover();
    };

    const profile = this.controller.createRunProfile(
      "Run",
      vscode.TestRunProfileKind.Run,
      (request, token) => this.run(request, token),
      true,
    );
    profile.supportsContinuousRun = true;

    const watcher = vscode.workspace.createFileSystemWatcher("**/*.tst");
    watcher.onDidCreate((uri) => void this.add(uri));
    watcher.onDidChange((uri) => void this.add(uri));
    watcher.onDidDelete((uri) => this.remove(uri));
    this.disposables.push(watcher, this.controller);
  }

  dispose(): void {
    for (const d of this.disposables) d.dispose();
  }

  private async discover(): Promise<void> {
    const files = await vscode.workspace.findFiles("**/*.tst", EXCLUDE);
    await Promise.all(files.map((uri) => this.add(uri)));
  }

  /** The folder item that groups scripts in the same directory. */
  private folderItem(uri: vscode.Uri): vscode.TestItem {
    const dir = vscode.Uri.joinPath(uri, "..");
    const id = dir.toString();
    let folder = this.controller.items.get(id);
    if (!folder) {
      const label = vscode.workspace.asRelativePath(dir, true) || path.basename(dir.fsPath);
      folder = this.controller.createTestItem(id, label, dir);
      this.controller.items.add(folder);
    }
    return folder;
  }

  /** Adds (or refreshes) the test item for a script. */
  async add(uri: vscode.Uri): Promise<vscode.TestItem> {
    const folder = this.folderItem(uri);
    let item = folder.children.get(uri.toString());
    if (!item) {
      item = this.controller.createTestItem(uri.toString(), path.basename(uri.fsPath), uri);
      folder.children.add(item);
    }
    item.range = await loadRange(uri);
    return item;
  }

  private remove(uri: vscode.Uri): void {
    const folder = this.controller.items.get(vscode.Uri.joinPath(uri, "..").toString());
    folder?.children.delete(uri.toString());
    if (folder && folder.children.size === 0) this.controller.items.delete(folder.id);
  }

  /** Runs the given scripts (all when empty), showing results in the Test Explorer. */
  async runFiles(uris: vscode.Uri[]): Promise<void> {
    const items = await Promise.all(uris.map((uri) => this.add(uri)));
    const source = new vscode.CancellationTokenSource();
    await this.run(new vscode.TestRunRequest(items), source.token);
    source.dispose();
  }

  private async run(request: vscode.TestRunRequest, token: vscode.CancellationToken): Promise<void> {
    if (request.continuous) {
      return this.watchAndRun(request, token);
    }
    const queue = this.collect(request);
    const run = this.controller.createTestRun(request);
    for (const item of queue) run.enqueued(item);
    for (const item of queue) {
      if (token.isCancellationRequested) {
        run.skipped(item);
        continue;
      }
      await this.runOne(item, run, token);
    }
    run.end();
  }

  /** Re-runs affected scripts whenever chips, scripts or compare files in their folders are saved. */
  private watchAndRun(request: vscode.TestRunRequest, token: vscode.CancellationToken): Promise<void> {
    const watched = this.collect(request);
    const listener = vscode.workspace.onDidSaveTextDocument((document) => {
      if (!/\.(hdl|tst|cmp)$/i.test(document.fileName)) return;
      const dir = path.dirname(document.fileName);
      const affected = watched.filter((item) => item.uri && path.dirname(item.uri.fsPath) === dir);
      if (affected.length > 0) {
        void this.run(new vscode.TestRunRequest(affected, undefined, request.profile), token);
      }
    });
    return new Promise((resolve) => {
      token.onCancellationRequested(() => {
        listener.dispose();
        resolve();
      });
    });
  }

  /** Expands folder items into scripts, honouring exclusions. */
  private collect(request: vscode.TestRunRequest): vscode.TestItem[] {
    const excluded = new Set((request.exclude ?? []).map((i) => i.id));
    const roots: vscode.TestItem[] = [];
    if (request.include) {
      roots.push(...request.include);
    } else {
      this.controller.items.forEach((item) => roots.push(item));
    }
    const out: vscode.TestItem[] = [];
    const visit = (item: vscode.TestItem) => {
      if (excluded.has(item.id)) return;
      if (item.children.size === 0) {
        if (item.uri?.fsPath.endsWith(".tst") && !out.includes(item)) out.push(item);
      } else {
        item.children.forEach(visit);
      }
    };
    roots.forEach(visit);
    return out;
  }

  private async runOne(item: vscode.TestItem, run: vscode.TestRun, token: vscode.CancellationToken): Promise<void> {
    const uri = item.uri;
    const n2t = this.n2t();
    if (!uri || !n2t) {
      run.errored(item, new vscode.TestMessage("Could not find the `n2t` executable (see the nand2tetris.path setting)."));
      return;
    }
    const config = vscode.workspace.getConfiguration("nand2tetris.test", uri);
    const args = ["test", uri.fsPath, "--max-commands", String(config.get<number>("maxCommands", 10_000_000))];
    if (config.get<boolean>("requireCompare")) args.push("--require-compare");
    if (config.get<boolean>("strictPauses")) args.push("--strict-pauses");

    run.started(item);
    const started = Date.now();
    const cwd = path.dirname(uri.fsPath);
    const result = await runProcess(n2t, args, cwd, token, (chunk) =>
      run.appendOutput(chunk.replace(/\r?\n/g, "\r\n"), undefined, item),
    );
    const duration = Date.now() - started;

    if (result.error) {
      run.errored(item, new vscode.TestMessage(`Failed to start \`${n2t}\`: ${result.error.message}`), duration);
    } else if (token.isCancellationRequested) {
      run.skipped(item);
    } else if (result.code === 0) {
      run.passed(item, duration);
    } else if (result.code === 1) {
      run.failed(item, failureMessages(result.stdout, result.stderr, cwd, uri), duration);
    } else {
      const text = (result.stderr || result.stdout).trim() || `n2t exited with code ${result.code}`;
      run.errored(item, new vscode.TestMessage(text), duration);
    }
  }
}

/** The range of a script's first `load` command (for gutter run buttons). */
async function loadRange(uri: vscode.Uri): Promise<vscode.Range> {
  try {
    const lines = (await fs.readFile(uri.fsPath, "utf8")).split(/\r?\n/);
    const index = lines.findIndex((line) => /^\s*load\b/i.test(line));
    const line = Math.max(index, 0);
    return new vscode.Range(line, 0, line, lines[line]?.length ?? 0);
  } catch {
    return new vscode.Range(0, 0, 0, 0);
  }
}

/**
 * Turns `n2t test` output into test messages: comparison failures become
 * expected/actual diffs; `--> file:line:col` lines become locations.
 */
function failureMessages(stdout: string, stderr: string, cwd: string, script: vscode.Uri): vscode.TestMessage[] {
  const messages: vscode.TestMessage[] = [];
  const blocks = stderr.split(/\n(?=(?:error|warning)[:[])/);
  for (const block of blocks) {
    const text = block.trim();
    if (!text.startsWith("error")) continue;
    const title = text.split("\n")[0]?.replace(/^error(\[[^\]]+\])?:\s*/, "") ?? text;
    const expected = /^\s*expected:\s?(.*)$/m.exec(text)?.[1];
    const actual = /^\s*actual:\s+(.*)$/m.exec(text)?.[1];
    const message =
      expected !== undefined && actual !== undefined
        ? vscode.TestMessage.diff(title.split(":")[0] ?? title, expected, actual)
        : new vscode.TestMessage(text);
    const location = /-->\s+(.*?):(\d+):(\d+)\s*$/m.exec(text);
    if (location?.[1] && location[2] && location[3]) {
      const file = path.isAbsolute(location[1]) ? location[1] : path.join(cwd, location[1]);
      const position = new vscode.Position(Number(location[2]) - 1, Number(location[3]) - 1);
      message.location = new vscode.Location(vscode.Uri.file(file), position);
    } else {
      message.location = new vscode.Location(script, new vscode.Position(0, 0));
    }
    messages.push(message);
  }
  if (messages.length === 0) {
    messages.push(new vscode.TestMessage((stderr || stdout).trim() || "The script failed."));
  }
  return messages;
}
