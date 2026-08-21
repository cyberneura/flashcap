import type { CropRect } from "./types";

/**
 * トリミング枠の最小サイズ (自然解像度ピクセル)。
 * 縦横比を固定していると片方の軸だけ縮むことはないので、両軸ともこれを下回らせない。
 */
export const MIN_CROP_SIZE = 8;

export interface Size {
  width: number;
  height: number;
}

/** 選べる縦横比。値は width / height */
export const ASPECT_SQUARE = 1;
export const ASPECT_WIDESCREEN = 16 / 9;

/**
 * 縦横比を保ったまま、w / h の**大きい方**に合わせた寸法を返す。
 *
 * 小さい方に合わせる (contain) と、対角線から外れた方向へポインタを動かした時に枠が
 * 縮んで追従しなくなる。大きい方に合わせれば、少なくとも片方の軸はポインタに追いつく。
 */
function coverAspect(width: number, height: number, aspect: number): Size {
  const w = Math.max(width, 0);
  const h = Math.max(height, 0);
  if (w <= 0 && h <= 0) return { width: 0, height: 0 };
  if (w / aspect >= h) return { width: w, height: w / aspect };
  return { width: h * aspect, height: h };
}

/**
 * これ以下の余地は「無い」とみなす。
 * largestAspectRect が返す寸法には丸め誤差が乗る (16:9 の 1600px 幅が
 * 1599.9999999999998 になるなど) ので、厳密な 0 比較では取りこぼす。
 */
const ROOM_EPSILON = 1e-6;

/** 縦横比を保ったまま maxW × maxH に収める (元から収まっていれば何もしない) */
function fitWithin(size: Size, maxW: number, maxH: number): Size {
  if (size.width <= 0 || size.height <= 0) return size;
  // 負の余地を渡されても寸法を負にしない
  const k = Math.min(1, Math.max(0, maxW) / size.width, Math.max(0, maxH) / size.height);
  return { width: size.width * k, height: size.height * k };
}

/**
 * 1 軸ぶんの伸ばす向きを決める。
 *
 * 素直にポインタの向きへ倒すと、**掴んだ角が画像の境界に乗っている時にその向きの余地が
 * 0 になり、枠が 0 に潰れる** (比率が NaN になり、mouseup の復元は draw 経路にしか
 * 無いので戻らない)。画像の外へドラッグすればポインタは境界にクランプされて anchor と
 * 一致するので、これは日常的に踏む。余地が無い向きは選ばない。
 */
function chooseDirection(
  anchor: number,
  pointer: number,
  limit: number
): { forward: boolean; room: number } {
  const forwardRoom = limit - anchor;
  const backwardRoom = anchor;
  let forward = pointer >= anchor;
  if ((forward ? forwardRoom : backwardRoom) <= ROOM_EPSILON) forward = !forward;
  return { forward, room: Math.max(0, forward ? forwardRoom : backwardRoom) };
}

/**
 * 縦横比を保ったまま最小サイズまで広げ、そのうえで maxW × maxH に収める。
 *
 * **収める方を後にする。** 画像が最小サイズの枠すら入らないほど小さい場合、
 * 広げる方を優先すると枠が画像からはみ出す。入らないなら入る範囲で妥協する。
 *
 * **余地が完全に無い (maxW / maxH が 0) 場合は 0 を返すしかない。** 呼び出し側で
 * その向きを選ばないようにするのが本筋 (chooseDirection)。ここは最後の砦で、
 * 0 を返した結果を枠として採用しないのは CropOverlay の責任。
 */
function atLeastMin(size: Size, aspect: number, maxW: number, maxH: number): Size {
  const base =
    size.width > 0 && size.height > 0
      ? size
      : coverAspect(MIN_CROP_SIZE, MIN_CROP_SIZE, aspect);
  const k = Math.max(1, MIN_CROP_SIZE / base.width, MIN_CROP_SIZE / base.height);
  return fitWithin({ width: base.width * k, height: base.height * k }, maxW, maxH);
}

/** 画像いっぱいに取れる、その縦横比の最大の枠 (中央寄せ) */
export function largestAspectRect(aspect: number, bounds: Size): CropRect {
  const size = fitWithin(
    coverAspect(bounds.width, bounds.height, aspect),
    bounds.width,
    bounds.height
  );
  return {
    x: (bounds.width - size.width) / 2,
    y: (bounds.height - size.height) / 2,
    width: size.width,
    height: size.height,
  };
}

/**
 * 既にある枠を縦横比へ合わせ直す (中心を保つ)。
 *
 * 縦横比ボタンを押した瞬間に呼ぶ。面積が近い方が違和感が無いので、大きい方ではなく
 * **小さい方**の軸に合わせる — 大きい方に合わせると枠が画像からはみ出して、
 * 収めるために結局縮むうえ、中心もずれる。
 */
export function refitToAspect(rect: CropRect, aspect: number, bounds: Size): CropRect {
  const centerX = rect.x + rect.width / 2;
  const centerY = rect.y + rect.height / 2;

  const byWidth = { width: rect.width, height: rect.width / aspect };
  const byHeight = { width: rect.height * aspect, height: rect.height };
  const contained = byWidth.height <= rect.height ? byWidth : byHeight;

  const size = atLeastMin(
    fitWithin(contained, bounds.width, bounds.height),
    aspect,
    bounds.width,
    bounds.height
  );
  return {
    x: clamp(centerX - size.width / 2, 0, bounds.width - size.width),
    y: clamp(centerY - size.height / 2, 0, bounds.height - size.height),
    width: size.width,
    height: size.height,
  };
}

