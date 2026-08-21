import assert from "node:assert/strict";
import {
  ASPECT_SQUARE,
  ASPECT_WIDESCREEN,
  MIN_CROP_SIZE,
  aspectRectFromCorner,
  aspectRectFromEdge,
  largestAspectRect,
  refitToAspect,
} from "../src/lib/cropAspect.ts";
import type { CropRect } from "../src/lib/types.ts";

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

const near = (a: number, b: number, tolerance = 1e-9) => Math.abs(a - b) <= tolerance;

/** 枠が縦横比どおりで、画像の中に収まっていることを確かめる */
function assertValid(rect: CropRect, aspect: number, bounds: { width: number; height: number }) {
  assert.ok(
    near(rect.width / rect.height, aspect, 1e-6),
    `比率が ${(rect.width / rect.height).toFixed(4)} で ${aspect.toFixed(4)} でない`
  );
  assert.ok(rect.x >= -1e-9, `x=${rect.x} が負`);
  assert.ok(rect.y >= -1e-9, `y=${rect.y} が負`);
  assert.ok(rect.x + rect.width <= bounds.width + 1e-9, `右端 ${rect.x + rect.width} がはみ出す`);
  assert.ok(rect.y + rect.height <= bounds.height + 1e-9, `下端 ${rect.y + rect.height} がはみ出す`);
}

const wide = { width: 1600, height: 900 };
const tall = { width: 600, height: 1200 };

check("正方形の最大枠は短辺に合わせて中央に置かれる", () => {
  // Arrange / Act
  const rect = largestAspectRect(ASPECT_SQUARE, wide);
  // Assert
  assert.deepEqual(rect, { x: 350, y: 0, width: 900, height: 900 });
  assertValid(rect, ASPECT_SQUARE, wide);
});

check("16:9 の最大枠は縦長画像では幅いっぱいになる", () => {
  // Arrange / Act
  const rect = largestAspectRect(ASPECT_WIDESCREEN, tall);
  // Assert
  assert.ok(near(rect.width, 600), `width=${rect.width}`);
  assert.ok(near(rect.height, 337.5), `height=${rect.height}`);
  assert.ok(near(rect.y, (1200 - 337.5) / 2), `y=${rect.y}`);
  assertValid(rect, ASPECT_WIDESCREEN, tall);
});

check("既にある枠を比率へ合わせ直す時は中心を保ち、内側に収める", () => {
  // Arrange: 中心 (500, 500) の横長の枠
  const rect: CropRect = { x: 300, y: 400, width: 400, height: 200 };
  // Act
  const fitted = refitToAspect(rect, ASPECT_SQUARE, wide);
  // Assert: 小さい方の軸 (高さ 200) に合わせるので 200x200、中心は動かない
  assert.deepEqual(fitted, { x: 400, y: 400, width: 200, height: 200 });
  assertValid(fitted, ASPECT_SQUARE, wide);
});

check("合わせ直しで画像からはみ出す時は中心をずらして収める", () => {
  // Arrange: 右端に寄った枠
  const rect: CropRect = { x: 1400, y: 100, width: 200, height: 600 };
  // Act
  const fitted = refitToAspect(rect, ASPECT_SQUARE, wide);
  // Assert
  assertValid(fitted, ASPECT_SQUARE, wide);
  assert.ok(near(fitted.x + fitted.width, 1600), `右端 ${fitted.x + fitted.width}`);
});

check("角から引くと、動かした量の大きい軸に合わせて比率どおりに伸びる", () => {
  // Arrange: 左上を固定して右下へ。横に大きく、縦に少しだけ動かす
  const anchor = { x: 100, y: 100 };
  // Act
  const rect = aspectRectFromCorner(anchor, { x: 900, y: 200 }, ASPECT_SQUARE, wide);
  // Assert: 大きい方 (幅 800) に合わせるので 800x800
  assert.deepEqual(rect, { x: 100, y: 100, width: 800, height: 800 });
  assertValid(rect, ASPECT_SQUARE, wide);
});

