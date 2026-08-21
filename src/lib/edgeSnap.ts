/**
 * 検出された 1 本の線。
 *
 * 座標は「画素 i-1 と i の**間**」を表す自然解像度ピクセル値なので、CropRect の
 * x / x+width / y / y+height のどれにもそのまま使える (どれも「境界の座標」であって
 * 「画素の座標」ではない)。
 *
 * **1 本の線が start と end の 2 つの境界を持つ。** 1px の枠線は左右 2 本の境界を作り、
 * 「枠線を含めて切る」「外して切る」のどちらも正当な意図なので、両方を吸着先にする。
 * アンチエイリアスで滲んだ遷移帯も 1 本にまとまり、両端だけが残る。単純な段差
 * (白→灰が 1 箇所で切り替わる) では start === end になる。
 */
export interface EdgeSnapRun {
  start: number;
  end: number;
  /** 境界をまたぐ 1 画素あたりの平均チャンネル差 (0-255)。間引きで強い方を残すのに使う */
  strength: number;
}

export interface EdgeSnapLines {
  xs: EdgeSnapRun[];
  ys: EdgeSnapRun[];
}

/** 走査する行 / 列の上限。これを超える画像は間引いて読む */
const MAX_SCAN_LINES = 1024;

/** 候補と認める、境界をまたぐ 1 画素あたりの平均チャンネル差 (0-255) */
const MIN_EDGE_STRENGTH = 10;

/**
 * 近傍平均に対して何倍あれば「線」と認めるか。
 *
 * 絶対値の閾値だけだと、写真やテクスチャのようにどこを切っても差分が出る画像で
 * ほぼ全列が候補になり、吸着先が実質ランダムになって細かい位置合わせができなくなる。
 * UI の枠線は周囲から突出しているので、この比で写真と切り分けられる。
 */
const PROMINENCE_RATIO = 1.5;

/** 近傍平均を取る半径 */
const PROMINENCE_RADIUS = 12;

/** これだけ近い境界同士は 1 本の線 (アンチエイリアスの遷移帯) とみなす */
const LINE_GAP = 2;

/**
 * 1 枚の画像から返す線の本数の上限。
 *
 * これは**メモリと計算量の蓋であって、吸着の効きすぎを防ぐものではない**。
 * 画像 px 基準の上限は表示スケールを知らないので、縮小表示中に「画面上で何 px
 * 離れているか」を制御できない。操作不能を防ぐ間引きは snapPositions が
 * 表示スケール込みで行う。
 */
const LINES_PER_PIXEL = 1 / 40;
const MIN_LINES = 24;

/**
 * 画像から縦線 / 横線の候補を検出する。
 *
 * 射影プロファイル: 列 i の強さを「列 i-1 と i の間の画素差を全行ぶん足したもの」と
 * 定義する。スクリーンショットは長い直線の境界が支配的なので、これで十分よく取れる。
 * canvas を読めない (getImageData が失敗する) 場合は null を返し、呼び出し側は
 * 吸着なしで動く。
 */
export function detectEdgeSnapLines(img: HTMLImageElement): EdgeSnapLines | null {
  const w = img.naturalWidth;
  const h = img.naturalHeight;
  if (w < 2 || h < 2) return { xs: [], ys: [] };

  // **縮小して読まない。** 吸着先の座標は自然解像度で 1px の精度が要るので、
  // 縮小した画素から求めた位置では枠が線に乗らない。全画素ぶんのバッファを取るが、
  // renderComposite() / applyCrop() が保存やコピーのたびにやっているのと同じ規模で、
  // こちらは画像 1 枚につき 1 回しか走らない
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.drawImage(img, 0, 0);

  try {
    return snapLinesFromPixels(ctx.getImageData(0, 0, w, h).data, w, h);
  } catch {
    return null;
  } finally {
    // 読み終えたバッキングストア (数十 MB) を GC 待ちにせず即手放す
    canvas.width = 0;
    canvas.height = 0;
  }
}

