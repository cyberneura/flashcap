import assert from "node:assert/strict";
import { collapseHome } from "../src/lib/homePath.ts";

{
  // ホームフォルダの下は ~ に畳む
  assert.equal(collapseHome("/Users/alice/Pictures/shots", "/Users/alice"), "~/Pictures/shots");
  assert.equal(collapseHome("/Users/alice", "/Users/alice"), "~");
  // homeDir() が末尾の区切りを付けて返しても同じ
  assert.equal(collapseHome("/Users/alice/x", "/Users/alice/"), "~/x");
}

{
  // 名前が前方一致するだけの別のフォルダは畳まない
  assert.equal(collapseHome("/Users/alice2/x", "/Users/alice"), "/Users/alice2/x");
  // ホームの外はそのまま
  assert.equal(collapseHome("/Volumes/Share/x", "/Users/alice"), "/Volumes/Share/x");
}

{
  // Windows の区切り
  assert.equal(collapseHome("C:\\Users\\alice\\Pictures", "C:\\Users\\alice"), "~\\Pictures");
}

{
  // ホームが分からない時は何もしない
  assert.equal(collapseHome("/Users/alice/x", ""), "/Users/alice/x");
}

console.log("home-path: ok");
