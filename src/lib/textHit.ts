import type { TextAnnotation } from "./types";

/**
 * テキスト注釈の当たり判定。
 *
 * 実際に描かれた字面ではなく、字数と font-size からの**推定**の箱で判定する。
 * SVG の `<text>` は測らないと本当の幅が分からず、測るにはレンダリング後の
 * `getBBox()` が要る — マウスが動くたびにそれを呼ぶと、レイアウトの再計算が
 * 1 フレームに何度も入る。推定で足りるのは、掴む対象が「だいたいこの辺の文字」
 * だからで、外れても隣の注釈を掴むだけの距離ではない。
 *
 * ここに置いてあるのは、この判定が **TextOverlay の操作の全部を決めている**ため。
 * どれを選ぶか、どれをダブルクリックで編集するか、カーソルを何にするかが、
 * すべてこの箱で決まる。DOM もマウスも要らない純粋な計算なので、
 * `edgeSnap.ts` / `cropAspect.ts` と同じくテストから直接呼べる場所に出してある。
 */

/** 1 行の高さ。font-size の何倍か */
export const LINE_HEIGHT_RATIO = 1.3;

/** 1 文字の幅の見積もり。font-size の何倍か */
const CHAR_WIDTH_RATIO = 0.6;

/** 箱に足す余白 (自然解像度ピクセル) */
const PADDING = 8;

export interface TextBBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * 注釈を囲む箱。
 *
 * 空文字でも `fontSize * 2` の幅を持つ。入力を始めたばかりの注釈が掴めない
 * (= 選べない、消せない) と、画面に何も無いのに何かがある状態になる。
 *
 * `annotationScale` は描画時に文字へ掛ける倍率 (Retina の撮影なら 2)。`fontSize` は
 * ツールバーの値のまま持っているので、ここで掛けないと箱が字面の半分になる。
 */
export function textBBox(
  t: Pick<TextAnnotation, "x" | "y" | "text" | "fontSize">,
  annotationScale = 1,
): TextBBox {
  const fontSize = t.fontSize * annotationScale;
  const padding = PADDING * annotationScale;
  const lines = t.text.split("\n");
  const lineHeight = fontSize * LINE_HEIGHT_RATIO;
  const width = Math.max(
    ...lines.map((line) => line.length * fontSize * CHAR_WIDTH_RATIO),
    fontSize * 2,
  );
  return {
    x: t.x,
    y: t.y,
    width: width + padding,
    height: lineHeight * lines.length + padding,
  };
}

export function hitsText(
  t: Pick<TextAnnotation, "x" | "y" | "text" | "fontSize">,
  px: number,
  py: number,
  annotationScale = 1,
): boolean {
  const bbox = textBBox(t, annotationScale);
  return px >= bbox.x && px <= bbox.x + bbox.width && py >= bbox.y && py <= bbox.y + bbox.height;
}

/**
 * その点にある注釈のうち、いちばん手前のもの。
 *
 * 後ろに足したものが手前に描かれるので、末尾から探す。重なった 2 つのうち
 * 見えているほうを掴めないと、掴めないほうを消すまで直せない。
 */
export function topmostTextAt<T extends Pick<TextAnnotation, "x" | "y" | "text" | "fontSize">>(
  texts: readonly T[],
  px: number,
  py: number,
  annotationScale = 1,
): T | null {
  for (let i = texts.length - 1; i >= 0; i--) {
    if (hitsText(texts[i], px, py, annotationScale)) return texts[i];
  }
  return null;
}

/**
 * 押下の微小なブレとドラッグを分ける距離 (画面ピクセル)。
 *
 * `CropOverlay` の `CLICK_SLOP` と同じ役目で、同じ値。ダブルクリックの 2 回目の
 * 押下も移動の開始なので、指が 1px 揺れただけで「動かした」と扱うと、編集を
 * 開くたびに文字がわずかにずれて undo が 1 段積まれる。
 */
export const DRAG_SLOP_PX = 2;

/**
 * その移動を、掴んで動かしたと見なしてよいか。
 *
 * `scale` で割るのは、dx / dy が自然解像度の座標だから。画面上の見た目の距離で
 * 判断しないと、縮小表示のときだけ閾値が実質的に大きくなる。
 */
export function exceedsDragSlop(dx: number, dy: number, scale: number): boolean {
  const slop = DRAG_SLOP_PX / (scale || 1);
  return Math.abs(dx) >= slop || Math.abs(dy) >= slop;
}
