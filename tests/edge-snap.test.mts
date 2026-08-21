import assert from "node:assert/strict";
import {
  snapLinesFromPixels,
  snapPositions,
  type EdgeSnapRun,
} from "../src/lib/edgeSnap.ts";

type Rgb = [number, number, number];

/** 間引き無しの吸着先座標 (検出そのものを見たいテスト用) */
function positionsOf(runs: EdgeSnapRun[], imageSize: number): number[] {
  return snapPositions(runs, imageSize, 0);
}

function blank(w: number, h: number, color: Rgb): Uint8Array {
  const data = new Uint8Array(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    data[i * 4] = color[0];
    data[i * 4 + 1] = color[1];
    data[i * 4 + 2] = color[2];
    data[i * 4 + 3] = 255;
  }
  return data;
}

function fillRect(
  data: Uint8Array,
  w: number,
  x0: number,
  y0: number,
  x1: number,
  y1: number,
  color: Rgb
) {
  for (let y = y0; y < y1; y++) {
    for (let x = x0; x < x1; x++) {
      const i = (y * w + x) * 4;
      data[i] = color[0];
      data[i + 1] = color[1];
      data[i + 2] = color[2];
      data[i + 3] = 255;
    }
  }
}

// 決定的な擬似乱数 (Math.random だと落ちた時に再現できない)
function makeRandom(seed: number) {
  let s = seed >>> 0;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 0x100000000;
  };
}

let failures = 0;
function check(name: string, fn: () => void) {
  try {
    fn();
    console.log(`ok   - ${name}`);
  } catch (e) {
    failures++;
    console.log(`FAIL - ${name}\n       ${(e as Error).message.split("\n")[0]}`);
  }
}

check("画像の端は常に候補に入る", () => {
  // Arrange
  const w = 200;
  const h = 120;
  const data = blank(w, h, [255, 255, 255]);
  // Act
  const lines = snapLinesFromPixels(data, w, h);
  // Assert
  assert.deepEqual(positionsOf(lines.xs, w), [0, w]);
  assert.deepEqual(positionsOf(lines.ys, h), [0, h]);
});

check("1px の縦線は両側の境界が候補になる", () => {
  // Arrange
  const w = 400;
  const h = 300;
  const data = blank(w, h, [255, 255, 255]);
  fillRect(data, w, 100, 0, 101, h, [0, 0, 0]);
  // Act
  const lines = snapLinesFromPixels(data, w, h);
  // Assert
  assert.ok(positionsOf(lines.xs, w).includes(100), `xs=${positionsOf(lines.xs, w)}`);
  assert.ok(positionsOf(lines.xs, w).includes(101), `xs=${positionsOf(lines.xs, w)}`);
  assert.deepEqual(positionsOf(lines.ys, h), [0, h]);
});

check("矩形の 4 辺がすべて候補になる", () => {
  // Arrange
  const w = 800;
  const h = 600;
  const data = blank(w, h, [250, 250, 250]);
  fillRect(data, w, 200, 120, 600, 400, [60, 90, 200]);
  // Act
  const lines = snapLinesFromPixels(data, w, h);
  // Assert
  assert.ok(positionsOf(lines.xs, w).includes(200), `xs=${positionsOf(lines.xs, w)}`);
  assert.ok(positionsOf(lines.xs, w).includes(600), `xs=${positionsOf(lines.xs, w)}`);
  assert.ok(positionsOf(lines.ys, h).includes(120), `ys=${positionsOf(lines.ys, h)}`);
  assert.ok(positionsOf(lines.ys, h).includes(400), `ys=${positionsOf(lines.ys, h)}`);
});

check("画像の一部にしか伸びていない線も拾える", () => {
  // Arrange: 高さの 30% しか無い縦線 (ダイアログの枠を想定)
  const w = 600;
  const h = 600;
  const data = blank(w, h, [255, 255, 255]);
  fillRect(data, w, 350, 0, 351, Math.round(h * 0.3), [0, 0, 0]);
  // Act
  const lines = snapLinesFromPixels(data, w, h);
  // Assert
  assert.ok(positionsOf(lines.xs, w).includes(350), `xs=${positionsOf(lines.xs, w)}`);
});

check("ノイズ画像では候補がほとんど出ない (吸着で操作不能にならない)", () => {
  // Arrange: どこを切っても差分が出る画像
  const w = 800;
  const h = 600;
  const data = blank(w, h, [0, 0, 0]);
  const rand = makeRandom(12345);
  for (let i = 0; i < w * h; i++) {
    data[i * 4] = Math.floor(rand() * 256);
    data[i * 4 + 1] = Math.floor(rand() * 256);
    data[i * 4 + 2] = Math.floor(rand() * 256);
  }
  // Act
  const lines = snapLinesFromPixels(data, w, h);
  // Assert: 端 2 本だけが望ましい。多くても密度上限のさらに 1/4 未満に収まること
  assert.ok(positionsOf(lines.xs, w).length <= 6, `xs=${positionsOf(lines.xs, w).length} 本: ${positionsOf(lines.xs, w).slice(0, 20)}`);
  assert.ok(positionsOf(lines.ys, h).length <= 6, `ys=${positionsOf(lines.ys, h).length} 本: ${positionsOf(lines.ys, h).slice(0, 20)}`);
});

