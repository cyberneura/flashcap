// スポットライト (囲んだ範囲の外側を暗くする) の判定 (CYBERNEURA-DEV-1322)
//
// スポットライトは ShapeType の 1 つ (`spotlight`) で、置く・動かす・消す操作は矩形と共有する。
// 描画は「画像全体を半透明の黒で覆い、スポットライトの矩形をくり抜く」1 枚のレイヤーで、
// 画面 (SpotlightLayer) と書き出し (renderComposite) が同じ答えを使うよう、暗くする範囲の
// 計算をここに置く。DOM もキャンバスも要らない形にして tests/spotlight.test.mts から直接呼ぶ。

import type { Shape } from "./types";
import { roundedMaskRect, subtractRects, type Rect } from "./highlight.ts";

/** 暗さ (外側に重ねる黒の不透明度) の既定値 */
export const DEFAULT_SPOTLIGHT_DARKNESS = 0.5;

/** ツールバーで選べる暗さ */
export const SPOTLIGHT_DARKNESS_OPTIONS: readonly number[] = [0.3, 0.5, 0.7, 0.85];

/**
 * 設定ファイルから読んだ暗さを検める。0 より大きく 1 以下の数だけ通し、他は既定値に戻す
 * (0 だと置いても何も見えず、1 を超える値は描画で意味を持たない)。
 */
export function parseSpotlightDarkness(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) && value > 0 && value <= 1
    ? value
    : DEFAULT_SPOTLIGHT_DARKNESS;
}

/**
 * 暗くする範囲を、互いに重ならない整数座標の矩形の並びで返す。
 *
 * 画像全体からスポットライトの矩形 (穴の和集合) とマスクの矩形をくり抜いた残り。
 * - **重ならない矩形に割る**のは、重なった所に黒を二度塗って濃くしないため。
 *   clip-path の evenodd で穴を開ける方式だと、重なった 2 つの穴の重なりが暗く戻る。
 * - **整数に丸める**のは、隣り合う矩形の境目に塗りの継ぎ目 (アンチエイリアスの薄い線) を
 *   出さないため。マスクは書き出しと同じ丸め (`roundedMaskRect`) を使う。
 * - **マスクの範囲は暗くしない** (蛍光ペンと同じくマスクを優先する。ぼかしのマスクは
 *   backdrop-filter で下の絵をぼかすので、暗くした絵が混ざると書き出しと食い違う)。
 *
 * スポットライトが 1 つも無ければ空 (何も暗くしない)。
 */
export function spotlightDarkRects(
  imageWidth: number,
  imageHeight: number,
  shapes: readonly Pick<Shape, "type" | "x" | "y" | "width" | "height">[],
  masks: readonly Rect[],
): Rect[] {
  const spots = shapes
    .filter((s) => s.type === "spotlight")
    .map(roundedMaskRect)
    .filter((r): r is Rect => r !== null);
  if (spots.length === 0) return [];
  const holes = [...spots, ...masks.map(roundedMaskRect).filter((r): r is Rect => r !== null)];
  const full: Rect = { x: 0, y: 0, width: Math.round(imageWidth), height: Math.round(imageHeight) };
  return subtractRects(full, holes);
}

/**
 * 画面のレイヤー (画像と同じ大きさの div) に当てる `clip-path`。暗くする範囲だけを残す。
 * 全部くり抜かれた時は面積の無い path (空の path は無効な値として無視され、全体が暗くなる)。
 */
export function spotlightClipPath(rects: readonly Rect[]): string {
  if (rects.length === 0) return `path("M0 0Z")`;
  const path = rects.map((r) => `M${r.x} ${r.y}h${r.width}v${r.height}h${-r.width}Z`).join("");
  return `path("${path}")`;
}
