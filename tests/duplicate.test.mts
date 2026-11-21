import assert from "node:assert/strict";
import {
  DUPLICATE_OFFSET,
  isDuplicateKey,
  offsetArrowCopy,
  offsetCopy,
  stepsWithDuplicate,
  type DuplicateKeyEvent,
} from "../src/lib/duplicate.ts";
import { renumber } from "../src/lib/steps.ts";
import type { Arrow, Step } from "../src/lib/types.ts";

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

function key(k: string, over: Partial<DuplicateKeyEvent> = {}): DuplicateKeyEvent {
  return { key: k, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, isComposing: false, repeat: false, ...over };
}
const body = { tagName: "BODY" };

check("Cmd+D on macOS and Ctrl+D on Windows duplicate", () => {
  assert.equal(isDuplicateKey(key("d", { metaKey: true }), body, false), true);
  assert.equal(isDuplicateKey(key("d", { ctrlKey: true }), body, true), true);
  // CapsLock
  assert.equal(isDuplicateKey(key("D", { metaKey: true }), body, false), true);
});

check("the other platform's modifier does not duplicate", () => {
  assert.equal(isDuplicateKey(key("d", { ctrlKey: true }), body, false), false);
  assert.equal(isDuplicateKey(key("d", { metaKey: true }), body, true), false);
  assert.equal(isDuplicateKey(key("d", { metaKey: true, ctrlKey: true }), body, false), false);
});

check("extra modifiers, a bare D and other letters do not duplicate", () => {
  assert.equal(isDuplicateKey(key("d"), body, false), false);
  assert.equal(isDuplicateKey(key("D", { metaKey: true, shiftKey: true }), body, false), false);
  assert.equal(isDuplicateKey(key("d", { metaKey: true, altKey: true }), body, false), false);
  assert.equal(isDuplicateKey(key("c", { metaKey: true }), body, false), false);
});

check("typing, IME composition and key repeat are left alone", () => {
  for (const tagName of ["INPUT", "TEXTAREA", "SELECT", "input"]) {
    assert.equal(isDuplicateKey(key("d", { metaKey: true }), { tagName }, false), false, tagName);
  }
  assert.equal(isDuplicateKey(key("d", { metaKey: true }), { tagName: "DIV", isContentEditable: true }, false), false);
  assert.equal(isDuplicateKey(key("d", { metaKey: true, isComposing: true }), body, false), false);
  assert.equal(isDuplicateKey(key("Process", { metaKey: true }), body, false), false);
  assert.equal(isDuplicateKey(key("d", { metaKey: true, repeat: true }), body, false), false);
  assert.equal(isDuplicateKey(key("d", { metaKey: true }), null, false), true);
});

check("offsetCopy moves the copy and keeps the rest, without touching the source", () => {
  const src = { id: "a", x: 10, y: 20, width: 30, height: 40, color: "#f00" };
  const copy = offsetCopy(src, DUPLICATE_OFFSET, DUPLICATE_OFFSET, "b");
  assert.deepEqual(copy, { id: "b", x: 26, y: 36, width: 30, height: 40, color: "#f00" });
  assert.deepEqual(src, { id: "a", x: 10, y: 20, width: 30, height: 40, color: "#f00" });
});

check("text copies keep their text", () => {
  const t = { id: "t", x: 0, y: 0, text: "hello\nworld", fontSize: 20 };
  assert.equal(offsetCopy(t, 1, 2, "u").text, "hello\nworld");
});

check("offsetArrowCopy moves both ends", () => {
  const a: Arrow = { id: "a", startX: 1, startY: 2, endX: 30, endY: 40, color: "#f00", thickness: 4, whiteStroke: true, dropShadow: false };
  const c = offsetArrowCopy(a, 5, -3, "b");
  assert.deepEqual([c.id, c.startX, c.startY, c.endX, c.endY], ["b", 6, -1, 35, 37]);
  assert.equal(c.thickness, 4);
});

const step = (id: string, number: number, x = 0): Step => ({ id, x, y: 0, number, color: "#f00", size: 32, whiteStroke: true, dropShadow: false });

check("a duplicated step gets the next number and the others keep theirs", () => {
  const steps = [step("a", 1), step("b", 2), step("c", 3)];
  const out = stepsWithDuplicate(steps, steps[0], 16, 16, "d");
  assert.deepEqual(out.map((s) => [s.id, s.number]), [["a", 1], ["b", 2], ["c", 3], ["d", 4]]);
  assert.equal(out[3].x, 16);
});

check("duplicate, delete and undo keep the numbering consistent", () => {
  const before = [step("a", 1), step("b", 2)];
  const dup = stepsWithDuplicate(before, before[1], 16, 16, "c");
  assert.deepEqual(dup.map((s) => s.number), [1, 2, 3]);
  // 1 を消すと詰め直される (StepOverlay の Delete と同じ)
  const deleted = renumber(dup.filter((s) => s.id !== "a"));
  assert.deepEqual(deleted.map((s) => [s.id, s.number]), [["b", 1], ["c", 2]]);
  // Undo はスナップショット (dup) に戻すだけで、番号も並び順どおり
  assert.deepEqual(renumber(dup).map((s) => s.number), [1, 2, 3]);
  // 消した後に複製すると、残りの数の次になる
  const again = stepsWithDuplicate(deleted, deleted[0], 16, 16, "d");
  assert.deepEqual(again.map((s) => s.number), [1, 2, 3]);
});

if (failures > 0) {
  console.log(`${failures} test(s) failed`);
  process.exit(1);
}