check("ノイズの中の直線は拾える", () => {
  // Arrange: ノイズ背景 + 一本の縦線
  const w = 800;
  const h = 600;
  const data = blank(w, h, [0, 0, 0]);
  const rand = makeRandom(999);
  for (let i = 0; i < w * h; i++) {
    // 弱いノイズ (平均差 ~20)
    const v = 100 + Math.floor(rand() * 40);
    data[i * 4] = v;
    data[i * 4 + 1] = v;
    data[i * 4 + 2] = v;
  }
  fillRect(data, w, 500, 0, 501, h, [255, 255, 255]);
  // Act
  const lines = snapLinesFromPixels(data, w, h);
  // Assert
  assert.ok(positionsOf(lines.xs, w).includes(500), `xs=${positionsOf(lines.xs, w)}`);
  assert.ok(positionsOf(lines.xs, w).includes(501), `xs=${positionsOf(lines.xs, w)}`);
});

check("候補の密度に上限がかかる", () => {
  // Arrange: 4px ごとに縦線を引いた縞模様
  const w = 1200;
  const h = 400;
  const data = blank(w, h, [255, 255, 255]);
  for (let x = 0; x < w; x += 4) fillRect(data, w, x, 0, x + 1, h, [0, 0, 0]);
  // Act
  const lines = snapLinesFromPixels(data, w, h);
  // Assert: 線の上限 = max(24, 1200/40=30) 本。1 本あたり両端 2 個 + 画像の端 2 個
  assert.ok(positionsOf(lines.xs, w).length <= 62, `xs=${positionsOf(lines.xs, w).length} 個`);
});

check("大きい画像でも実用的な速度で終わる", () => {
  // Arrange: 5K 相当
  const w = 5120;
  const h = 2880;
  const data = blank(w, h, [255, 255, 255]);
  fillRect(data, w, 1000, 500, 4000, 2000, [30, 30, 30]);
  // Act
  const started = performance.now();
  const lines = snapLinesFromPixels(data, w, h);
  const elapsed = performance.now() - started;
  // Assert: 測っているのは検出の計算だけで、canvas 確保・drawImage・getImageData は
  // 含まない (それらはブラウザ側の処理で node からは動かせない)
  console.log(`       (5120x2880 の検出計算のみ: ${elapsed.toFixed(0)}ms)`);
  assert.ok(positionsOf(lines.xs, w).includes(1000) && positionsOf(lines.xs, w).includes(4000), `xs=${positionsOf(lines.xs, w)}`);
  assert.ok(elapsed < 1000, `${elapsed}ms かかった`);
});

check("アルファだけが違う境界も拾える", () => {
  // Arrange: 透明な黒 (0,0,0,0) の上に、不透明な黒 (0,0,0,255) の矩形。
  // RGB は全画素 0 なので、アルファを見ないと差分がゼロになって線が 1 本も出ない
  const w = 400;
  const h = 300;
  const data = new Uint8Array(w * h * 4); // 全画素 (0,0,0,0)
  for (let y = 50; y < 250; y++) {
    for (let x = 100; x < 300; x++) data[(y * w + x) * 4 + 3] = 255;
  }
  // Act
  const lines = snapLinesFromPixels(data, w, h);
  // Assert
  assert.ok(positionsOf(lines.xs, w).includes(100), `xs=${positionsOf(lines.xs, w)}`);
  assert.ok(positionsOf(lines.xs, w).includes(300), `xs=${positionsOf(lines.xs, w)}`);
  assert.ok(positionsOf(lines.ys, h).includes(50), `ys=${positionsOf(lines.ys, h)}`);
  assert.ok(positionsOf(lines.ys, h).includes(250), `ys=${positionsOf(lines.ys, h)}`);
});

check("不透明な画像のスコアはアルファ導入前と変わらない", () => {
  // Arrange: 同じ絵を「全画素不透明」で作る。アルファ差は常に 0 なので、
  // 4 チャンネルの和を 3 で割る限り従来とスコアが一致するはず。
  // 従来の値は blank/fillRect ベースのテスト群が担保しているので、ここでは
  // 「アルファを変えても結果が動かないこと」を直接見る
  const w = 400;
  const h = 300;
  const opaque = blank(w, h, [250, 250, 250]);
  fillRect(opaque, w, 100, 50, 300, 250, [40, 90, 200]);
  // 同じ絵で、アルファだけ一律に別の値 (128) にしたもの
  const translucent = blank(w, h, [250, 250, 250]);
  fillRect(translucent, w, 100, 50, 300, 250, [40, 90, 200]);
  for (let i = 0; i < w * h; i++) translucent[i * 4 + 3] = 128;

  // Act
  const a = snapLinesFromPixels(opaque, w, h);
  const b = snapLinesFromPixels(translucent, w, h);

  // Assert: アルファが一様なら (255 でも 128 でも) 差分に寄与しないので同じ結果になる
  assert.deepEqual(positionsOf(a.xs, w), positionsOf(b.xs, w));
  assert.deepEqual(positionsOf(a.ys, h), positionsOf(b.ys, h));
  assert.ok(positionsOf(a.xs, w).includes(100), `xs=${positionsOf(a.xs, w)}`);
});

