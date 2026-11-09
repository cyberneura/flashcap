import assert from "node:assert/strict";
import {
  STEP_LABEL_BASELINE_RATIO,
  hitsStep,
  nextStep,
  renumber,
  stepFontSize,
  stepLabelBaseline,
  topmostStepAt,
} from "../src/lib/steps.ts";
import type { Step } from "../src/lib/types.ts";

const settings = { color: "#FF0000", size: 32, whiteStroke: true, dropShadow: true };

const step = (over: Partial<Step> = {}): Step => ({
  id: "a",
  x: 100,
  y: 100,
  number: 1,
  color: "#FF0000",
  size: 32,
  whiteStroke: true,
  dropShadow: true,
  ...over,
});

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

check("nextStep numbers from the current count and copies the settings", () => {
  const first = nextStep([], 10, 20, settings, "s1");
  assert.deepEqual(first, { id: "s1", x: 10, y: 20, number: 1, ...settings });
  const second = nextStep([first], 30, 40, { ...settings, color: "#00FF00" }, "s2");
  assert.equal(second.number, 2);
  assert.equal(second.color, "#00FF00");
});

check("renumber closes the gap left by a deleted step and keeps the order", () => {
  const steps = [step({ id: "a", number: 1 }), step({ id: "b", number: 2 }), step({ id: "c", number: 3 })];
  const after = renumber(steps.filter((s) => s.id !== "b"));
  assert.deepEqual(after.map((s) => [s.id, s.number]), [["a", 1], ["c", 2]]);
  // Untouched entries are the same objects (no needless copies)
  assert.equal(after[0], steps[0]);
});

check("renumber on an already numbered list returns equal numbers", () => {
  const steps = [step({ id: "a", number: 1 }), step({ id: "b", number: 2 })];
  assert.deepEqual(renumber(steps), steps);
});

check("renumber fixes numbers that are out of order (e.g. restored from undo)", () => {
  const steps = [step({ id: "x", number: 7 }), step({ id: "y", number: 7 })];
  assert.deepEqual(renumber(steps).map((s) => s.number), [1, 2]);
});

check("hitsStep is a circle of radius size / 2 plus slack", () => {
  const s = step({ x: 100, y: 100, size: 32 });
  assert.ok(hitsStep(s, 100, 100));
  assert.ok(hitsStep(s, 116, 100));
  assert.ok(!hitsStep(s, 117, 100));
  assert.ok(hitsStep(s, 117, 100, 2));
  // The corner of the bounding box is outside the circle
  assert.ok(!hitsStep(s, 114, 114));
});

check("topmostStepAt prefers the later (drawn on top) step", () => {
  const a = step({ id: "a", x: 100, y: 100 });
  const b = step({ id: "b", x: 110, y: 100 });
  assert.equal(topmostStepAt([a, b], 105, 100)?.id, "b");
  assert.equal(topmostStepAt([a, b], 86, 100)?.id, "a");
  assert.equal(topmostStepAt([a, b], 200, 200), null);
});

check("annotationScale widens the hit circle with the drawn badge", () => {
  // 直径 32 x 2 = 半径 32 で描かれるので、中心から 30 の点は当たる (倍率 1 なら外れる)
  assert.equal(hitsStep(step(), 130, 100), false);
  assert.equal(hitsStep(step(), 130, 100, 0, 2), true);
  assert.equal(topmostStepAt([step()], 130, 100, 0, 2)?.id, "a");
});

check("stepFontSize shrinks for 2 and 3 digit numbers", () => {
  assert.ok(stepFontSize(32, 9) > stepFontSize(32, 10));
  assert.ok(stepFontSize(32, 10) > stepFontSize(32, 100));
  assert.equal(stepFontSize(32, 1), 19);
});

check("stepLabelBaseline puts the baseline below the center by the fixed ratio", () => {
  assert.equal(stepLabelBaseline(100, 20), 100 + 20 * STEP_LABEL_BASELINE_RATIO);
  assert.ok(STEP_LABEL_BASELINE_RATIO > 0.3 && STEP_LABEL_BASELINE_RATIO < 0.4);
});

if (failures > 0) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
