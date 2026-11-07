/**
 * OCR の結果から、隠すべき文字列 (メール / IP / API キーやトークン / クレジットカード番号) を含む行を
 * 選ぶ (CYBERNEURA-DEV-1210、「Redact sensitive text」)。純粋な関数で、判定は行のテキストだけを見る。
 * 検出した行の箱にマスクを置くのは `+page.svelte` (`redactSensitiveText`)。
 *
 * 誤検出は Delete で外せ、検出漏れは手でマスクを足せるので、ここは「見落としそうなものを拾う」
 * 側に寄せつつ、普通の単語や番号まで隠さない程度に絞る。
 */

/** `ocr.swift --json` の 1 行 (src-tauri/src/ocr_lines.rs の OcrLine と同じ形) */
export interface OcrLine {
  text: string;
  /** 画像の自然解像度ピクセル、左上原点の [x, y, w, h] */
  box: [number, number, number, number];
}

export type SensitiveKind = "email" | "ip" | "token" | "jwt" | "secret" | "card";

// 1 行の判定に使う文字数の上限。OCR の 1 行は短いが、UI スレッドで動くので入力の長さで
// 仕事量が伸びないようにする (超えた分は見ない)
const MAX_LINE_CHARS = 2000;
// メールは空白区切りのトークン単位で、先頭固定の正規表現で判定する (行全体に `[^\s@]+@` を
// 当てると、@ の無い長い行で開始位置ごとに末尾まで走査して二乗時間になる)
const EMAIL_TOKEN = /^[^@]+@[^@.]+(?:\.[^@.]+)*\.[a-z]{2,}$/i;
const IPV4 = /(?<![\d.])(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}(?![\d.])/;
// 代表的なキーの接頭辞 (OpenAI / GitHub / Slack / AWS)。接頭辞の後ろは英数字と _ - が続く
const TOKEN_PREFIX = /(?:^|[^A-Za-z0-9])(?:sk-|ghp_|gho_|xox[abp]-|AKIA)[A-Za-z0-9_-]{8,}/;
// JWT 風: base64url の 3 つの部分を . で繋いだもの (各部 20 文字以上)
const JWT = /(?:^|[^A-Za-z0-9_-])[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}/;
// 16 文字以上の英数字だけの連続 (空白なし)。普通の長い単語 (internationalization) や
// 数字だけの ID を拾わないよう、数字と英字の両方を含むものに限る
const LONG_ALNUM = /(?:^|[^A-Za-z0-9])(?=[A-Za-z0-9]{16,}(?![A-Za-z0-9]))(?=[A-Za-z0-9]*\d)(?=[A-Za-z0-9]*[A-Za-z])[A-Za-z0-9]{16,}/;
// クレジットカード風: 13〜19 桁の数字 (空白 / ハイフン区切り可)。数字のかたまりを区切りごとに
// 分け、連続するかたまりの組み合わせで 13〜19 桁になるものを Luhn で検査する
// (`Order 1 4242 4242 4242 4242` の先頭の `1` に引きずられて 17 桁を 1 候補にしない)
const CARD_MIN_DIGITS = 13;
const CARD_MAX_DIGITS = 19;

/** Luhn 検査 (カード番号の末尾のチェックディジット) */
export function luhnValid(digits: string): boolean {
  if (!/^\d{13,19}$/.test(digits)) return false;
  let sum = 0;
  let double = false;
  for (let i = digits.length - 1; i >= 0; i--) {
    let d = digits.charCodeAt(i) - 48;
    if (double) {
      d *= 2;
      if (d > 9) d -= 9;
    }
    sum += d;
    double = !double;
  }
  return sum % 10 === 0;
}

function hasEmail(text: string): boolean {
  for (const token of text.split(/\s+/)) {
    if (!token.includes("@")) continue;
    // 末尾の句読点・括弧は外して判定する ("mail: a@b.com)" のような行)
    const bare = token.replace(/^[(<["']+|[)>\]"',.;:]+$/g, "");
    if (EMAIL_TOKEN.test(bare)) return true;
  }
  return false;
}

/** 数字のかたまり (空白 / ハイフンで区切られた連続) の並びごとに、13〜19 桁の組み合わせを Luhn で検査 */
function looksLikeCard(text: string): boolean {
  for (const run of text.matchAll(/\d(?:[ -]?\d)*/g)) {
    const groups = run[0].split(/[ -]/);
    for (let i = 0; i < groups.length; i++) {
      let digits = "";
      for (let j = i; j < groups.length; j++) {
        digits += groups[j];
        if (digits.length > CARD_MAX_DIGITS) break;
        if (digits.length >= CARD_MIN_DIGITS && luhnValid(digits)) return true;
      }
    }
  }
  return false;
}

/** その行がどの種類の秘密を含むか (含まなければ null)。種類は最初に当たったもの */
export function classifySensitive(line: string): SensitiveKind | null {
  const text = line.length > MAX_LINE_CHARS ? line.slice(0, MAX_LINE_CHARS) : line;
  if (hasEmail(text)) return "email";
  if (IPV4.test(text)) return "ip";
  if (TOKEN_PREFIX.test(text)) return "token";
  if (JWT.test(text)) return "jwt";
  if (LONG_ALNUM.test(text)) return "secret";
  if (looksLikeCard(text)) return "card";
  return null;
}

/** 隠すべき文字列を含む行だけを (元の順で) 返す */
export function findSensitive(lines: readonly OcrLine[]): OcrLine[] {
  return lines.filter((line) => classifySensitive(line.text) !== null);
}

/** 行の箱を上下左右に `pad` 広げたマスクの矩形 (画像の外にはみ出さない) */
export function maskRectFor(
  line: OcrLine,
  pad: number,
  imageWidth: number,
  imageHeight: number,
): { x: number; y: number; width: number; height: number } {
  const [bx, by, bw, bh] = line.box;
  const x = Math.max(0, bx - pad);
  const y = Math.max(0, by - pad);
  const right = Math.min(imageWidth, bx + bw + pad);
  const bottom = Math.min(imageHeight, by + bh + pad);
  return { x, y, width: Math.max(0, right - x), height: Math.max(0, bottom - y) };
}
