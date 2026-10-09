// 窓ごとの capability (src-tauri/capabilities/*.json) が、その窓のページが invoke している
// アプリ独自コマンドとちょうど一致しているかを確かめる。
//
// build.rs で app manifest を宣言しているので、capability で許可していないコマンドを
// invoke すると ACL で拒否される (= その窓の機能が黙って壊れる)。逆に使っていない
// コマンドを許可すると、窓ごとに絞っている意味が無くなる。どちらも実機を触らないと
// 気付けないので、ページのソースから invoke を拾って突き合わせる。
import assert from "node:assert/strict";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join, posix } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
// リポジトリ内のパスは OS を問わず "/" 区切りで扱う (CI は Windows でも走る)
const read = (path: string) => readFileSync(join(ROOT, path), "utf8");

// ウインドウのラベル (capability の windows) → そのウインドウが開くページ。
// ラベルと URL は Rust 側で決めている: main は tauri.conf.json、preferences は lib.rs の
// open_preferences、licenses は licenses.rs、region-selector-<n> は video.rs、
// capture-region-<n> は region_capture.rs。ウインドウを足したらここにも足すこと。
const WINDOW_PAGES: Record<string, string> = {
  main: "src/routes/+page.svelte",
  preferences: "src/routes/preferences/+page.svelte",
  licenses: "src/routes/licenses/+page.svelte",
  "region-selector-*": "src/routes/region-select/+page.svelte",
  "capture-region-*": "src/routes/capture-region/+page.svelte",
};

// コマンド名を変数で渡している invoke。ソースからは名前を読めないので、取りうる値を
// ここに書く (件数も固定して、増えたらテストを落とす)。
const DYNAMIC_INVOKES: Record<string, { sites: number; commands: string[] }> = {
  // captureScreen(command) — 既定は通常の撮影、do-capture の payload でタイマー付き
  "src/routes/+page.svelte": {
    sites: 1,
    commands: ["take_screenshot_interactive", "take_screenshot_timer"],
  },
};

/** `$lib/...` と相対パスの import を、実在するファイルへ解決する (パッケージは無視) */
function resolveImport(from: string, spec: string): string | null {
  let base: string;
  if (spec.startsWith("$lib/")) base = posix.join("src/lib", spec.slice("$lib/".length));
  else if (spec.startsWith(".")) base = posix.join(posix.dirname(from), spec);
  else return null;
  for (const candidate of [base, `${base}.ts`, `${base}.svelte`]) {
    if (/\.(ts|svelte)$/.test(candidate) && existsSync(join(ROOT, candidate))) return candidate;
  }
  return null;
}

/** ページと、そこから辿れる src 内のモジュールをすべて集める */
function moduleGraph(entry: string): string[] {
  const seen = new Set<string>();
  const queue = [entry];
  while (queue.length > 0) {
    const file = queue.pop()!;
    if (seen.has(file)) continue;
    seen.add(file);
    const source = read(file);
    for (const m of source.matchAll(/\bfrom\s+["']([^"']+)["']|\bimport\s+["']([^"']+)["']/g)) {
      const resolved = resolveImport(file, m[1] ?? m[2]);
      if (resolved) queue.push(resolved);
    }
  }
  return [...seen].sort();
}

