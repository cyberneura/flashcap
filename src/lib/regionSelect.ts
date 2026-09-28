// Windows の撮影範囲選択 (src/routes/capture-region/+page.svelte) の幾何計算。
// DOM もマウスも要らない形にして、tests/region-select.test.mts から直接呼べるようにしてある。

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** 押してから離すまでにこれ未満しか動いていなければ、範囲を引いたのではなくクリック */
export const CLICK_SLOP = 2;

function clamp(v: number, min: number, max: number): number {
  return Math.min(Math.max(v, min), max);
}

/**
 * ドラッグの始点と現在点から矩形を作る (どちら向きに引いても幅・高さは正)。
 * 画面 (オーバーレイ) の外へ出たポインタは端に寄せる。
 */
export function dragRect(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  viewWidth: number,
  viewHeight: number,
): Rect {
  const x1 = clamp(Math.min(ax, bx), 0, viewWidth);
  const y1 = clamp(Math.min(ay, by), 0, viewHeight);
  const x2 = clamp(Math.max(ax, bx), 0, viewWidth);
  const y2 = clamp(Math.max(ay, by), 0, viewHeight);
  return { x: x1, y: y1, width: x2 - x1, height: y2 - y1 };
}

/** 範囲を引いたとみなせるだけ動いたか (どちらかの軸で CLICK_SLOP 以上) */
export function isDrag(ax: number, ay: number, bx: number, by: number): boolean {
  return Math.abs(bx - ax) >= CLICK_SLOP || Math.abs(by - ay) >= CLICK_SLOP;
}

/**
 * 画面 (CSS px) 上の矩形を、撮影画像のピクセル単位に直す。
 *
 * オーバーレイは撮影画像をウインドウいっぱいに引き伸ばして映しているので、
 * 倍率は軸ごとに「画像の寸法 / 表示の寸法」。高 DPI のモニターでは 1 CSS px が
 * 複数の画像ピクセルに当たるので、端数は外側へ丸めて、選んだ範囲を欠かさない。
 * 画像からはみ出した分は削る。Rust (region_capture.rs の crop_bounds) も同じ丸めを
 * もう一度行うので、ここで丸めるのは表示するサイズ (幅 x 高さ) を合わせるため。
 */
export function toImageRect(
  rect: Rect,
  viewWidth: number,
  viewHeight: number,
  imageWidth: number,
  imageHeight: number,
): Rect {
  if (viewWidth <= 0 || viewHeight <= 0) {
    return { x: 0, y: 0, width: 0, height: 0 };
  }
  const sx = imageWidth / viewWidth;
  const sy = imageHeight / viewHeight;
  const left = clamp(Math.floor(rect.x * sx), 0, imageWidth);
  const top = clamp(Math.floor(rect.y * sy), 0, imageHeight);
  const right = clamp(Math.ceil((rect.x + rect.width) * sx), 0, imageWidth);
  const bottom = clamp(Math.ceil((rect.y + rect.height) * sy), 0, imageHeight);
  return { x: left, y: top, width: right - left, height: bottom - top };
}
