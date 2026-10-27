import * as path from "node:path";
import * as vscode from "vscode";

export const TASK_TYPE = "n2t";

interface N2tTaskDefinition extends vscode.TaskDefinition {
  command: "check" | "test";
  path: string;
}

/** Builds an `n2t check|test <path>` task using the `$n2t` problem matcher. */
export function createTask(
  n2t: string,
  definition: N2tTaskDefinition,
  scope: vscode.WorkspaceFolder | vscode.TaskScope = vscode.TaskScope.Workspace,
  cwd?: string,
): vscode.Task {
  const execution = new vscode.ProcessExecution(n2t, [definition.command, definition.path], {
    cwd: cwd ?? (typeof scope === "object" ? scope.uri.fsPath : undefined),
    env: { NO_COLOR: "1" },
  });
  // `asRelativePath` returns its input unchanged for a workspace folder root.
  const relative = vscode.workspace.asRelativePath(definition.path, false);
  const label = relative === definition.path ? path.basename(definition.path) : relative;
  const name = `${definition.command} ${label}`;
  const task = new vscode.Task(definition, scope, name, "n2t", execution, ["$n2t"]);
  task.group = definition.command === "test" ? vscode.TaskGroup.Test : vscode.TaskGroup.Build;
  task.presentationOptions = { reveal: vscode.TaskRevealKind.Always, clear: true };
  return task;
}

export class N2tTaskProvider implements vscode.TaskProvider {
  constructor(private readonly n2t: () => string | undefined) {}

  provideTasks(): vscode.Task[] {
    const n2t = this.n2t();
    if (!n2t) return [];
    return (vscode.workspace.workspaceFolders ?? []).flatMap((folder) =>
      (["check", "test"] as const).map((command) =>
        createTask(n2t, { type: TASK_TYPE, command, path: folder.uri.fsPath }, folder),
      ),
    );
  }

  resolveTask(task: vscode.Task): vscode.Task | undefined {
    const n2t = this.n2t();
    const definition = task.definition as N2tTaskDefinition;
    if (!n2t || !definition.command || !definition.path) return undefined;
    return createTask(n2t, definition, task.scope ?? vscode.TaskScope.Workspace);
  }
}