// ---- 吸着先への変換 (snapPositions) ----

check("近すぎる線は強い方だけが残る", () => {
  // Arrange
  const runs = [
    { start: 100, end: 100, strength: 20 },
    { start: 110, end: 110, strength: 50 },
    { start: 400, end: 400, strength: 15 },
  ];
  // Act: tolerance 6 → 最小間隔 18
  const positions = snapPositions(runs, 1000, 6);
  // Assert: 100 と 110 は 10 しか離れていないので強い 110 が残る
  assert.deepEqual(positions, [0, 110, 400, 1000]);
});

check("1 本の線の両端は間引きで分けられない", () => {
  // Arrange: 1px の枠線 (両端が 1 しか離れていない)
  const runs = [{ start: 300, end: 301, strength: 90 }];
  // Act
  const positions = snapPositions(runs, 1000, 6);
  // Assert
  assert.deepEqual(positions, [0, 300, 301, 1000]);
});

check("画像の端は間引きに参加しないので、端の近くの線も残る", () => {
  // Arrange: 画像の端から 3px の位置にある線 (ウインドウ枠を想定)
  const runs = [{ start: 3, end: 3, strength: 90 }];
  // Act
  const positions = snapPositions(runs, 1000, 6);
  // Assert: 端 (0) が強さ無限で線を潰す、という作りにはしていない
  assert.deepEqual(positions, [0, 3, 1000]);
});

check("別サイズの画像から取った線は捨てる", () => {
  // Arrange: 画像を差し替えた直後に、旧画像 (幅 1000) の線が残っている状況
  const runs = [
    { start: 900, end: 900, strength: 90 },
    { start: 200, end: 200, strength: 80 },
  ];
  // Act: 新しい画像は幅 400 しかない
  const positions = snapPositions(runs, 400, 6);
  // Assert: 画像の外を指す 900 は吸着先にしない (枠が画像外へ飛ぶ)
  assert.deepEqual(positions, [0, 200, 400]);
});

check("縮小表示でも、吸着しない位置が十分に残る", () => {
  // Arrange: 5K のスクショに 60px 間隔の 1px 罫線。ビューポート 1200x700 に収めると
  // scale ≈ 0.234 で、許容距離は画像 px にすると 25.6 まで広がる。
  // (この条件で「辺を置ける位置の 86.7% が罫線に吸われる」のがレビューで出た実害)
  const w = 5120;
  const h = 2880;
  const data = blank(w, h, [255, 255, 255]);
  for (let x = 60; x < w; x += 60) fillRect(data, w, x, 0, x + 1, h, [80, 80, 80]);
  const scale = Math.min(1200 / w, 700 / h);
  const tolerance = 6 / scale;

  // Act
  const lines = snapLinesFromPixels(data, w, h);
  const positions = snapPositions(lines.xs, w, tolerance);

  // Assert: 1 画像 px 刻みで、どこにも吸着しない位置がどれだけ残るかを数える
  let free = 0;
  for (let x = 0; x <= w; x++) {
    if (!positions.some((p) => Math.abs(p - x) <= tolerance)) free++;
  }
  const freeRatio = free / (w + 1);
  console.log(`       (scale ${scale.toFixed(3)} / 候補 ${positions.length} 個 / 自由な位置 ${(freeRatio * 100).toFixed(1)}%)`);
  assert.ok(freeRatio >= 0.25, `自由に置ける位置が ${(freeRatio * 100).toFixed(1)}% しかない`);
});

check("等倍表示では縮小時より多くの線を吸着先に残す", () => {
  // Arrange: 上と同じ画像を scale 1 で使う
  const w = 5120;
  const h = 2880;
  const data = blank(w, h, [255, 255, 255]);
  for (let x = 60; x < w; x += 60) fillRect(data, w, x, 0, x + 1, h, [80, 80, 80]);
  const lines = snapLinesFromPixels(data, w, h);

  // Act
  const atFullScale = snapPositions(lines.xs, w, 6);
  const atReducedScale = snapPositions(lines.xs, w, 6 / Math.min(1200 / w, 700 / h));

  // Assert: 間引きは表示スケールに追随する (キャッシュに焼き付いていない)
  assert.ok(
    atFullScale.length > atReducedScale.length,
    `等倍 ${atFullScale.length} 個 / 縮小 ${atReducedScale.length} 個`
  );
});

console.log(failures === 0 ? "\nAll checks passed" : `\n${failures} check(s) failed`);
process.exit(failures === 0 ? 0 : 1);
