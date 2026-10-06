// シェルコマンド (Preferences で登録し、ツールバーから画像に対して実行する)。
// 実行は src-tauri/src/shell_command.rs

export interface ShellCommand {
  id: string;
  name: string;
  shell: string;
  command: string;
}

export const SHELL_COMMANDS_KEY = "shell_commands";

export const SHELL_PRESETS = ["/bin/zsh", "/bin/bash", "/bin/sh"];

export interface ShellRunResult {
  success: boolean;
  exit_code: number | null;
  log_path: string;
}

/** 設定ファイルの値を読む。手で書き換えられうるので、形の合わない要素は捨てる */
export function parseShellCommands(value: unknown): ShellCommand[] {
  if (!Array.isArray(value)) return [];
  const commands: ShellCommand[] = [];
  for (const item of value) {
    if (typeof item !== "object" || item === null) continue;
    const { id, name, shell, command } = item as Record<string, unknown>;
    if (typeof id !== "string" || typeof shell !== "string" || typeof command !== "string") continue;
    commands.push({ id, name: typeof name === "string" ? name : "", shell, command });
  }
  return commands;
}

/** ツールバーのメニューに出すもの。コマンドが空のものは押しても何も起きないので出さない */
export function runnableShellCommands(commands: ShellCommand[]): ShellCommand[] {
  return commands.filter((c) => c.command.trim() !== "" && c.shell.trim() !== "");
}

export function shellCommandLabel(command: ShellCommand): string {
  return command.name.trim() || "Untitled command";
}
