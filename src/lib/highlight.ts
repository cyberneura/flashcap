// 蛍光ペン (半透明の塗りの矩形) の判定 (CYBERNEURA-DEV-1264)
//
// 蛍光ペンは ShapeType の 1 つ (`highlight`) で、置く・動かす・消す操作は矩形と共有する。
// ここにあるのは、画面 (ShapeOverlay) と書き出し (renderComposite) が同じ答えを使うための
// 純粋な判定だけ。DOM もキャンバスも要らない形にして tests/highlight.test.mts から直接呼ぶ。

import type { Shape, ShapeType } from "./types";

export const SHAPE_TYPES: readonly ShapeType[] = ["rect", "ellipse", "highlight"];

/** 蛍光ペンの既定色 (黄色) */
export const DEFAULT_HIGHLIGHT_COLOR = "#ffe600";

/**
 * 設定ファイルから読んだ種別を検める。config.json は手で書き換えられうるので、
 * 知らない値は矩形に戻す (知らない種別のまま描画に渡すと何も描かれない図形ができる)。
 */
export function parseShapeType(value: unknown): ShapeType {
  return SHAPE_TYPES.includes(value as ShapeType) ? (value as ShapeType) : "rect";
}

/**
 * 画像の明るさの粗い地図。画像を `width` x `height` の升目に縮めたもので、
 * `luma` は升目ごとの輝度 (0 = 黒、1 = 白) を行優先で並べる。
 */
export interface LumaMap {
  width: number;
  height: number;
  /** 元の画像の幅・高さ (自然解像度 px)。注釈の座標を升目へ写すのに使う */
  imageWidth: number;
  imageHeight: number;
  luma: Float32Array;
}

/** 地図の長辺の升目数。蛍光ペンの判定は「この辺りは明るいか暗いか」だけなので粗くてよい */
export const LUMA_MAP_MAX_SIDE = 256;

/** 画像の寸法から地図の升目数を決める (長辺を LUMA_MAP_MAX_SIDE に収める。拡大はしない) */
export function lumaMapSize(imageWidth: number, imageHeight: number): { width: number; height: number } {
  const ratio = Math.min(1, LUMA_MAP_MAX_SIDE / Math.max(imageWidth, imageHeight, 1));
  return {
    width: Math.max(1, Math.round(imageWidth * ratio)),
    height: Math.max(1, Math.round(imageHeight * ratio)),
  };
}

/** RGBA のバイト列 (ImageData.data) から地図を作る */
export function lumaMapFromRgba(
  rgba: ArrayLike<number>,
  width: number,
  height: number,
  imageWidth: number,
  imageHeight: number,
): LumaMap {
  const luma = new Float32Array(width * height);
  for (let i = 0; i < luma.length; i++) {
    const o = i * 4;
    luma[i] = (0.2126 * rgba[o] + 0.7152 * rgba[o + 1] + 0.0722 * rgba[o + 2]) / 255;
  }
  return { width, height, imageWidth, imageHeight, luma };
}

/**
 * 矩形 (自然解像度 px) の範囲の平均輝度。画像に掛からない範囲は null。
 * 画像からはみ出した分は画像の内側だけで平均する。
 */
export function averageLuma(
  map: LumaMap,
  rect: { x: number; y: number; width: number; height: number },
): number | null {
  if (map.imageWidth <= 0 || map.imageHeight <= 0) return null;
  const sx = map.width / map.imageWidth;
  const sy = map.height / map.imageHeight;
  const left = Math.max(0, Math.floor(rect.x * sx));
  const top = Math.max(0, Math.floor(rect.y * sy));
  const right = Math.min(map.width, Math.ceil((rect.x + rect.width) * sx));
  const bottom = Math.min(map.height, Math.ceil((rect.y + rect.height) * sy));
  if (right <= left || bottom <= top) return null;
  let sum = 0;
  for (let y = top; y < bottom; y++) {
    const row = y * map.width;
    for (let x = left; x < right; x++) sum += map.luma[row + x];
  }
  return sum / ((right - left) * (bottom - top));
}

/** これより暗い範囲では乗算をやめる */
export const DARK_LUMA_THRESHOLD = 0.35;
/** 暗い範囲に重ねる時の不透明度 */
export const DARK_HIGHLIGHT_OPACITY = 0.4;

export interface HighlightPaint {
  /** `multiply` = 乗算 (下の文字が黒いまま残る)。`normal` = 半透明の通常合成 */
  blend: "multiply" | "normal";
  opacity: number;
}

/**
 * 蛍光ペンの重ね方。
 *
 * 明るい地 (白い画面に黒い文字) は乗算: 地が色付き、文字は黒いまま読める。
 * 暗い地 (ダークモードの画面) は乗算だと地がさらに暗くなるだけで色が出ないので、
 * 半透明の通常合成に切り替える。地図がまだ無い (画像のデコード待ち) 間と、
 * 画像に掛からない範囲は乗算。
 *
 * **画面と書き出しの両方がこの関数を通る。** 片方だけ別の判定にすると、見た目と
 * 保存した画像が食い違う。
 */
export function highlightPaint(
  map: LumaMap | null,
  rect: { x: number; y: number; width: number; height: number },
): HighlightPaint {
  const luma = map ? averageLuma(map, rect) : null;
  if (luma !== null && luma < DARK_LUMA_THRESHOLD) {
    return { blend: "normal", opacity: DARK_HIGHLIGHT_OPACITY };
  }
  return { blend: "multiply", opacity: 1 };
}

/**
 * 蛍光ペンの当たり判定。枠線だけの矩形と違って塗りなので、内側のどこを押しても当たる。
 * `margin` は縁の外側に許す遊び (自然解像度 px)。
 */
export function hitsHighlight(
  shape: Pick<Shape, "x" | "y" | "width" | "height">,
  px: number,
  py: number,
  margin: number,
): boolean {
  return (
    px >= shape.x - margin &&
    px <= shape.x + shape.width + margin &&
    py >= shape.y - margin &&
    py <= shape.y + shape.height + margin
  );
}