check("角から引く方向が左上でも、固定した角は動かない", () => {
  // Arrange: 右下 (1000, 800) を固定して左上へ引く
  const anchor = { x: 1000, y: 800 };
  // Act
  const rect = aspectRectFromCorner(anchor, { x: 400, y: 700 }, ASPECT_SQUARE, wide);
  // Assert: 固定した角がそのまま右下に残る
  assert.ok(near(rect.x + rect.width, 1000), `右端 ${rect.x + rect.width}`);
  assert.ok(near(rect.y + rect.height, 800), `下端 ${rect.y + rect.height}`);
  assertValid(rect, ASPECT_SQUARE, wide);
});

check("角から引いて画像の外へ出そうな時は、比率のまま縮めて収める", () => {
  // Arrange: 左上を固定して、画像の外を大きく超えるところまで引く
  const anchor = { x: 1000, y: 100 };
  // Act
  const rect = aspectRectFromCorner(anchor, { x: 5000, y: 5000 }, ASPECT_SQUARE, wide);
  // Assert: 右へは 600、下へは 800 しか無いので 600x600 に収まる
  assert.deepEqual(rect, { x: 1000, y: 100, width: 600, height: 600 });
  assertValid(rect, ASPECT_SQUARE, wide);
});

check("角をほとんど動かさなくても最小サイズは保たれる", () => {
  // Arrange / Act
  const rect = aspectRectFromCorner({ x: 100, y: 100 }, { x: 101, y: 101 }, ASPECT_WIDESCREEN, wide);
  // Assert
  assert.ok(rect.width >= MIN_CROP_SIZE - 1e-9, `width=${rect.width}`);
  assert.ok(rect.height >= MIN_CROP_SIZE - 1e-9, `height=${rect.height}`);
  assertValid(rect, ASPECT_WIDESCREEN, wide);
});

check("右辺を引くと高さは比率から決まり、上下の中心は動かない", () => {
  // Arrange: 中心 y = 500 の枠
  const orig: CropRect = { x: 200, y: 400, width: 300, height: 200 };
  // Act
  const rect = aspectRectFromEdge(orig, "e", { x: 800, y: 0 }, ASPECT_SQUARE, wide);
  // Assert: 左辺は固定、幅 600 → 高さ 600、中心 y は 500 のまま
  assert.ok(near(rect.x, 200), `x=${rect.x}`);
  assert.ok(near(rect.width, 600), `width=${rect.width}`);
  assert.ok(near(rect.y + rect.height / 2, 500), `中心 y=${rect.y + rect.height / 2}`);
  assertValid(rect, ASPECT_SQUARE, wide);
});

check("左辺を引くと右辺が固定される", () => {
  // Arrange
  const orig: CropRect = { x: 400, y: 300, width: 300, height: 200 };
  // Act
  const rect = aspectRectFromEdge(orig, "w", { x: 100, y: 0 }, ASPECT_SQUARE, wide);
  // Assert
  assert.ok(near(rect.x + rect.width, 700), `右端 ${rect.x + rect.width}`);
  assertValid(rect, ASPECT_SQUARE, wide);
});

check("導いた側が画像に収まらない時は、中心を保つのをやめて滑らせる", () => {
  // Arrange: 中心 y が上端寄り (y=50) の小さな枠
  const orig: CropRect = { x: 200, y: 20, width: 60, height: 60 };
  // Act: 幅を 900 まで広げる
  const rect = aspectRectFromEdge(orig, "e", { x: 1100, y: 0 }, ASPECT_SQUARE, wide);
  // Assert: 中心固定に縛られず 900x900 まで伸び、収めるために上端へ寄る
  assert.ok(near(rect.width, 900), `width=${rect.width}`);
  assert.ok(near(rect.height, 900), `height=${rect.height}`);
  assert.ok(near(rect.y, 0), `y=${rect.y}`);
  assertValid(rect, ASPECT_SQUARE, wide);
});

