import assert from "node:assert/strict";
import {
  TOOL_SHORTCUTS,
  isFormControlTarget,
  toolForKey,
  toolShortcutLabel,
  type ToolKeyEvent,
} from "../src/lib/toolShortcuts.ts";

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

const key = (k: string, over: Partial<ToolKeyEvent> = {}): ToolKeyEvent => ({
  key: k,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  isComposing: false,
  repeat: false,
  ...over,
});
const ready = { hasImage: true, busy: false };
const body = { tagName: "BODY" };

check("each key selects its tool", () => {
  assert.equal(toolForKey(key("a"), body, ready), "arrow");
  assert.equal(toolForKey(key("r"), body, ready), "rect");
  assert.equal(toolForKey(key("e"), body, ready), "ellipse");
  assert.equal(toolForKey(key("h"), body, ready), "highlight");
  assert.equal(toolForKey(key("n"), body, ready), "step");
  assert.equal(toolForKey(key("t"), body, ready), "text");
  assert.equal(toolForKey(key("m"), body, ready), "mask");
  assert.equal(toolForKey(key("c"), body, ready), "crop");
});

check("the table has no duplicate keys or tools, and keys are single lowercase letters", () => {
  assert.equal(new Set(TOOL_SHORTCUTS.map((s) => s.key)).size, TOOL_SHORTCUTS.length);
  assert.equal(new Set(TOOL_SHORTCUTS.map((s) => s.tool)).size, TOOL_SHORTCUTS.length);
  for (const s of TOOL_SHORTCUTS) assert.match(s.key, /^[a-z]$/);
});

check("the keys do not collide with the letters of the mod-key shortcuts being typed alone", () => {
  // Cmd/Ctrl + C / V / S / Z / Y exist, but only with the modifier; the plain letters are free.
  // "c" is used for crop on purpose; with the modifier it must still fall through (next test).
  assert.equal(toolForKey(key("v"), body, ready), null);
  assert.equal(toolForKey(key("s"), body, ready), null);
  assert.equal(toolForKey(key("z"), body, ready), null);
});

check("keys with a modifier fall through to the existing shortcuts", () => {
  assert.equal(toolForKey(key("c", { metaKey: true }), body, ready), null);
  assert.equal(toolForKey(key("c", { ctrlKey: true }), body, ready), null);
  assert.equal(toolForKey(key("C", { metaKey: true, shiftKey: true }), body, ready), null);
  assert.equal(toolForKey(key("a", { altKey: true }), body, ready), null);
  assert.equal(toolForKey(key("A", { shiftKey: true }), body, ready), null);
});

check("CapsLock (uppercase without Shift) still selects the tool", () => {
  assert.equal(toolForKey(key("A"), body, ready), "arrow");
  assert.equal(toolForKey(key("T"), body, ready), "text");
});

check("typing in a form control does not switch tools", () => {
  assert.equal(toolForKey(key("t"), { tagName: "TEXTAREA" }, ready), null);
  assert.equal(toolForKey(key("a"), { tagName: "INPUT" }, ready), null);
  assert.equal(toolForKey(key("m"), { tagName: "SELECT" }, ready), null);
  assert.equal(toolForKey(key("c"), { tagName: "DIV", isContentEditable: true }, ready), null);
  // A focused toolbar button is not a text target
  assert.equal(toolForKey(key("a"), { tagName: "BUTTON" }, ready), "arrow");
  assert.equal(toolForKey(key("a"), null, ready), "arrow");
});

check("isFormControlTarget covers inputs, textareas, selects and contentEditable", () => {
  assert.equal(isFormControlTarget({ tagName: "input" }), true);
  assert.equal(isFormControlTarget({ tagName: "TEXTAREA" }), true);
  assert.equal(isFormControlTarget({ tagName: "SELECT" }), true);
  assert.equal(isFormControlTarget({ tagName: "SPAN", isContentEditable: true }), true);
  assert.equal(isFormControlTarget({ tagName: "BUTTON" }), false);
  assert.equal(isFormControlTarget({ tagName: "svg" }), false);
  assert.equal(isFormControlTarget(undefined), false);
});

check("IME composition and key repeat are ignored", () => {
  assert.equal(toolForKey(key("a", { isComposing: true }), body, ready), null);
  assert.equal(toolForKey(key("Process"), body, ready), null);
  assert.equal(toolForKey(key("a", { repeat: true }), body, ready), null);
});

check("nothing happens without an image, or while capturing / recording", () => {
  assert.equal(toolForKey(key("a"), body, { hasImage: false, busy: false }), null);
  assert.equal(toolForKey(key("a"), body, { hasImage: true, busy: true }), null);
});

check("other keys are not tool shortcuts", () => {
  for (const k of ["b", "x", "1", " ", "Enter", "Escape", "Delete", "ArrowLeft", "Shift", "あ"]) {
    assert.equal(toolForKey(key(k), body, ready), null);
  }
});

check("toolShortcutLabel gives the uppercase key for tooltips", () => {
  assert.equal(toolShortcutLabel("arrow"), "A");
  assert.equal(toolShortcutLabel("rect"), "R");
  assert.equal(toolShortcutLabel("ellipse"), "E");
  assert.equal(toolShortcutLabel("highlight"), "H");
  assert.equal(toolShortcutLabel("step"), "N");
  assert.equal(toolShortcutLabel("text"), "T");
  assert.equal(toolShortcutLabel("mask"), "M");
  assert.equal(toolShortcutLabel("crop"), "C");
});

if (failures > 0) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