/** ファイル内の invoke を拾う。リテラルはそのまま、変数は DYNAMIC_INVOKES で補う */
function invokedCommands(file: string): Set<string> {
  const source = read(file);
  const commands = new Set<string>();
  let dynamicSites = 0;
  for (const m of source.matchAll(/\binvoke\s*(?:<[^>]*>)?\s*\(\s*([^,)]*)/g)) {
    const arg = m[1].trim();
    const literal = /^["']([a-z_]+)["']$/.exec(arg);
    if (literal) commands.add(literal[1]);
    else dynamicSites++;
  }
  const dynamic = DYNAMIC_INVOKES[file];
  assert.equal(
    dynamicSites,
    dynamic?.sites ?? 0,
    `${file}: コマンド名を変数で渡す invoke の数が DYNAMIC_INVOKES と合わない (取りうる値を書き足すこと)`,
  );
  for (const command of dynamic?.commands ?? []) commands.add(command);
  return commands;
}

function buildRsCommands(): Set<string> {
  const source = read("src-tauri/build.rs");
  const body = /const APP_COMMANDS: &\[&str\] = &\[([\s\S]*?)\];/.exec(source);
  assert.ok(body, "build.rs に APP_COMMANDS が無い");
  return new Set([...body[1].matchAll(/"([a-z_]+)"/g)].map((m) => m[1]));
}

const appCommands = buildRsCommands();
assert.ok(appCommands.has("run_shell_command"), "build.rs の APP_COMMANDS を読めていない");

// ページを足したのに WINDOW_PAGES に載せ忘れると、そのページの invoke を誰も検査しない
const pages = readdirSync(join(ROOT, "src/routes"), { recursive: true })
  .map((p) => String(p).replaceAll("\\", "/"))
  .filter((p) => p.endsWith("+page.svelte"))
  .map((p) => posix.join("src/routes", p))
  .sort();
assert.deepEqual(pages, Object.values(WINDOW_PAGES).sort(), "src/routes のページと WINDOW_PAGES が一致しない");

const capabilityDir = "src-tauri/capabilities";
const coveredWindows = new Set<string>();
for (const name of readdirSync(join(ROOT, capabilityDir)).filter((f) => f.endsWith(".json")).sort()) {
  const capability = JSON.parse(read(posix.join(capabilityDir, name))) as {
    windows: string[];
    permissions: (string | { identifier: string })[];
  };

  // プラグインの permission は "<plugin>:<name>"、アプリ独自のものは接頭辞なしの "allow-<kebab>"
  const allowed = new Set<string>();
  for (const permission of capability.permissions) {
    const id = typeof permission === "string" ? permission : permission.identifier;
    if (id.includes(":")) continue;
    assert.ok(id.startsWith("allow-"), `${name}: アプリ独自コマンドは allow-* だけで許可する (${id})`);
    const command = id.slice("allow-".length).replaceAll("-", "_");
    assert.ok(appCommands.has(command), `${name}: build.rs に無いコマンドを許可している (${id})`);
    allowed.add(command);
  }

  // 窓ごとに比べる。1 つの capability に複数の窓を載せると、許可はその和集合になって
  // 各窓に渡る (Preferences に撮影やシェル実行が付く) ので、和集合で比べてはいけない
  for (const window of capability.windows) {
    const page = WINDOW_PAGES[window];
    assert.ok(page, `${name}: ウインドウ ${window} のページが WINDOW_PAGES に無い`);
    // 複数の capability に載った窓は、それらの許可の和集合を持つ。窓ごとの比較が崩れるので禁止する
    assert.ok(!coveredWindows.has(window), `${name}: ウインドウ ${window} が複数の capability に載っている`);
    coveredWindows.add(window);

    const used = new Set<string>();
    for (const file of moduleGraph(page)) {
      for (const command of invokedCommands(file)) {
        assert.ok(appCommands.has(command), `${file}: build.rs に無いコマンドを invoke している (${command})`);
        used.add(command);
      }
    }

    const missing = [...used].filter((c) => !allowed.has(c)).sort();
    const extra = [...allowed].filter((c) => !used.has(c)).sort();
    assert.deepEqual(missing, [], `${name} (${window}): ページが invoke しているのに許可していない (ACL で拒否されて機能が壊れる)`);
    assert.deepEqual(extra, [], `${name} (${window}): このページは invoke していないのに許可している`);
  }
}

assert.deepEqual(
  [...coveredWindows].sort(),
  Object.keys(WINDOW_PAGES).sort(),
  "capability の付いていないウインドウがある",
);

console.log("capabilities: ok");