check("枠が画像の端に接していても、辺ハンドルで広げられる", () => {
  // Arrange: move で上端まで寄せた枠 (move は y を 0 にクランプするので必ずこうなる)。
  // 導いた軸の上限を「中心を保ったまま伸ばせる範囲」にすると、それが現在の高さと
  // 一致してハンドルが完全に死ぬ
  const atTop: CropRect = { x: 100, y: 0, width: 200, height: 200 };
  // Act
  const grown = [400, 800, 1600].map((px) =>
    aspectRectFromEdge(atTop, "e", { x: px, y: 0 }, ASPECT_SQUARE, wide)
  );
  // Assert: 引くほど大きくなる (元の 200 のままにならない)
  assert.ok(grown[0].width > atTop.width, `x=400 で ${grown[0].width}`);
  assert.ok(grown[1].width > grown[0].width, `x=800 で ${grown[1].width}`);
  assert.ok(grown[2].width > grown[1].width, `x=1600 で ${grown[2].width}`);
  for (const rect of grown) assertValid(rect, ASPECT_SQUARE, wide);
});

check("辺ハンドルを固定辺より向こうへ引いても、逆向きに伸びない", () => {
  // Arrange: 左辺が x=400 に固定されている枠の、右辺を掴む
  const orig: CropRect = { x: 400, y: 300, width: 300, height: 300 };
  // Act: ポインタを固定辺より左まで動かしていく
  const widths = [500, 410, 400, 390, 300, 0].map(
    (px) => aspectRectFromEdge(orig, "e", { x: px, y: 0 }, ASPECT_SQUARE, wide).width
  );
  // Assert: 左へ動かすほど縮み、最小サイズで止まる (行き過ぎて再び伸びない)
  for (let i = 1; i < widths.length; i++) {
    assert.ok(widths[i] <= widths[i - 1] + 1e-9, `${widths[i - 1]} → ${widths[i]} で増えた`);
  }
  assert.ok(near(widths[widths.length - 1], MIN_CROP_SIZE), `最終 ${widths[widths.length - 1]}`);
});

check("上辺ハンドルを固定辺より向こうへ引いても、逆向きに伸びない", () => {
  // Arrange: 下辺が y=600 に固定されている枠の、上辺を掴む
  const orig: CropRect = { x: 400, y: 300, width: 300, height: 300 };
  // Act
  const heights = [400, 590, 600, 610, 900].map(
    (py) => aspectRectFromEdge(orig, "n", { x: 0, y: py }, ASPECT_SQUARE, wide).height
  );
  // Assert
  for (let i = 1; i < heights.length; i++) {
    assert.ok(heights[i] <= heights[i - 1] + 1e-9, `${heights[i - 1]} → ${heights[i]} で増えた`);
  }
  assert.ok(near(heights[heights.length - 1], MIN_CROP_SIZE), `最終 ${heights[heights.length - 1]}`);
});

check("左右の端に接した枠も、縦の辺ハンドルで広げられる", () => {
  // Arrange
  const atLeft: CropRect = { x: 0, y: 300, width: 200, height: 200 };
  // Act
  const rect = aspectRectFromEdge(atLeft, "n", { x: 0, y: 0 }, ASPECT_SQUARE, wide);
  // Assert
  assert.ok(rect.height > atLeft.height, `height=${rect.height}`);
  assertValid(rect, ASPECT_SQUARE, wide);
});

check("下辺を引くと上辺が固定され、左右の中心は動かない", () => {
  // Arrange
  const orig: CropRect = { x: 500, y: 100, width: 200, height: 100 };
  // Act
  const rect = aspectRectFromEdge(orig, "s", { x: 0, y: 500 }, ASPECT_WIDESCREEN, wide);
  // Assert
  assert.ok(near(rect.y, 100), `y=${rect.y}`);
  assert.ok(near(rect.x + rect.width / 2, 600), `中心 x=${rect.x + rect.width / 2}`);
  assertValid(rect, ASPECT_WIDESCREEN, wide);
});

