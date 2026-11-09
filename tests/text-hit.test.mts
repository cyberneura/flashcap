import assert from "node:assert/strict";
import { DRAG_SLOP_PX, exceedsDragSlop, hitsText, textBBox, topmostTextAt } from "../src/lib/textHit.ts";

type Text = { id: string; x: number; y: number; text: string; fontSize: number };

const text = (over: Partial<Text> = {}): Text => ({
  id: "a",
  x: 0,
  y: 0,
  text: "hello",
  fontSize: 20,
  ...over,
});

// 箱

{
  // 1 行の箱は、字数 x font-size x 0.6 に余白。高さは 1 行ぶん。
  const bbox = textBBox(text({ text: "hello", fontSize: 20 }));
  assert.equal(bbox.width, 5 * 20 * 0.6 + 8);
  assert.equal(bbox.height, 20 * 1.3 + 8);
}

{
  // 複数行は、いちばん長い行の幅と、行数ぶんの高さ
  const bbox = textBBox(text({ text: "ab\nabcd\na", fontSize: 10 }));
  assert.equal(bbox.width, 4 * 10 * 0.6 + 8);
  assert.equal(bbox.height, 10 * 1.3 * 3 + 8);
}

{
  // 入力を始めたばかりの空の注釈も掴める大きさを持つ。掴めないと、選ぶことも
  // 消すこともできないまま画面に残る。
  const bbox = textBBox(text({ text: "", fontSize: 20 }));
  assert.ok(bbox.width >= 40);
  assert.ok(bbox.height > 0);
  assert.ok(hitsText(text({ text: "" }), 5, 5));
}

// 当たり判定

{
  const t = text({ x: 100, y: 50, text: "ab", fontSize: 10 });
  const bbox = textBBox(t);
  assert.ok(hitsText(t, 100, 50), "左上の角は中");
  assert.ok(hitsText(t, 100 + bbox.width, 50 + bbox.height), "右下の角も中");
  assert.ok(!hitsText(t, 99, 50), "1px 左は外");
  assert.ok(!hitsText(t, 100, 49), "1px 上は外");
  assert.ok(!hitsText(t, 100 + bbox.width + 1, 50), "1px 右は外");
}

// 重なり

{
  // 後ろに足したものが手前に描かれるので、末尾から探す。
  const back = text({ id: "back", x: 0, y: 0 });
  const front = text({ id: "front", x: 0, y: 0 });
  assert.equal(topmostTextAt([back, front], 5, 5)?.id, "front");
}

{
  assert.equal(topmostTextAt([text({ x: 0, y: 0 })], 1000, 1000), null);
  assert.equal(topmostTextAt([], 0, 0), null);
}

// ドラッグの閾値

{
  // ダブルクリックの 2 回目の押下も移動の開始なので、指の微小なブレで
  // 「動かした」と扱うと、編集を開くたびに文字がずれて undo が積まれる。
  assert.ok(!exceedsDragSlop(0, 0, 1));
  assert.ok(!exceedsDragSlop(1, 1, 1));
  assert.ok(exceedsDragSlop(DRAG_SLOP_PX, 0, 1));
  assert.ok(exceedsDragSlop(0, -DRAG_SLOP_PX, 1), "向きは問わない");
}

{
  // 座標は自然解像度なので、縮小表示ほど小さな dx で閾値に届く。
  // 見た目の距離で判断しないと、縮小時だけ閾値が実質的に大きくなる。
  assert.ok(exceedsDragSlop(1, 0, 2), "2 倍表示なら 1px の座標差が 2px の見た目");
  assert.ok(!exceedsDragSlop(1, 0, 0.5), "縮小表示では同じ座標差でも届かない");
}

{
  // scale が 0 で来ても 0 除算にしない
  assert.ok(!exceedsDragSlop(0, 0, 0));
  assert.ok(exceedsDragSlop(100, 0, 0));
}

{
  // Retina の撮影 (倍率 2) では字面が 2 倍で描かれるので、箱も余白ごと 2 倍になる
  const bbox = textBBox(text({ text: "hello", fontSize: 20 }), 2);
  assert.equal(bbox.width, (5 * 20 * 0.6 + 8) * 2);
  assert.equal(bbox.height, (20 * 1.3 + 8) * 2);
  const inDoubledBox = bbox.width - 1;
  assert.ok(!hitsText(text(), inDoubledBox, 1), "倍率 1 なら箱の外");
  assert.ok(hitsText(text(), inDoubledBox, 1, 2), "倍率 2 なら箱の中");
  assert.equal(topmostTextAt([text()], inDoubledBox, 1, 2)?.id, "a");
}

console.log("text-hit: ok");
