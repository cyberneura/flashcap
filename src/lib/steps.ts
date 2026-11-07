import type { Step } from "./types";

/**
 * 番号付きステップ (カウンター) の純粋な計算 (CYBERNEURA-DEV-1209)。DOM もマウスも要らない形にして
 * `tests/steps.test.mts` から直接呼べるようにしてある。
 */

/** 配列の並び順どおりに 1 から番号を振り直す。途中を消しても番号が詰まる */
export function renumber(steps: readonly Step[]): Step[] {
  return steps.map((s, i) => (s.number === i + 1 ? s : { ...s, number: i + 1 }));
}

/** 末尾に足す新しいステップ。番号は「今の件数 + 1」 */
export function nextStep(
  steps: readonly Step[],
  x: number,
  y: number,
  settings: { color: string; size: number; whiteStroke: boolean; dropShadow: boolean },
  id: string = crypto.randomUUID(),
): Step {
  return {
    id,
    x,
    y,
    number: steps.length + 1,
    color: settings.color,
    size: settings.size,
    whiteStroke: settings.whiteStroke,
    dropShadow: settings.dropShadow,
  };
}

/** 点がバッジ (円) の上にあるか。`slack` は画面 px の余裕を画像 px に直したもの */
export function hitsStep(step: Step, px: number, py: number, slack = 0): boolean {
  const r = step.size / 2 + slack;
  const dx = px - step.x;
  const dy = py - step.y;
  return dx * dx + dy * dy <= r * r;
}

/** 点の上にある一番手前 (配列の後ろ) のステップ */
export function topmostStepAt(steps: readonly Step[], px: number, py: number, slack = 0): Step | null {
  for (let i = steps.length - 1; i >= 0; i--) {
    if (hitsStep(steps[i], px, py, slack)) return steps[i];
  }
  return null;
}

/**
 * 数字を円の中心に見せるための、alphabetic baseline から中心までの下げ幅 (フォントサイズ比)。
 * 数字は descender を持たず cap height がおよそ 0.7em なので、baseline を中心より 0.35em 下に
 * 置くと字面の中央が円の中心に来る。SVG (`StepOverlay.svelte`) と canvas (`renderComposite`) の
 * 両方で alphabetic baseline + この下げ幅を使い、エンジンごとに基準が違う `central` / `middle` に
 * 頼らない (画面と書き出しで縦位置をずらさないため)
 */
export const STEP_LABEL_BASELINE_RATIO = 0.35;

/** バッジの中の数字の baseline の y (alphabetic) */
export function stepLabelBaseline(y: number, fontSize: number): number {
  return y + fontSize * STEP_LABEL_BASELINE_RATIO;
}

/** バッジの中の数字のフォントサイズ。2 桁以上は少し縮めて円に収める */
export function stepFontSize(size: number, number: number): number {
  const digits = String(number).length;
  return Math.round(size * (digits >= 3 ? 0.42 : digits === 2 ? 0.5 : 0.58));
}
