import assert from "node:assert/strict";
import {
  DARK_HIGHLIGHT_OPACITY,
  DARK_LUMA_THRESHOLD,
  LUMA_MAP_MAX_SIDE,
  SHAPE_TYPES,
  averageLuma,
  highlightPaint,
  hitsHighlight,
  lumaMapFromRgba,
  lumaMapSize,
  parseShapeType,
} from "../src/lib/highlight.ts";

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

/** A w x h map whose cells come from `gray(x, y)` (0..255), standing for an image `scale` times larger */
function grayMap(w: number, h: number, gray: (x: number, y: number) => number, scale = 1) {
  const rgba = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * 4;
      const g = gray(x, y);
      rgba[o] = g;
      rgba[o + 1] = g;
      rgba[o + 2] = g;
      rgba[o + 3] = 255;
    }
  }
  return lumaMapFromRgba(rgba, w, h, w * scale, h * scale);
}

check("parseShapeType keeps the known types and falls back to rect", () => {
  for (const t of SHAPE_TYPES) assert.equal(parseShapeType(t), t);
  assert.equal(parseShapeType("highlight"), "highlight");
  assert.equal(parseShapeType("triangle"), "rect");
  assert.equal(parseShapeType(""), "rect");
  assert.equal(parseShapeType(3), "rect");
  assert.equal(parseShapeType(null), "rect");
  assert.equal(parseShapeType(undefined), "rect");
  assert.equal(parseShapeType({ toString: () => "ellipse" }), "rect");
});

check("lumaMapSize caps the longer side and never enlarges", () => {
  assert.deepEqual(lumaMapSize(100, 50), { width: 100, height: 50 });
  assert.deepEqual(lumaMapSize(2560, 1440), { width: LUMA_MAP_MAX_SIDE, height: 144 });
  assert.deepEqual(lumaMapSize(1440, 2560), { width: 144, height: LUMA_MAP_MAX_SIDE });
  // A very thin image still gets at least one cell per axis
  assert.deepEqual(lumaMapSize(5000, 2), { width: LUMA_MAP_MAX_SIDE, height: 1 });
});

check("lumaMapFromRgba weights the channels (green counts most) and ignores alpha", () => {
  const map = lumaMapFromRgba([255, 0, 0, 255, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255], 4, 1, 4, 1);
  assert.ok(Math.abs(map.luma[0] - 0.2126) < 1e-6);
  assert.ok(Math.abs(map.luma[1] - 0.7152) < 1e-6);
  assert.ok(Math.abs(map.luma[2] - 0.0722) < 1e-6);
  assert.ok(Math.abs(map.luma[3] - 1) < 1e-6);
});

check("averageLuma averages only the cells under the rectangle", () => {
  // Left half black, right half white
  const map = grayMap(10, 10, (x) => (x < 5 ? 0 : 255));
  assert.equal(averageLuma(map, { x: 0, y: 0, width: 5, height: 10 }), 0);
  assert.equal(averageLuma(map, { x: 5, y: 2, width: 5, height: 3 }), 1);
  assert.equal(averageLuma(map, { x: 0, y: 0, width: 10, height: 10 }), 0.5);
});

check("averageLuma maps natural pixels onto a downscaled map", () => {
  // The map is 10x10 but the image is 100x100
  const map = grayMap(10, 10, (x) => (x < 5 ? 0 : 255), 10);
  assert.equal(averageLuma(map, { x: 0, y: 0, width: 50, height: 100 }), 0);
  assert.equal(averageLuma(map, { x: 50, y: 0, width: 50, height: 100 }), 1);
  // A rectangle smaller than one cell still reads the cell it sits in
  assert.equal(averageLuma(map, { x: 72, y: 31, width: 2, height: 2 }), 1);
});

check("averageLuma clips to the image and returns null when nothing overlaps", () => {
  const map = grayMap(10, 10, () => 255);
  assert.equal(averageLuma(map, { x: -5, y: -5, width: 8, height: 8 }), 1);
  assert.equal(averageLuma(map, { x: 8, y: 8, width: 50, height: 50 }), 1);
  assert.equal(averageLuma(map, { x: 20, y: 0, width: 5, height: 5 }), null);
  assert.equal(averageLuma(map, { x: -20, y: 0, width: 5, height: 5 }), null);
  assert.equal(averageLuma(map, { x: 2, y: 2, width: 0, height: 5 }), null);
});

check("highlightPaint multiplies on a light background", () => {
  const map = grayMap(10, 10, () => 250);
  assert.deepEqual(highlightPaint(map, { x: 1, y: 1, width: 5, height: 2 }), { blend: "multiply", opacity: 1 });
});

check("highlightPaint switches to a translucent fill on a dark background", () => {
  const map = grayMap(10, 10, () => 30);
  assert.deepEqual(highlightPaint(map, { x: 1, y: 1, width: 5, height: 2 }), {
    blend: "normal",
    opacity: DARK_HIGHLIGHT_OPACITY,
  });
});

check("highlightPaint decides per rectangle, not per image", () => {
  // Dark sidebar on the left, light document on the right
  const map = grayMap(10, 10, (x) => (x < 3 ? 20 : 245));
  assert.equal(highlightPaint(map, { x: 0, y: 0, width: 3, height: 4 }).blend, "normal");
  assert.equal(highlightPaint(map, { x: 4, y: 0, width: 5, height: 4 }).blend, "multiply");
});

check("highlightPaint threshold: just below is dark, at or above is light", () => {
  const below = Math.floor(DARK_LUMA_THRESHOLD * 255) - 1;
  const above = Math.ceil(DARK_LUMA_THRESHOLD * 255) + 1;
  assert.equal(highlightPaint(grayMap(2, 2, () => below), { x: 0, y: 0, width: 2, height: 2 }).blend, "normal");
  assert.equal(highlightPaint(grayMap(2, 2, () => above), { x: 0, y: 0, width: 2, height: 2 }).blend, "multiply");
});

check("highlightPaint multiplies when there is no map yet or the rectangle is off the image", () => {
  assert.deepEqual(highlightPaint(null, { x: 0, y: 0, width: 5, height: 5 }), { blend: "multiply", opacity: 1 });
  const dark = grayMap(10, 10, () => 0);
  assert.deepEqual(highlightPaint(dark, { x: 50, y: 50, width: 5, height: 5 }), { blend: "multiply", opacity: 1 });
});

check("hitsHighlight hits anywhere inside, plus the margin around the edge", () => {
  const shape = { x: 100, y: 50, width: 80, height: 20 };
  assert.equal(hitsHighlight(shape, 140, 60, 0), true);
  assert.equal(hitsHighlight(shape, 100, 50, 0), true);
  assert.equal(hitsHighlight(shape, 180, 70, 0), true);
  assert.equal(hitsHighlight(shape, 99, 60, 0), false);
  assert.equal(hitsHighlight(shape, 97, 60, 3), true);
  assert.equal(hitsHighlight(shape, 140, 74, 3), false);
  assert.equal(hitsHighlight(shape, 185, 60, 3), false);
});

if (failures > 0) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