/**
 * 固定した角 (anchor) からポインタへ向けて、縦横比どおりの枠を作る。
 *
 * 範囲の引き直し (anchor = ドラッグ開始点) と、角ハンドルのリサイズ
 * (anchor = 対角の角) の両方がこれ 1 つで表せる。anchor は動かないので、
 * 画像に収める時も anchor 側は縮まない。
 *
 * **ポインタが anchor を越えると枠は反対側へ反転する。** 引き直しではこれが必須
 * (左上へ向かって引ける必要がある)。角ハンドルのリサイズでは自由モードが MIN_SIZE で
 * 止まるのと挙動が違うが、反転しても anchor は角として保たれ、ポインタを戻せば
 * そのまま戻るので、共用をやめてまで揃える価値は無いと判断している。
 * **辺ハンドル (aspectRectFromEdge) は反転させない** — あちらは引き直しと共用しておらず、
 * 反転するとポインタと逆向きに枠が伸びるだけで得るものが無い。
 */
export function aspectRectFromCorner(
  anchor: { x: number; y: number },
  pointer: { x: number; y: number },
  aspect: number,
  bounds: Size
): CropRect {
  // anchor から伸ばせる余地。これを超えないよう縦横比のまま縮める
  const horizontal = chooseDirection(anchor.x, pointer.x, bounds.width);
  const vertical = chooseDirection(anchor.y, pointer.y, bounds.height);
  const towardRight = horizontal.forward;
  const towardBottom = vertical.forward;
  const availableW = horizontal.room;
  const availableH = vertical.room;

  const wanted = coverAspect(
    Math.abs(pointer.x - anchor.x),
    Math.abs(pointer.y - anchor.y),
    aspect
  );
  const size = atLeastMin(
    fitWithin(wanted, availableW, availableH),
    aspect,
    availableW,
    availableH
  );

  return {
    x: towardRight ? anchor.x : anchor.x - size.width,
    y: towardBottom ? anchor.y : anchor.y - size.height,
    width: size.width,
    height: size.height,
  };
}

/**
 * 辺ハンドルのリサイズ。動かす辺の軸だけがポインタで決まり、もう一方は縦横比から出す。
 *
 * **導いた側は、余地がある間は枠の中心を保ち、画像の端に当たったら滑らせる。**
 * 右辺を引いた時、高さは幅から決まるが上下どちらへ伸ばすかは決めようがないので、
 * 基本は中心を保って対称に伸ばす。
 *
 * ただし**中心を保つことを制約にしてはいけない。** 「中心を保ったまま伸ばせる範囲」
 * (= 中心から近い方の端までの 2 倍) を上限にすると、枠が画像の端に接している時に
 * その上限が現在の高さと一致し、**ハンドルが完全に死ぬ** (どこまで引いても寸法が
 * 元へ丸め戻される)。枠を上端まで move すれば必ず踏むので、特殊な状況ではない。
 * 上限は画像そのものにして、収まらないぶんは位置をずらして吸収する。
 */
export function aspectRectFromEdge(
  orig: CropRect,
  handle: string,
  pointer: { x: number; y: number },
  aspect: number,
  bounds: Size
): CropRect {
  const horizontal = handle === "e" || handle === "w";

  if (horizontal) {
    const fixedX = handle === "e" ? orig.x : orig.x + orig.width;
    const centerY = orig.y + orig.height / 2;
    const availableW = handle === "e" ? bounds.width - fixedX : fixedX;

    // 固定辺より向こう側は 0 に倒す。abs で取ると、行き過ぎた瞬間から枠が
    // **ポインタと逆向きに伸び始める** (自由モードは MIN_SIZE で止まるので挙動も食い違う)
    const wantedW = Math.max(0, handle === "e" ? pointer.x - fixedX : fixedX - pointer.x);
    const size = atLeastMin(
      fitWithin({ width: wantedW, height: wantedW / aspect }, availableW, bounds.height),
      aspect,
      availableW,
      bounds.height
    );
    return {
      x: handle === "e" ? fixedX : fixedX - size.width,
      y: clamp(centerY - size.height / 2, 0, bounds.height - size.height),
      width: size.width,
      height: size.height,
    };
  }

  const fixedY = handle === "s" ? orig.y : orig.y + orig.height;
  const centerX = orig.x + orig.width / 2;
  const availableH = handle === "s" ? bounds.height - fixedY : fixedY;

  const wantedH = Math.max(0, handle === "s" ? pointer.y - fixedY : fixedY - pointer.y);
  const size = atLeastMin(
    fitWithin({ width: wantedH * aspect, height: wantedH }, bounds.width, availableH),
    aspect,
    bounds.width,
    availableH
  );
  return {
    x: clamp(centerX - size.width / 2, 0, bounds.width - size.width),
    y: handle === "s" ? fixedY : fixedY - size.height,
    width: size.width,
    height: size.height,
  };
}

function clamp(v: number, lo: number, hi: number): number {
  // 画像より枠が大きい退化ケースで lo > hi になりうる。その時は lo (= 0 側) を採る
  return Math.min(Math.max(v, lo), Math.max(lo, hi));
}
