// 選択中の注釈の複製 (CYBERNEURA-DEV-1321)
//
// ⌘D / Ctrl+D (+page.svelte) と Option / Alt+ドラッグ (各オーバーレイ) が使う、DOM に触らない部分。
// tests/duplicate.test.mts から直接呼ぶ。

import type { Arrow, Step } from "./types";
import { renumber } from "./steps.ts";
import { isFormControlTarget, type ToolKeyTarget } from "./toolShortcuts.ts";

/** ⌘D で複製した注釈をずらす量 (右下へ。画像 px。Retina の撮影では annotationScale 倍する) */
export const DUPLICATE_OFFSET = 16;

export interface DuplicateKeyEvent {
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

/**
 * ⌘D (Windows は Ctrl+D) か。修飾キーは OS の主キー 1 つだけ (⌘⇧D や Ctrl+Alt+D は別のキー)。
 * 文字入力中・フォーム部品にフォーカスがある時・IME の変換中・自動リピートは反応しない
 * (ツールの 1 文字キーと同じ判定。押しっぱなしで複製が量産されないように)。
 */
export function isDuplicateKey(
  e: DuplicateKeyEvent,
  target: ToolKeyTarget | null | undefined,
  isWindows: boolean,
): boolean {
  const mod = isWindows ? e.ctrlKey && !e.metaKey : e.metaKey && !e.ctrlKey;
  if (!mod || e.altKey || e.shiftKey) return false;
  if (e.isComposing || e.key === "Process" || e.repeat) return false;
  if (isFormControlTarget(target)) return false;
  // CapsLock 中は大文字で届く
  return e.key.toLowerCase() === "d";
}

/** 位置を (x, y) で持つ注釈 (矩形・楕円・蛍光ペン・スポットライト・マスク・テキスト・ステップ) の複製 */
export function offsetCopy<T extends { id: string; x: number; y: number }>(
  source: T,
  dx: number,
  dy: number,
  id: string,
): T {
  return { ...source, id, x: source.x + dx, y: source.y + dy };
}

/** 矢印の複製 (始点と終点を同じだけずらす) */
export function offsetArrowCopy(source: Arrow, dx: number, dy: number, id: string): Arrow {
  return {
    ...source,
    id,
    startX: source.startX + dx,
    startY: source.startY + dy,
    endX: source.endX + dx,
    endY: source.endY + dy,
  };
}

/**
 * ステップを複製した後の配列。複製は末尾に足し、番号を振り直す (= 次の番号になる)。
 * 元のバッジの番号は変わらない。
 */
export function stepsWithDuplicate(steps: readonly Step[], source: Step, dx: number, dy: number, id: string): Step[] {
  return renumber([...steps, offsetCopy(source, dx, dy, id)]);
}
