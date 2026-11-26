// 注釈エディタの Undo / Redo の履歴 (CYBERNEURA-DEV-1263)
//
// スナップショット方式: 編集の直前の状態を `undo` に積み、Undo で取り出した時は
// 「今の状態」を `redo` に積む。Redo はその逆。新しい編集をしたら `redo` は捨てる
// (Undo した先から別の編集を始めたら、元の続きには戻れない)。
//
// スナップショットの中身 (注釈の配列・土台画像) はここでは見ない。積み方と
// 取り出し方だけを Svelte から切り離して、tests/edit-history.test.mts で検査する。

export interface EditHistory<T> {
  /** 古い順。末尾が直前の状態 */
  undo: T[];
  /** Undo で通り過ぎた状態。末尾が次に Redo で戻る状態 */
  redo: T[];
  /**
   * 最後の Undo / Redo で反映した直後の状態の署名 (`markApplied`)。`redo` が指す「続き」は
   * この状態から先のものなので、今の状態の署名がこれと違えば `redo` は使えない。
   */
  appliedSignature: string | null;
}

export function createEditHistory<T>(): EditHistory<T> {
  return { undo: [], redo: [], appliedSignature: null };
}

/** 新しい編集の直前に呼ぶ。`before` は編集前の状態 */
export function recordEdit<T>(history: EditHistory<T>, before: T): void {
  history.undo.push(before);
  history.redo.length = 0;
  history.appliedSignature = null;
}

/** Undo / Redo で取り出したスナップショットを反映した直後に、その状態の署名を控える */
export function markApplied<T>(history: EditHistory<T>, signature: string): void {
  history.appliedSignature = signature;
}

/**
 * Undo / Redo の前に呼ぶ。最後の Undo / Redo の後に、`recordEdit` を通らない編集
 * (選択中の注釈の色をツールバーから変える等。Undo には積まない) で状態が変わっていたら、
 * Redo の履歴を捨てる。捨てないと、Redo がその編集を黙って消して古い続きへ進む。
 */
export function dropStaleRedo<T>(history: EditHistory<T>, signature: string): void {
  if (history.redo.length > 0 && history.appliedSignature !== signature) {
    history.redo.length = 0;
  }
}

/**
 * Undo。戻す先のスナップショットを返す (履歴が空なら null)。
 * `current` は呼び出し時点の状態で、Redo で戻れるように積む。
 * 履歴が空の時は `current` を呼ばない (スナップショットの複製を無駄に作らない)。
 */
export function stepUndo<T>(history: EditHistory<T>, current: () => T): T | null {
  const entry = history.undo.pop();
  if (entry === undefined) return null;
  history.redo.push(current());
  return entry;
}

/** Redo。Undo の逆向き */
export function stepRedo<T>(history: EditHistory<T>, current: () => T): T | null {
  const entry = history.redo.pop();
  if (entry === undefined) return null;
  history.undo.push(current());
  return entry;
}

/** 別の画像を開いた時など、履歴を持ち越せない時に呼ぶ */
export function clearEditHistory<T>(history: EditHistory<T>): void {
  history.undo.length = 0;
  history.redo.length = 0;
  history.appliedSignature = null;
}

export type HistoryAction = "undo" | "redo";

export interface HistoryKeyEvent {
  key: string;
  /** ショートカットの修飾キー (macOS は ⌘、Windows は Ctrl) */
  mod: boolean;
  shift: boolean;
  alt: boolean;
}

/**
 * キー入力が Undo / Redo のどちらに当たるか。
 *
 * - mod+Z = Undo、mod+Shift+Z = Redo。Shift を押すと Windows の `e.key` は大文字に
 *   なるので小文字に揃えて比べる
 * - Windows は Ctrl+Y も Redo (macOS の ⌘Y は別の意味で使われるので割り当てない)
 * - Alt 付きは別のショートカットとして素通しする
 */
export function historyActionForKey(e: HistoryKeyEvent, windows: boolean): HistoryAction | null {
  if (!e.mod || e.alt) return null;
  const key = e.key.toLowerCase();
  if (key === "z") return e.shift ? "redo" : "undo";
  if (windows && key === "y" && !e.shift) return "redo";
  return null;
}

/** 文字を打ち込む `<input>` の type。range / color / checkbox などは含めない */
const TEXT_INPUT_TYPES = new Set([
  "", "text", "search", "url", "tel", "email", "password", "number",
]);

export interface EditableTarget {
  tagName?: string;
  isContentEditable?: boolean;
  type?: string;
}

/**
 * フォーカスが文字入力欄にあるか。入力欄にある間の ⌘Z / ⌘⇧Z は入力欄自身の
 * Undo / Redo (WebView / ネイティブの Edit メニュー) に任せ、注釈の履歴を動かさない。
 *
 * スライダーや色選択の `<input>` は文字入力ではないので false を返す。これらは
 * 操作した後もフォーカスが残るため、true にすると太さを変えた直後の ⌘Z が効かなくなる。
 */
export function isTextEntryTarget(target: EditableTarget | null | undefined): boolean {
  if (!target) return false;
  if (target.isContentEditable) return true;
  const tag = (target.tagName ?? "").toUpperCase();
  if (tag === "TEXTAREA") return true;
  if (tag === "INPUT") return TEXT_INPUT_TYPES.has((target.type ?? "").toLowerCase());
  return false;
}
