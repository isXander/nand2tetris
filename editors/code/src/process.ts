import { spawn } from "node:child_process";
import type * as vscode from "vscode";

export interface ProcessResult {
  /** `null` when the process was killed or failed to start. */
  code: number | null;
  stdout: string;
  stderr: string;
  /** Set when the process could not be started. */
  error?: Error;
}

/** Runs a command to completion, killing it if `token` is cancelled. */
export function runProcess(
  command: string,
  args: string[],
  cwd: string,
  token?: vscode.CancellationToken,
  onOutput?: (chunk: string) => void,
): Promise<ProcessResult> {
  return new Promise((resolve) => {
    const child = spawn(command, args, { cwd, env: { ...process.env, NO_COLOR: "1" } });
    let stdout = "";
    let stderr = "";
    const subscription = token?.onCancellationRequested(() => child.kill());
    child.stdout.setEncoding("utf8").on("data", (chunk: string) => {
      stdout += chunk;
      onOutput?.(chunk);
    });
    child.stderr.setEncoding("utf8").on("data", (chunk: string) => {
      stderr += chunk;
      onOutput?.(chunk);
    });
    child.on("error", (error) => {
      subscription?.dispose();
      resolve({ code: null, stdout, stderr, error });
    });
    child.on("close", (code) => {
      subscription?.dispose();
      resolve({ code, stdout, stderr });
    });
  });
}
