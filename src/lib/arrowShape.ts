// 矢印の形 (頭あり / 頭なし = 直線) と、Shift+ドラッグの角度吸着 (CYBERNEURA-DEV-1402)
//
// DOM に触らない部分。ArrowOverlay.svelte (画面) と +page.svelte の renderComposite (書き出し) が
// 同じ判定を使う。tests/arrow-shape.test.mts から直接呼ぶ。

import type { ArrowHead } from "./types";

/** 既定の形。この項目が入る前の設定ファイル・矢印は Skitch 式 (頭が始点側) のまま読む */
export const DEFAULT_ARROW_HEAD: ArrowHead = "start";

/** 設定ファイルの値を形に直す。知らない値・無い値は既定 (頭あり) */
export function parseArrowHead(value: unknown): ArrowHead {
  return value === "none" ? "none" : DEFAULT_ARROW_HEAD;
}

/** 頭を描くか。`head` を持たない古い矢印は頭あり */
export function hasArrowHead(arrow: { head?: ArrowHead }): boolean {
  return arrow.head !== "none";
}

/**
 * 終点を、始点から見て 0 / 45 / 90 度の方向に吸着させた位置。
 *
 * 吸着後の点は「始点と指の位置を対角とする矩形」の中に収める (水平なら y だけ、
 * 垂直なら x だけを始点に揃え、斜めは短い方の移動量に揃える)。長さを射影で決めると
 * 斜めの時に指より遠くへ伸び、画像の中でドラッグしても端点が画像の外に出て
 * つかみ直せなくなるため。始点と同じ点ならそのまま。
 */
export function snapEndpoint(
  fromX: number,
  fromY: number,
  toX: number,
  toY: number,
): { x: number; y: number } {
  const dx = toX - fromX;
  const dy = toY - fromY;
  if (dx === 0 && dy === 0) return { x: toX, y: toY };
  const ax = Math.abs(dx);
  const ay = Math.abs(dy);
  // tan(22.5°) より水平に近ければ水平、tan(67.5°) より垂直に近ければ垂直、それ以外は斜め
  const tan225 = Math.tan(Math.PI / 8);
  if (ay <= ax * tan225) return { x: toX, y: fromY };
  if (ax <= ay * tan225) return { x: fromX, y: toY };
  const d = Math.min(ax, ay);
  return { x: fromX + Math.sign(dx) * d, y: fromY + Math.sign(dy) * d };
}
