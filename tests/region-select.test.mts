import assert from "node:assert/strict";
import { CLICK_SLOP, dragRect, isDrag, toImageRect } from "../src/lib/regionSelect.ts";

// ドラッグの矩形

{
  // 右下へ引いても左上へ引いても同じ矩形になる
  assert.deepEqual(dragRect(10, 20, 110, 70, 1000, 800), { x: 10, y: 20, width: 100, height: 50 });
  assert.deepEqual(dragRect(110, 70, 10, 20, 1000, 800), { x: 10, y: 20, width: 100, height: 50 });
}

{
  // 画面の外へ出たポインタは端に寄せる
  assert.deepEqual(dragRect(900, 700, 1200, -50, 1000, 800), { x: 900, y: 0, width: 100, height: 700 });
}

// クリックとドラッグの区別

{
  assert.equal(isDrag(10, 10, 10, 10), false);
  assert.equal(isDrag(10, 10, 10 + CLICK_SLOP - 1, 10 + CLICK_SLOP - 1), false);
  assert.equal(isDrag(10, 10, 10 + CLICK_SLOP, 10), true);
  assert.equal(isDrag(10, 10, 10, 10 - CLICK_SLOP), true);
}

// 画像ピクセルへの変換

{
  // 等倍 (100% 表示) ならそのまま
  assert.deepEqual(toImageRect({ x: 10, y: 20, width: 30, height: 40 }, 1920, 1080, 1920, 1080), {
    x: 10,
    y: 20,
    width: 30,
    height: 40,
  });
}

{
  // 150% のモニター: 1280x720 の CSS px に 1920x1080 の画像を映している
  assert.deepEqual(toImageRect({ x: 10, y: 20, width: 30, height: 40 }, 1280, 720, 1920, 1080), {
    x: 15,
    y: 30,
    width: 45,
    height: 60,
  });
}

{
  // 端数は外側へ丸める (選んだ範囲を欠かさない)
  assert.deepEqual(toImageRect({ x: 1, y: 1, width: 1, height: 1 }, 1280, 720, 1920, 1080), {
    x: 1,
    y: 1,
    width: 2,
    height: 2,
  });
}

{
  // 画面いっぱいなら画像いっぱい。はみ出しは削る
  assert.deepEqual(toImageRect({ x: 0, y: 0, width: 1280, height: 720 }, 1280, 720, 1920, 1080), {
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
  });
  assert.deepEqual(toImageRect({ x: 1270, y: 710, width: 50, height: 50 }, 1280, 720, 1920, 1080), {
    x: 1905,
    y: 1065,
    width: 15,
    height: 15,
  });
}

{
  // 表示の寸法が取れていない (0) 時は空を返す。0 で割って NaN を送らない
  assert.deepEqual(toImageRect({ x: 1, y: 1, width: 1, height: 1 }, 0, 0, 1920, 1080), {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
  });
}

console.log("region-select: ok");
