import assert from "node:assert/strict";
import { formatFileSize, formatRelativeTime, thumbnailCacheKey } from "../src/lib/thumbnailBrowser.ts";

const MIN = 60 * 1000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const now = Date.UTC(2026, 9, 10, 12, 0, 0);

{
  // 1 分未満は "just now"。時計のずれで少し未来になっても同じ
  assert.equal(formatRelativeTime(now, now), "just now");
  assert.equal(formatRelativeTime(now - 59 * 1000, now), "just now");
  assert.equal(formatRelativeTime(now + 30 * 1000, now), "just now");
}

{
  // 単位は切り捨てで一番大きいもの
  assert.equal(formatRelativeTime(now - MIN, now), "1 minute ago");
  assert.equal(formatRelativeTime(now - 59 * MIN, now), "59 minutes ago");
  assert.equal(formatRelativeTime(now - 2 * HOUR - 5 * MIN, now), "2 hours ago");
  assert.equal(formatRelativeTime(now - DAY, now), "yesterday");
  assert.equal(formatRelativeTime(now - 3 * DAY, now), "3 days ago");
  assert.equal(formatRelativeTime(now - 8 * DAY, now), "last week");
  assert.equal(formatRelativeTime(now - 45 * DAY, now), "last month");
  assert.equal(formatRelativeTime(now - 800 * DAY, now), "2 years ago");
}

{
  assert.equal(formatFileSize(0), "0 B");
  assert.equal(formatFileSize(1023), "1023 B");
  assert.equal(formatFileSize(1024), "1.0 KB");
  assert.equal(formatFileSize(12 * 1024 + 300), "12 KB");
  assert.equal(formatFileSize(4.5 * 1024 * 1024), "4.5 MB");
  assert.equal(formatFileSize(3 * 1024 ** 4), "3072 GB");
}

{
  // 書き戻しで更新日時か容量が変わったら別のキーになる (サムネイルを作り直す)
  const base = { path: "/a/b.png", name: "b.png", created_ms: 1, modified_ms: 2, size: 3 };
  assert.equal(thumbnailCacheKey(base), thumbnailCacheKey({ ...base }));
  assert.notEqual(thumbnailCacheKey(base), thumbnailCacheKey({ ...base, modified_ms: 4 }));
  assert.notEqual(thumbnailCacheKey(base), thumbnailCacheKey({ ...base, size: 4 }));
  // 作成日時だけ変わっても中身は同じ
  assert.equal(thumbnailCacheKey(base), thumbnailCacheKey({ ...base, created_ms: 9 }));
}

console.log("thumbnail-browser: ok");