check("掴んだ角が画像の隅に乗っていても枠が潰れない", () => {
  // Arrange: 画像いっぱいの 16:9 枠の nw ハンドルを掴む → anchor は右下の隅そのもの。
  // ポインタは画像内へクランプされるので anchor と一致し、右下方向の余地は 0 になる
  const anchor = { x: wide.width, y: wide.height };
  // Act
  const rect = aspectRectFromCorner(anchor, { x: wide.width, y: wide.height }, ASPECT_WIDESCREEN, wide);
  // Assert: 余地のある左上へ倒して最小サイズの枠を作る (0×0 で比率 NaN にしない)
  assert.ok(rect.width >= MIN_CROP_SIZE - 1e-9, `width=${rect.width}`);
  assert.ok(rect.height >= MIN_CROP_SIZE - 1e-9, `height=${rect.height}`);
  assert.ok(near(rect.x + rect.width, wide.width), `固定した角が動いた: ${rect.x + rect.width}`);
  assert.ok(near(rect.y + rect.height, wide.height), `固定した角が動いた: ${rect.y + rect.height}`);
  assertValid(rect, ASPECT_WIDESCREEN, wide);
});

check("4 隅すべてで、余地の無い向きへは伸ばさない", () => {
  // Arrange / Act / Assert
  for (const anchor of [
    { x: 0, y: 0 },
    { x: wide.width, y: 0 },
    { x: 0, y: wide.height },
    { x: wide.width, y: wide.height },
  ]) {
    for (const pointer of [
      { x: 0, y: 0 },
      { x: wide.width, y: 0 },
      { x: 0, y: wide.height },
      { x: wide.width, y: wide.height },
      { ...anchor },
    ]) {
      const rect = aspectRectFromCorner(anchor, pointer, ASPECT_SQUARE, wide);
      assert.ok(
        rect.width > 0 && rect.height > 0,
        `anchor=${JSON.stringify(anchor)} pointer=${JSON.stringify(pointer)} で ${JSON.stringify(rect)}`
      );
      assertValid(rect, ASPECT_SQUARE, wide);
    }
  }
});

check("丸め誤差で余地がほぼ 0 の角でも潰れない", () => {
  // Arrange: largestAspectRect が返す寸法には誤差が乗りうるので、それを模す
  const anchor = { x: wide.width - 4.5e-13, y: wide.height - 2e-13 };
  // Act
  const rect = aspectRectFromCorner(anchor, { x: wide.width, y: wide.height }, ASPECT_WIDESCREEN, wide);
  // Assert
  assert.ok(rect.width >= MIN_CROP_SIZE - 1e-9, `width=${rect.width}`);
  assertValid(rect, ASPECT_WIDESCREEN, wide);
});

check("辺ハンドルでも、掴んだ辺が画像の端に乗っていて潰れない", () => {
  // Arrange: 右辺が画像の右端に接している枠の右辺を、さらに外へ引く
  const orig: CropRect = { x: 1000, y: 300, width: 600, height: 600 };
  // Act
  const rect = aspectRectFromEdge(orig, "e", { x: 5000, y: 0 }, ASPECT_SQUARE, wide);
  // Assert
  assert.ok(rect.width > 0 && rect.height > 0, JSON.stringify(rect));
  assertValid(rect, ASPECT_SQUARE, wide);
});

check("画像が最小サイズの枠より小さくても、はみ出さない", () => {
  // Arrange: 16:9 の最小枠 (14.2 x 8) すら入らない極小画像
  const tiny = { width: 6, height: 6 };
  // Act
  const drawn = aspectRectFromCorner({ x: 0, y: 0 }, { x: 5, y: 5 }, ASPECT_WIDESCREEN, tiny);
  const largest = largestAspectRect(ASPECT_WIDESCREEN, tiny);
  // Assert: 最小サイズより収まる方を優先する
  assertValid(drawn, ASPECT_WIDESCREEN, tiny);
  assertValid(largest, ASPECT_WIDESCREEN, tiny);
});

console.log(failures === 0 ? "\nAll checks passed" : `\n${failures} check(s) failed`);
process.exit(failures === 0 ? 0 : 1);
