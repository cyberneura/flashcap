import assert from "node:assert/strict";
import { DEFAULT_ARROW_HEAD, hasArrowHead, parseArrowHead, snapEndpoint } from "../src/lib/arrowShape.ts";

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

check("parseArrowHead keeps existing settings as the arrow with a head", () => {
  assert.equal(DEFAULT_ARROW_HEAD, "start");
  assert.equal(parseArrowHead("none"), "none");
  assert.equal(parseArrowHead("start"), "start");
  assert.equal(parseArrowHead(undefined), "start");
  assert.equal(parseArrowHead("end"), "start");
  assert.equal(parseArrowHead(0), "start");
});

check("hasArrowHead: only 'none' drops the head (old arrows without the field keep it)", () => {
  assert.equal(hasArrowHead({ head: "start" }), true);
  assert.equal(hasArrowHead({ head: "none" }), false);
  assert.equal(hasArrowHead({}), true);
});

check("snapEndpoint snaps to 0 / 45 / 90 degrees", () => {
  // ほぼ水平 → 水平
  assert.deepEqual(snapEndpoint(10, 10, 110, 14), { x: 110, y: 10 });
  // ほぼ垂直 (上向き) → 垂直
  assert.deepEqual(snapEndpoint(10, 10, 13, -90), { x: 10, y: -90 });
  // ほぼ 45 度 → 45 度 (x と y の移動量が等しい)
  assert.deepEqual(snapEndpoint(0, 0, 100, 90), { x: 90, y: 90 });
  // 左下 (135 度方向)
  assert.deepEqual(snapEndpoint(0, 0, -50, 52), { x: -50, y: 50 });
});

check("snapEndpoint stays inside the box spanned by the start and the pointer", () => {
  // 画像内のドラッグなら吸着後も画像内 (射影だと (-40, 340) まで伸びていた)
  const p = snapEndpoint(200, 100, 5, 385);
  assert.ok(p.x >= 5 && p.x <= 200, `x=${p.x}`);
  assert.ok(p.y >= 100 && p.y <= 385, `y=${p.y}`);
  for (const [x, y] of [[300, 120], [180, 400], [-20, -30], [0, 99]]) {
    const q = snapEndpoint(100, 100, x, y);
    assert.ok(q.x >= Math.min(100, x) && q.x <= Math.max(100, x));
    assert.ok(q.y >= Math.min(100, y) && q.y <= Math.max(100, y));
  }
});

check("snapEndpoint leaves a zero-length drag as is", () => {
  assert.deepEqual(snapEndpoint(5, 5, 5, 5), { x: 5, y: 5 });
});

if (failures > 0) {
  console.log(`${failures} failed`);
  process.exit(1);
}
