import assert from "node:assert/strict";
import {
  DEFAULT_SPOTLIGHT_DARKNESS,
  SPOTLIGHT_DARKNESS_OPTIONS,
  parseSpotlightDarkness,
  spotlightClipPath,
  spotlightDarkRects,
} from "../src/lib/spotlight.ts";
import { parseShapeType } from "../src/lib/highlight.ts";

let failures = 0;
function check(name: string, fn: () => void) {
  try {
    fn();
    console.log(`ok - ${name}`);
  } catch (e) {
    failures++;
    console.log(`not ok - ${name}`);
    console.log(e);
  }
}

type R = { x: number; y: number; width: number; height: number };
const spot = (x: number, y: number, width: number, height: number) => ({ type: "spotlight" as const, x, y, width, height });
const area = (rects: R[]) => rects.reduce((sum, r) => sum + r.width * r.height, 0);

/** 各画素が暗くなる回数 (0 か 1 でなければ重なっている) */
function coverage(w: number, h: number, rects: R[]): number[] {
  const grid = new Array(w * h).fill(0);
  for (const r of rects) {
    for (let y = r.y; y < r.y + r.height; y++) for (let x = r.x; x < r.x + r.width; x++) grid[y * w + x]++;
  }
  return grid;
}

check("parseShapeType accepts spotlight", () => {
  assert.equal(parseShapeType("spotlight"), "spotlight");
});

check("no spotlight darkens nothing", () => {
  assert.deepEqual(spotlightDarkRects(100, 80, [], []), []);
  // 他の種別の図形はスポットライトではない
  assert.deepEqual(spotlightDarkRects(100, 80, [{ type: "rect", x: 10, y: 10, width: 20, height: 20 }], []), []);
  assert.deepEqual(spotlightDarkRects(100, 80, [{ type: "highlight", x: 10, y: 10, width: 20, height: 20 }], []), []);
});

check("one spotlight leaves its rectangle bright", () => {
  const rects = spotlightDarkRects(100, 80, [spot(10, 20, 30, 40)], []);
  assert.equal(area(rects), 100 * 80 - 30 * 40);
  const grid = coverage(100, 80, rects);
  assert.equal(grid[25 * 100 + 15], 0); // 内側は明るい
  assert.equal(grid[0], 1); // 外側は暗い
  assert.equal(grid[79 * 100 + 99], 1);
  assert.ok(grid.every((c) => c <= 1));
});

check("overlapping spotlights leave the union bright and never darken twice", () => {
  const rects = spotlightDarkRects(100, 80, [spot(10, 10, 40, 30), spot(30, 20, 40, 30)], []);
  const union = 40 * 30 + 40 * 30 - 20 * 20;
  assert.equal(area(rects), 100 * 80 - union);
  const grid = coverage(100, 80, rects);
  assert.ok(grid.every((c) => c <= 1));
  assert.equal(grid[35 * 100 + 35], 0); // 重なった所も明るい
});

check("separate spotlights all stay bright", () => {
  const rects = spotlightDarkRects(100, 80, [spot(0, 0, 10, 10), spot(80, 60, 20, 20)], []);
  assert.equal(area(rects), 100 * 80 - 100 - 400);
});

check("masks are not darkened", () => {
  const rects = spotlightDarkRects(100, 80, [spot(10, 10, 20, 20)], [{ x: 60, y: 40, width: 10, height: 10 }]);
  assert.equal(area(rects), 100 * 80 - 400 - 100);
  assert.equal(coverage(100, 80, rects)[45 * 100 + 65], 0);
});

check("a mask alone does not darken anything", () => {
  assert.deepEqual(spotlightDarkRects(100, 80, [], [{ x: 0, y: 0, width: 10, height: 10 }]), []);
});

check("fractional coordinates are rounded to whole pixels", () => {
  const rects = spotlightDarkRects(100.4, 80, [spot(10.4, 10.6, 20.2, 19.7)], [{ x: 50.5, y: 5.2, width: 9.6, height: 9.4 }]);
  for (const r of rects) {
    for (const v of [r.x, r.y, r.width, r.height]) assert.ok(Number.isInteger(v), `${v} is not an integer`);
  }
  assert.ok(coverage(100, 80, rects).every((c) => c <= 1));
});

check("spotlights outside the image or collapsed ones are ignored safely", () => {
  // 潰れたスポットライトは無いものとして扱う (画像全体を暗くしない)
  assert.deepEqual(spotlightDarkRects(50, 50, [spot(10, 10, 0, 20)], []), []);
  // 画像より大きいスポットライトは何も暗くしない
  assert.deepEqual(spotlightDarkRects(50, 50, [spot(-10, -10, 100, 100)], []), []);
});

check("clip path covers exactly the dark rectangles", () => {
  assert.equal(spotlightClipPath([]), `path("M0 0Z")`);
  assert.equal(
    spotlightClipPath([{ x: 0, y: 0, width: 10, height: 5 }, { x: 0, y: 5, width: 3, height: 2 }]),
    `path("M0 0h10v5h-10ZM0 5h3v2h-3Z")`,
  );
});

check("darkness from the config is validated", () => {
  for (const v of SPOTLIGHT_DARKNESS_OPTIONS) assert.equal(parseSpotlightDarkness(v), v);
  assert.ok(SPOTLIGHT_DARKNESS_OPTIONS.includes(DEFAULT_SPOTLIGHT_DARKNESS));
  assert.equal(parseSpotlightDarkness(1), 1);
  for (const bad of [0, -0.5, 1.5, NaN, Infinity, "0.5", null, undefined]) {
    assert.equal(parseSpotlightDarkness(bad), DEFAULT_SPOTLIGHT_DARKNESS);
  }
});

if (failures > 0) {
  console.log(`${failures} test(s) failed`);
  process.exit(1);
}
