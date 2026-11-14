import assert from "node:assert/strict";
import {
  clearEditHistory,
  createEditHistory,
  historyActionForKey,
  isTextEntryTarget,
  recordEdit,
  stepRedo,
  stepUndo,
} from "../src/lib/editHistory.ts";

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

// A small stand-in for the editor: the annotations plus the base image and its size,
// driven through the history the same way +page.svelte does (pushUndo before each edit,
// applySnapshot with whatever undo / redo hands back).
interface Snapshot {
  arrows: string[];
  image: string;
  width: number;
  height: number;
}

function editor() {
  const history = createEditHistory<Snapshot>();
  let state: Snapshot = { arrows: [], image: "full", width: 800, height: 600 };
  const capture = (): Snapshot => ({ ...state, arrows: [...state.arrows] });
  return {
    history,
    get state() {
      return state;
    },
    edit(change: (s: Snapshot) => void) {
      recordEdit(history, capture());
      change(state);
    },
    undo() {
      const entry = stepUndo(history, capture);
      if (entry) state = entry;
      return entry !== null;
    },
    redo() {
      const entry = stepRedo(history, capture);
      if (entry) state = entry;
      return entry !== null;
    },
  };
}

check("undo then redo returns to the state before the undo", () => {
  const ed = editor();
  ed.edit((s) => s.arrows.push("a"));
  ed.edit((s) => s.arrows.push("b"));
  assert.equal(ed.undo(), true);
  assert.deepEqual(ed.state.arrows, ["a"]);
  assert.equal(ed.redo(), true);
  assert.deepEqual(ed.state.arrows, ["a", "b"]);
});

check("several undos can be redone in the reverse order", () => {
  const ed = editor();
  ed.edit((s) => s.arrows.push("a"));
  ed.edit((s) => s.arrows.push("b"));
  ed.edit((s) => s.arrows.push("c"));
  ed.undo();
  ed.undo();
  ed.undo();
  assert.deepEqual(ed.state.arrows, []);
  ed.redo();
  assert.deepEqual(ed.state.arrows, ["a"]);
  ed.redo();
  ed.redo();
  assert.deepEqual(ed.state.arrows, ["a", "b", "c"]);
  // Nothing left to redo
  assert.equal(ed.redo(), false);
  assert.deepEqual(ed.state.arrows, ["a", "b", "c"]);
  // ... and everything can be undone again
  ed.undo();
  ed.undo();
  ed.undo();
  assert.deepEqual(ed.state.arrows, []);
  assert.equal(ed.undo(), false);
});

check("a new edit after an undo drops the redo history", () => {
  const ed = editor();
  ed.edit((s) => s.arrows.push("a"));
  ed.edit((s) => s.arrows.push("b"));
  ed.undo();
  ed.edit((s) => s.arrows.push("x"));
  assert.equal(ed.redo(), false);
  assert.deepEqual(ed.state.arrows, ["a", "x"]);
  // The undo history still leads back through the new edit
  ed.undo();
  assert.deepEqual(ed.state.arrows, ["a"]);
});

check("undo / redo of a crop restores the image and its dimensions", () => {
  const ed = editor();
  ed.edit((s) => s.arrows.push("a"));
  ed.edit((s) => {
    s.image = "cropped";
    s.width = 400;
    s.height = 300;
  });
  ed.undo();
  assert.deepEqual(ed.state, { arrows: ["a"], image: "full", width: 800, height: 600 });
  ed.redo();
  assert.deepEqual(ed.state, { arrows: ["a"], image: "cropped", width: 400, height: 300 });
});

check("undo / redo on an empty history do nothing and do not capture the state", () => {
  const history = createEditHistory<number>();
  let captured = 0;
  const capture = () => {
    captured++;
    return 1;
  };
  assert.equal(stepUndo(history, capture), null);
  assert.equal(stepRedo(history, capture), null);
  assert.equal(captured, 0);
  assert.deepEqual(history, { undo: [], redo: [] });
});

check("clearEditHistory empties both directions", () => {
  const ed = editor();
  ed.edit((s) => s.arrows.push("a"));
  ed.edit((s) => s.arrows.push("b"));
  ed.undo();
  clearEditHistory(ed.history);
  assert.equal(ed.undo(), false);
  assert.equal(ed.redo(), false);
});

const key = (k: string, over: Partial<{ mod: boolean; shift: boolean; alt: boolean }> = {}) => ({
  key: k,
  mod: true,
  shift: false,
  alt: false,
  ...over,
});

check("mod+Z is undo and mod+Shift+Z is redo on both platforms", () => {
  for (const windows of [false, true]) {
    assert.equal(historyActionForKey(key("z"), windows), "undo");
    assert.equal(historyActionForKey(key("z", { shift: true }), windows), "redo");
    // Windows reports an uppercase key while Shift is held
    assert.equal(historyActionForKey(key("Z", { shift: true }), windows), "redo");
  }
});

check("Ctrl+Y is redo on Windows only", () => {
  assert.equal(historyActionForKey(key("y"), true), "redo");
  assert.equal(historyActionForKey(key("y"), false), null);
  assert.equal(historyActionForKey(key("Y", { shift: true }), true), null);
});

check("keys without the modifier, with Alt, or other keys are not history shortcuts", () => {
  assert.equal(historyActionForKey(key("z", { mod: false }), false), null);
  assert.equal(historyActionForKey(key("z", { mod: false, shift: true }), true), null);
  assert.equal(historyActionForKey(key("z", { alt: true }), false), null);
  assert.equal(historyActionForKey(key("c"), false), null);
  assert.equal(historyActionForKey(key("s"), true), null);
});

check("text entry targets keep their own undo / redo", () => {
  assert.equal(isTextEntryTarget({ tagName: "TEXTAREA" }), true);
  assert.equal(isTextEntryTarget({ tagName: "textarea" }), true);
  assert.equal(isTextEntryTarget({ tagName: "INPUT", type: "text" }), true);
  assert.equal(isTextEntryTarget({ tagName: "INPUT", type: "number" }), true);
  assert.equal(isTextEntryTarget({ tagName: "INPUT" }), true);
  assert.equal(isTextEntryTarget({ tagName: "DIV", isContentEditable: true }), true);
});

check("non-text controls and plain elements do not block the annotation history", () => {
  assert.equal(isTextEntryTarget({ tagName: "INPUT", type: "color" }), false);
  assert.equal(isTextEntryTarget({ tagName: "INPUT", type: "range" }), false);
  assert.equal(isTextEntryTarget({ tagName: "INPUT", type: "radio" }), false);
  assert.equal(isTextEntryTarget({ tagName: "INPUT", type: "checkbox" }), false);
  assert.equal(isTextEntryTarget({ tagName: "BUTTON" }), false);
  assert.equal(isTextEntryTarget({ tagName: "BODY", isContentEditable: false }), false);
  assert.equal(isTextEntryTarget(null), false);
  assert.equal(isTextEntryTarget(undefined), false);
});

if (failures > 0) {
  console.log(`${failures} failure(s)`);
  process.exit(1);
}
