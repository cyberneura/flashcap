import assert from "node:assert/strict";
import { parseShellCommands, runnableShellCommands, shellCommandLabel } from "../src/lib/shellCommands.ts";

{
  // 設定ファイルの値をそのまま読む
  const parsed = parseShellCommands([
    { id: "a", name: "Upload", shell: "/bin/zsh", command: "aws s3 cp \"${IMAGE_PATH}\" s3://b/" },
  ]);
  assert.deepEqual(parsed, [
    { id: "a", name: "Upload", shell: "/bin/zsh", command: "aws s3 cp \"${IMAGE_PATH}\" s3://b/" },
  ]);
}

{
  // 手で壊された要素は捨て、name が無いものは空文字で補う
  const parsed = parseShellCommands([
    null,
    "echo",
    { id: "x", shell: "/bin/sh" },
    { id: 1, shell: "/bin/sh", command: "true" },
    { id: "ok", shell: "/bin/sh", command: "true" },
  ]);
  assert.deepEqual(parsed, [{ id: "ok", name: "", shell: "/bin/sh", command: "true" }]);
}

{
  // 配列でなければ空
  assert.deepEqual(parseShellCommands(undefined), []);
  assert.deepEqual(parseShellCommands({ id: "a" }), []);
}

{
  // コマンドやシェルが空のものはメニューに出さない
  const commands = [
    { id: "1", name: "a", shell: "/bin/sh", command: "  " },
    { id: "2", name: "b", shell: "", command: "true" },
    { id: "3", name: "c", shell: "/bin/sh", command: "true" },
  ];
  assert.deepEqual(runnableShellCommands(commands).map((c) => c.id), ["3"]);
}

{
  assert.equal(shellCommandLabel({ id: "1", name: "  ", shell: "/bin/sh", command: "x" }), "Untitled command");
  assert.equal(shellCommandLabel({ id: "1", name: "Upload", shell: "/bin/sh", command: "x" }), "Upload");
}

console.log("shell-commands: ok");
