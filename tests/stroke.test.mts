import assert from "node:assert/strict";
import {
  appendStrokePoint,
  distToStroke,
  finishStrokePoints,
  hitsStroke,
  offsetStrokeCopy,
  strokeAt,
  strokeBounds,
  strokeLength,
  strokeCurvePoints,
  strokePath,
} from "../src/lib/stroke.ts";
import type { Stroke } from "../src/lib/types.ts";

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

function stroke(points: { x: number; y: number }[], id = "s", thickness = 4): Stroke {
  return { id, points, color: "#ff0000", thickness, whiteStroke: true, dropShadow: true };
}

check("appendStrokePoint drops points closer than the minimum step", () => {
  const p0 = [{ x: 0, y: 0 }];
  assert.equal(appendStrokePoint(p0, { x: 1, y: 1 }, 2), p0); // same array = no re-render
  const p1 = appendStrokePoint(p0, { x: 3, y: 0 }, 2);
  assert.deepEqual(p1, [{ x: 0, y: 0 }, { x: 3, y: 0 }]);
  assert.notEqual(p1, p0);
  // The first point is always kept
  assert.deepEqual(appendStrokePoint([], { x: 5, y: 5 }, 2), [{ x: 5, y: 5 }]);
});

check("finishStrokePoints adds the release position unless it is already the last point", () => {
  const pts = [{ x: 0, y: 0 }, { x: 10, y: 0 }];
  assert.deepEqual(finishStrokePoints(pts, { x: 11, y: 0 }), [...pts, { x: 11, y: 0 }]);
  assert.equal(finishStrokePoints(pts, { x: 10, y: 0 }), pts);
});

check("strokeLength sums the segments", () => {
  assert.equal(strokeLength([]), 0);
  assert.equal(strokeLength([{ x: 0, y: 0 }]), 0);
  assert.equal(strokeLength([{ x: 0, y: 0 }, { x: 3, y: 4 }, { x: 3, y: 10 }]), 11);
});

check("strokePath: straight segments for 1 or 2 points", () => {
  assert.equal(strokePath([]), "");
  assert.equal(strokePath([{ x: 1, y: 2 }]), "M1 2L1 2");
  assert.equal(strokePath([{ x: 1, y: 2 }, { x: 3.456, y: 4 }]), "M1 2L3.46 4");
});

check("strokePath: midpoint-smoothed quadratics, starting and ending on the real points", () => {
  const d = strokePath([{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }, { x: 20, y: 10 }]);
  assert.equal(d, "M0 0L5 0Q10 0 10 5Q10 10 15 10L20 10");
  // Every number in the path is finite (Path2D rejects the whole string otherwise)
  for (const n of d.match(/-?[\d.]+/g)!) assert.ok(Number.isFinite(Number(n)));
});

check("distToStroke measures to the nearest segment", () => {
  const pts = [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }];
  // Points beside the straight parts (the curve rounds only the corner, from (5, 0) to (10, 5))
  assert.ok(Math.abs(distToStroke(pts, 3, 3) - 3) < 1e-9);
  assert.ok(Math.abs(distToStroke(pts, 13, 8) - 3) < 1e-9);
  assert.equal(distToStroke(pts, -3, -4), 5);
  assert.equal(distToStroke([{ x: 1, y: 1 }], 4, 5), 5);
  assert.equal(distToStroke([], 0, 0), Infinity);
});

check("distToStroke follows the smoothed curve, not the polyline through the kept points", () => {
  // At the corner (100, 0) the drawn curve passes through (87.5, 12.5), 12.5 px from the polyline
  const pts = [{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 100 }];
  assert.ok(distToStroke(pts, 87.5, 12.5) < 0.5);
  const s = stroke(pts, "corner", 4);
  assert.equal(hitsStroke(s, 87.5, 12.5, 1), true);
  // The sharp corner itself is outside the drawn line (the curve cuts inside it)
  assert.ok(distToStroke(pts, 100, 0) > 10);
});

check("strokeCurvePoints keeps the real start and end", () => {
  const pts = [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }, { x: 20, y: 10 }];
  const curve = strokeCurvePoints(pts);
  assert.deepEqual(curve[0], { x: 0, y: 0 });
  assert.deepEqual(curve[curve.length - 1], { x: 20, y: 10 });
  assert.deepEqual(strokeCurvePoints([{ x: 1, y: 1 }]), [{ x: 1, y: 1 }]);
});

check("hitsStroke: drawn thickness x annotationScale + 6 screen px", () => {
  const s = stroke([{ x: 0, y: 0 }, { x: 100, y: 0 }], "a", 4);
  assert.equal(hitsStroke(s, 50, 9.9, 1), true); // 4 + 6
  assert.equal(hitsStroke(s, 50, 10.1, 1), false);
  // Retina: the line is drawn twice as thick, so the tolerance follows
  assert.equal(hitsStroke(s, 50, 13.9, 1, 2), true);
  // Zoomed out to 50%: 6 screen px = 12 image px
  assert.equal(hitsStroke(s, 50, 15.9, 0.5), true);
});

check("strokeAt returns the topmost (last drawn) stroke", () => {
  const a = stroke([{ x: 0, y: 0 }, { x: 100, y: 0 }], "a");
  const b = stroke([{ x: 50, y: -50 }, { x: 50, y: 50 }], "b");
  assert.equal(strokeAt([a, b], 50, 0, 1)?.id, "b");
  assert.equal(strokeAt([a, b], 10, 0, 1)?.id, "a");
  assert.equal(strokeAt([a, b], 10, 40, 1), undefined);
});

check("offsetStrokeCopy moves every point and does not touch the original", () => {
  const s = stroke([{ x: 1, y: 2 }, { x: 3, y: 4 }], "orig");
  const c = offsetStrokeCopy(s, 16, -2, "copy");
  assert.equal(c.id, "copy");
  assert.deepEqual(c.points, [{ x: 17, y: 0 }, { x: 19, y: 2 }]);
  assert.deepEqual(s.points, [{ x: 1, y: 2 }, { x: 3, y: 4 }]);
  assert.equal(c.color, s.color);
  // Moving the copy must not move the original through a shared point object
  c.points[0].x = 999;
  assert.equal(s.points[0].x, 1);
});

check("strokeBounds", () => {
  assert.equal(strokeBounds([]), null);
  assert.deepEqual(strokeBounds([{ x: 5, y: 1 }, { x: -2, y: 7 }, { x: 3, y: 3 }]), { x: -2, y: 1, width: 7, height: 6 });
});

if (failures > 0) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
