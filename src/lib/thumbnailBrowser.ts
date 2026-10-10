// サムネイルブラウザ (左サイドバー) の表示用の純関数。Rust の thumbnails.rs が返す一覧を整形する

/** Rust の `thumbnails::SavedImage` */
export interface SavedImage {
  path: string;
  name: string;
  /** 作成日時 (UNIX ミリ秒) */
  created_ms: number;
  /** 更新日時 (UNIX ミリ秒) */
  modified_ms: number;
  size: number;
}

/** サイドバーの表示 / 非表示を持つ設定ファイルのキー (Rust の THUMBNAIL_SIDEBAR_KEY と同じ) */
export const THUMBNAIL_SIDEBAR_KEY = "thumbnail_sidebar";

const RELATIVE_UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 60 * 60 * 1000],
  ["month", 30 * 24 * 60 * 60 * 1000],
  ["week", 7 * 24 * 60 * 60 * 1000],
  ["day", 24 * 60 * 60 * 1000],
  ["hour", 60 * 60 * 1000],
  ["minute", 60 * 1000],
];

const relativeFormat = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

/** "just now" / "5 minutes ago" / "yesterday" のような相対時刻 */
export function formatRelativeTime(timeMs: number, nowMs: number): string {
  // 時計のずれで少し未来の時刻になっても "in 1 minute" とは出さない
  const elapsed = Math.max(0, nowMs - timeMs);
  for (const [unit, ms] of RELATIVE_UNITS) {
    if (elapsed >= ms) return relativeFormat.format(-Math.floor(elapsed / ms), unit);
  }
  return "just now";
}

/** "820 B" / "12.3 KB" / "4.5 MB" (1024 単位。Finder の表示ではなく見積もりとして十分な精度) */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

/**
 * 作ったサムネイルを取り置くキー。注釈を書き戻すとパスはそのままで中身が変わるので、
 * 更新日時と容量も含めて、変わったものは作り直させる
 */
export function thumbnailCacheKey(image: SavedImage): string {
  return `${image.path}\u0000${image.modified_ms}\u0000${image.size}`;
}