/**
 * RGBA 画素列から候補線を求める (detectEdgeSnapLines の本体)。
 * DOM から切り離してあるのは、合成画像を作ってそのまま検証できるようにするため。
 */
export function snapLinesFromPixels(
  data: Uint8ClampedArray | Uint8Array,
  w: number,
  h: number
): EdgeSnapLines {
  if (w < 2 || h < 2) return { xs: [], ys: [] };

  const strideY = Math.max(1, Math.ceil(h / MAX_SCAN_LINES));
  const strideX = Math.max(1, Math.ceil(w / MAX_SCAN_LINES));

  // 縦線: 間引いた行だけを左から右へ舐める (行ごとに連続アクセスになる)
  const colScore = new Float64Array(w);
  let sampledRows = 0;
  for (let y = 0; y < h; y += strideY) {
    const base = y * w * 4;
    let pr = data[base];
    let pg = data[base + 1];
    let pb = data[base + 2];
    for (let x = 1; x < w; x++) {
      const i = base + x * 4;
      const r = data[i];
      const g = data[i + 1];
      const b = data[i + 2];
      colScore[x] += Math.abs(r - pr) + Math.abs(g - pg) + Math.abs(b - pb);
      pr = r;
      pg = g;
      pb = b;
    }
    sampledRows++;
  }

  // 横線: y の精度は落とせないので全行見るが、列は間引く
  const rowScore = new Float64Array(h);
  let sampledCols = 0;
  for (let x = 0; x < w; x += strideX) sampledCols++;
  for (let y = 1; y < h; y++) {
    const base = y * w * 4;
    const prev = (y - 1) * w * 4;
    let sum = 0;
    for (let x = 0; x < w; x += strideX) {
      const i = base + x * 4;
      const j = prev + x * 4;
      sum +=
        Math.abs(data[i] - data[j]) +
        Math.abs(data[i + 1] - data[j + 1]) +
        Math.abs(data[i + 2] - data[j + 2]);
    }
    rowScore[y] = sum;
  }

  return {
    xs: pickLines(colScore, w, sampledRows),
    ys: pickLines(rowScore, h, sampledCols),
  };
}

/**
 * 積算スコアから線を選ぶ。
 *
 * score[i] は「i-1 と i の間」の積算値で、score[0] は境界ではないので使わない。
 * 画像の端 (0 と size) はここには入れない — 検出された線ではないうえ、間引きの
 * 対象にすると端の近くにある本物の線を潰してしまう。端は snapPositions が足す。
 */
function pickLines(score: Float64Array, size: number, samples: number): EdgeSnapRun[] {
  if (samples <= 0 || size < 2) return [];

  // チャンネル 3 本ぶんの和なので、3 で割って 0-255 の平均差に戻す
  const norm = samples * 3;
  const strength = new Float64Array(size);
  for (let i = 1; i < size; i++) strength[i] = score[i] / norm;

  // 近傍平均を prefix sum で引く
  const prefix = new Float64Array(size + 1);
  for (let i = 0; i < size; i++) prefix[i + 1] = prefix[i] + strength[i];

  // 近接した境界は 1 本の線 (EdgeSnapRun) にまとめる。両端の扱いは型の説明を参照
  const runs: EdgeSnapRun[] = [];
  for (let i = 1; i < size; i++) {
    const v = strength[i];
    if (v < MIN_EDGE_STRENGTH) continue;
    const lo = Math.max(0, i - PROMINENCE_RADIUS);
    const hi = Math.min(size, i + PROMINENCE_RADIUS + 1);
    const localMean = (prefix[hi] - prefix[lo]) / (hi - lo);
    if (v < PROMINENCE_RATIO * localMean) continue;

    const last = runs[runs.length - 1];
    if (last && i - last.end <= LINE_GAP) {
      last.end = i;
      last.strength = Math.max(last.strength, v);
    } else {
      runs.push({ start: i, end: i, strength: v });
    }
  }

  // 多すぎる時は強い線から採る
  const limit = Math.max(MIN_LINES, Math.round(size * LINES_PER_PIXEL));
  if (runs.length > limit) {
    runs.sort((a, b) => b.strength - a.strength);
    runs.length = limit;
    runs.sort((a, b) => a.start - b.start);
  }
  return runs;
}

