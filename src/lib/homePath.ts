// 設定ファイルはマシン間で共有する前提なので、ホームフォルダの実パスを書かない。
// 保存する前に先頭のホームフォルダを `~` に畳む (展開は Rust の config::expand_home)

export function collapseHome(path: string, home: string): string {
  const base = home.replace(/[\\/]+$/, "");
  if (!base) return path;
  if (path === base) return "~";
  for (const sep of ["/", "\\"]) {
    if (path.startsWith(base + sep)) return "~" + sep + path.slice(base.length + 1);
  }
  return path;
}
