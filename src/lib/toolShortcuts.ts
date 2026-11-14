// 注釈ツールを 1 文字のキーで切り替える (CYBERNEURA-DEV-1265)
//
// キー → ツールの対応と「今このキーに反応してよいか」の判定。DOM に触らない形にして
// tests/tool-shortcuts.test.mts から直接呼ぶ。実際の切り替えは +page.svelte が
// ツールバーのボタンと同じ toggle…Tool を呼ぶ (もう一度押すと解除、というトグルのまま)。

export type ShortcutTool = "arrow" | "rect" | "ellipse" | "highlight" | "step" | "text" | "mask" | "crop";

/** キー (小文字) → ツール。ツールバーの並び順 */
export const TOOL_SHORTCUTS: readonly { key: string; tool: ShortcutTool }[] = [
  { key: "a", tool: "arrow" },
  { key: "r", tool: "rect" },
  { key: "e", tool: "ellipse" },
  { key: "h", tool: "highlight" },
  { key: "n", tool: "step" },
  { key: "t", tool: "text" },
  { key: "m", tool: "mask" },
  { key: "c", tool: "crop" },
];

/** ツールチップに併記するキーの表記 (例: "A")。修飾キーが無いので OS で変わらない */
export function toolShortcutLabel(tool: ShortcutTool): string {
  const entry = TOOL_SHORTCUTS.find((s) => s.tool === tool);
  return entry ? entry.key.toUpperCase() : "";
}

export interface ToolKeyEvent {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  /** IME の変換中 */
  isComposing: boolean;
  /** 押しっぱなしの自動リピート */
  repeat: boolean;
}

export interface ToolKeyTarget {
  tagName?: string;
  isContentEditable?: boolean;
}

/**
 * フォーカスがフォーム部品にあるか。文字を打つ欄 (テキスト注釈の textarea、パス欄) だけでなく、
 * `<select>` (文字キーで選択肢へ飛ぶ) と他の `<input>` も含める。ここで反応すると、
 * 太さの select にフォーカスがある時の "T" が選択肢の移動とツールの切り替えの両方に効く。
 */
export function isFormControlTarget(target: ToolKeyTarget | null | undefined): boolean {
  if (!target) return false;
  if (target.isContentEditable) return true;
  const tag = (target.tagName ?? "").toUpperCase();
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

export interface ToolKeyContext {
  /** 画像が開かれている (動画モードではない) */
  hasImage: boolean;
  /** 撮影中・録画中 */
  busy: boolean;
}

/**
 * キー入力がどのツールの切り替えに当たるか。当たらなければ null。
 *
 * 反応しないのは: 修飾キー (⌘ / Ctrl / Alt / Shift) が押されている (⌘C などの既存の
 * ショートカットを素通しする)、フォーカスがフォーム部品にある、IME の変換中、
 * キーの自動リピート (押しっぱなしでトグルが点滅する)、画像が開かれていない、撮影中・録画中。
 */
export function toolForKey(
  e: ToolKeyEvent,
  target: ToolKeyTarget | null | undefined,
  context: ToolKeyContext,
): ShortcutTool | null {
  if (!context.hasImage || context.busy) return null;
  if (e.metaKey || e.ctrlKey || e.altKey || e.shiftKey) return null;
  // 変換中の keydown は isComposing が立つか、key が "Process" になる (WebView2)
  if (e.isComposing || e.key === "Process" || e.repeat) return null;
  if (isFormControlTarget(target)) return null;
  // CapsLock 中は Shift 無しでも大文字で届く
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  return TOOL_SHORTCUTS.find((s) => s.key === key)?.tool ?? null;
}