/** 2 本の線の間隔 (重なっていれば 0) */
function runDistance(a: EdgeSnapRun, b: EdgeSnapRun): number {
  return Math.max(b.start - a.end, a.start - b.end, 0);
}

/**
 * 残す線の最小間隔を、吸着の許容距離の何倍にするか。
 *
 * 1 本の線は前後 tolerance ぶん、つまり幅 2×tolerance を吸い込む。間隔をちょうど
 * 2 倍にすると吸い込む範囲が隙間なく連なり、**枠の辺を線以外の場所へ置けなくなる**。
 * 3 倍にすると線と線の間に tolerance ぶんの自由地帯が残る (辺を置ける位置の 1/3)。
 * これ以上広げると、間隔の詰まった UI (アイコンの列など) で本物の線が落ちすぎる。
 */
const MIN_SPACING_RATIO = 3;

/**
 * 線を吸着先の座標列に変換する。近すぎる線は強い方だけを残す。
 *
 * **間引きが要るのは、許容距離が画面 px 基準だから。** 許容距離は表示スケールに
 * 反比例して画像 px では広がるので (Retina のスクショを縮小表示すると 6 画面 px が
 * 25 画像 px 以上になる)、検出時の画像 px 基準の本数上限では「画面上で見て候補が
 * 詰まりすぎている」状態を防げない。だから間引きは検出結果のキャッシュとは分けて、
 * **表示スケールが変わるたびに** やり直す。
 *
 * **間引くのは線どうしで、1 本の線が持つ両端は分けない。** 両端は 1〜3px しか離れて
 * いないので必ず「詰まりすぎ」と判定されるが、そこは選ばせたい 2 択そのもの。
 *
 * 画像の端 (0 と imageSize) は常に入れる。検出された線ではないので間引きにも
 * 参加させない (端の近くにある本物の線を潰さないため)。
 *
 * tolerance に 0 を渡すと間引きなしになる (検出結果をそのまま見たいテスト用)。
 */
export function snapPositions(
  runs: EdgeSnapRun[] | undefined,
  imageSize: number,
  tolerance: number
): number[] {
  const positions = [0, imageSize];
  if (runs && runs.length > 0) {
    const minSpacing = tolerance * MIN_SPACING_RATIO;
    const kept: EdgeSnapRun[] = [];
    // 画像の外を指す線は捨てる。検出時点では出ないが、呼び出し側が別サイズの画像から
    // 取った線を渡してくると (画像を差し替えた直後の 1 瞬など) 枠が画像外へ飛ぶ
    const inside = runs.filter((run) => run.start >= 0 && run.end <= imageSize);
    // 強い順に採ることで、間引きで生き残るのが「その周辺で一番はっきりした線」になる
    for (const run of [...inside].sort((a, b) => b.strength - a.strength)) {
      if (kept.some((k) => runDistance(k, run) < minSpacing)) continue;
      kept.push(run);
    }
    for (const run of kept) {
      positions.push(run.start);
      if (run.end !== run.start) positions.push(run.end);
    }
  }
  positions.sort((a, b) => a - b);
  return positions.filter((v, i) => i === 0 || v !== positions[i - 1]);
}

/**
 * value に最も近い吸着先を返す。許容距離内に無ければ null。
 * 距離が同じなら小さい方の座標を採る (nearest が一意に決まらないと、
 * 同じ位置でドラッグを止めているのに吸着先がちらつく)。
 */
export function snapToLine(
  value: number,
  positions: number[] | undefined,
  tolerance: number
): number | null {
  if (!positions || positions.length === 0) return null;
  let best: number | null = null;
  let bestDistance = Infinity;
  for (const position of positions) {
    const d = Math.abs(position - value);
    if (d <= tolerance && d < bestDistance) {
      bestDistance = d;
      best = position;
    }
  }
  return best;
}
