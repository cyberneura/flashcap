// 実行中の OS に応じた振る舞いの切り替え
//
// FlashCap は macOS と Windows で配布している。録画 (screencapture -v) と OCR
// (Vision Framework) は macOS にしか無く、ショートカットの修飾キーも違う。
//
// 判定は WebView の UA で行う。Windows の WebView2 は UA に "Windows" を含み、
// macOS の WKWebView は含まない。バックエンドに問い合わせないのは、描画の最初から
// (onMount の await を待たずに) 同期で使いたいため。

/** Windows の WebView2 の中で動いているか */
export const isWindows =
  typeof navigator !== "undefined" && /\bWindows\b/.test(navigator.userAgent);

/** ショートカットの修飾キー (macOS は ⌘、Windows は Ctrl) が押されているか */
export function isModKey(e: KeyboardEvent): boolean {
  return isWindows ? e.ctrlKey : e.metaKey;
}

/** ツールチップに出すショートカットの表記 (例: ⌘⇧C / Ctrl+Shift+C) */
export function shortcutLabel(key: string, { shift = false } = {}): string {
  if (isWindows) {
    return `Ctrl+${shift ? "Shift+" : ""}${key}`;
  }
  return `⌘${shift ? "⇧" : ""}${key}`;
}
