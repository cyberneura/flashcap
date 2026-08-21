<script lang="ts">
  import type { CropRect } from "./types";
  import { snapPositions, snapToLine, type EdgeSnapLines } from "./edgeSnap";
  import {
    MIN_CROP_SIZE,
    aspectRectFromCorner,
    aspectRectFromEdge,
  } from "./cropAspect";

  interface Props {
    rect: CropRect;
    imageWidth: number;
    imageHeight: number;
    scale: number;
    snapEnabled: boolean;
    snapLines: EdgeSnapLines | null;
    /** 固定する縦横比 (width / height)。null なら自由 */
    aspect: number | null;
    onRectChange: (rect: CropRect) => void;
  }

  let {
    rect,
    imageWidth,
    imageHeight,
    scale,
    snapEnabled,
    snapLines,
    aspect,
    onRectChange,
  }: Props = $props();

  // 画面上の見た目を一定に保つため、スケール済みの値を使う箇所では / scale する
  const HANDLE_SIZE = 8;
  const MIN_SIZE = MIN_CROP_SIZE;
  /** 境界線に吸着する距離 (画面上の px) */
  const SNAP_DISTANCE = 6;
  /**
   * 「クリックしただけ」と「引いた」を分ける移動量 (画面上の px)。
   *
   * **MIN_SIZE を流用しない。** 縦横比の固定中は 3px 引いただけでも
   * aspectRectFromCorner が最小サイズまで広げた正当な枠を作って画面に出すので、
   * MIN_SIZE を閾値にすると「見えている枠が離した瞬間に消える」ことになる。
   * ここで見たいのは寸法ではなく、押下時の微小なブレとドラッグの区別。
   */
  const CLICK_SLOP = 2;

  let bounds = $derived({ width: imageWidth, height: imageHeight });

  let svgEl = $state<SVGSVGElement | null>(null);
  let dragging = $state<"draw" | "move" | "resize" | null>(null);
  let dragStart = $state<{ x: number; y: number } | null>(null);
  let dragOrigRect = $state<CropRect | null>(null);
  let resizeHandle = $state<string | null>(null);
  let hoverCursor = $state("crosshair");
  /** 引き直しのドラッグでポインタが実際に動いたか (縦横比固定時の「クリックだけ」判定用) */
  let drawMoved = false;
  /** 引き直しを始めた生のポインタ位置 (吸着前)。drawMoved の判定に使う */
  let drawStartRaw: { x: number; y: number } | null = null;

  // lo > hi の時は lo に倒す。素の Math.min(Math.max(v, lo), hi) だと hi を返してしまい、
  // 下限より小さい値が通る (MIN_SIZE より小さい画像で下の minWidth/minHeight が効かなくなる)
  const clamp = (v: number, lo: number, hi: number) =>
    Math.min(Math.max(v, lo), Math.max(lo, hi));

  // 画像そのものが MIN_SIZE より小さいことがある (小さなアイコン画像を開いた時)。
  // その場合まで MIN_SIZE を強制すると、枠が画像からはみ出す
  let minWidth = $derived(Math.min(MIN_SIZE, imageWidth));
  let minHeight = $derived(Math.min(MIN_SIZE, imageHeight));

  // 吸着の許容距離は画面上で一定にする (縮小表示中に画像座標で 6px だと届かない)
  let snapTolerance = $derived(SNAP_DISTANCE / scale);

  // 検出結果は画像 1 枚につき 1 回しか計算しないが、間引きは **表示スケールが変わるたびに**
  // やり直す必要があるのでここで derive する。ウインドウのリサイズで scale が変われば
  // 画面上の候補の詰まり具合も変わるため
  let snapXs = $derived(
    snapEnabled ? snapPositions(snapLines?.xs, imageWidth, snapTolerance) : undefined
  );
  let snapYs = $derived(
    snapEnabled ? snapPositions(snapLines?.ys, imageHeight, snapTolerance) : undefined
  );

  function snapped(value: number, positions: number[] | undefined): number {
    return snapToLine(value, positions, snapTolerance) ?? value;
  }

  /**
   * 範囲を引いている最中の終点を吸着させる。
   *
   * **吸着で最小サイズを割り込むなら吸着させない。** handleWindowMouseUp は
   * MIN_SIZE 未満の引き直しをドラッグ前の枠へ戻すので、吸着が原因で幅が縮むと
   * 「引けていたはずの選択が、離した瞬間に丸ごと消える」ことになる。
   */
  function snapEndpoint(raw: number, anchor: number, positions: number[] | undefined): number {
    const value = snapped(raw, positions);
    return Math.abs(value - anchor) < MIN_SIZE ? raw : value;
  }

  /**
   * ドラッグ中の枠を親へ渡す。**退化した枠は渡さない。**
   *
   * 縦横比の計算は、伸ばす余地が完全に無い向きでは 0 を返すしかない。0×0 を渡すと
   * 比率が NaN になり、しかも mouseup の復元は引き直し経路にしかないのでリサイズ中は
   * 戻せない。渡さなければ枠は直前の状態のまま残る。
   */
  function emitRect(next: CropRect) {
    if (!(next.width > 0) || !(next.height > 0)) return;
    onRectChange(next);
  }

  function toImageCoords(e: MouseEvent): { x: number; y: number } | null {
    if (!svgEl) return null;
    const r = svgEl.getBoundingClientRect();
    return { x: (e.clientX - r.left) / scale, y: (e.clientY - r.top) / scale };
  }

  function handlePositions(r: CropRect): [string, number, number][] {
    return [
      ["nw", r.x, r.y],
      ["n", r.x + r.width / 2, r.y],
      ["ne", r.x + r.width, r.y],
      ["w", r.x, r.y + r.height / 2],
      ["e", r.x + r.width, r.y + r.height / 2],
      ["sw", r.x, r.y + r.height],
      ["s", r.x + r.width / 2, r.y + r.height],
      ["se", r.x + r.width, r.y + r.height],
    ];
  }

  function hitTestHandle(r: CropRect, px: number, py: number): string | null {
    const hitRadius = HANDLE_SIZE / scale;
    for (const [name, hx, hy] of handlePositions(r)) {
      if (Math.abs(px - hx) <= hitRadius && Math.abs(py - hy) <= hitRadius) return name;
    }
    return null;
  }

  function cursorForHandle(handle: string): string {
    const map: Record<string, string> = {
      nw: "nwse-resize", se: "nwse-resize",
      ne: "nesw-resize", sw: "nesw-resize",
      n: "ns-resize", s: "ns-resize",
      e: "ew-resize", w: "ew-resize",
    };
    return map[handle] ?? "default";
  }

  /**
   * 縦横比を固定している時のリサイズ。
   *
   * **吸着は縦横比に負ける。** 動かす軸だけを吸着させ、もう一方は縦横比から導く。
   * 両軸を吸着させると比率が崩れるので、固定した比率の方を硬い制約として扱う。
   */
  function computeAspectResize(orig: CropRect, handle: string, dx: number, dy: number, ratio: number): CropRect {
    const pointer = {
      x: snapped(clamp(orig.x + (handle.includes("w") ? dx : orig.width + dx), 0, imageWidth), snapXs),
      y: snapped(clamp(orig.y + (handle.includes("n") ? dy : orig.height + dy), 0, imageHeight), snapYs),
    };

    // 角ハンドルは対角の角を固定して引く。辺ハンドルは片軸だけがポインタで決まる
    if (handle.length === 2) {
      const anchor = {
        x: handle.includes("w") ? orig.x + orig.width : orig.x,
        y: handle.includes("n") ? orig.y + orig.height : orig.y,
      };
      return aspectRectFromCorner(anchor, pointer, ratio, bounds);
    }
    return aspectRectFromEdge(orig, handle, pointer, ratio, bounds);
  }

  // ハンドル方向のリサイズ。動かす辺だけを境界線に吸着させ、
  // 最小サイズを保ちつつ画像の外へは出さない
  function computeResize(orig: CropRect, handle: string, dx: number, dy: number): CropRect {
    if (aspect !== null) return computeAspectResize(orig, handle, dx, dy, aspect);

    let { x, y, width, height } = orig;

    if (handle.includes("w")) {
      const left = clamp(snapped(orig.x + dx, snapXs), 0, orig.x + orig.width - minWidth);
      x = left;
      width = orig.x + orig.width - left;
    }
    if (handle.includes("e")) {
      const right = clamp(
        snapped(orig.x + orig.width + dx, snapXs),
        orig.x + minWidth,
        imageWidth
      );
      width = right - orig.x;
    }
    if (handle.includes("n")) {
      const top = clamp(snapped(orig.y + dy, snapYs), 0, orig.y + orig.height - minHeight);
      y = top;
      height = orig.y + orig.height - top;
    }
    if (handle.includes("s")) {
      const bottom = clamp(
        snapped(orig.y + orig.height + dy, snapYs),
        orig.y + minHeight,
        imageHeight
      );
      height = bottom - orig.y;
    }
    return { x, y, width, height };
  }

  // 移動は両端が同時に動くので、前後の辺のうち吸着先が近い方に合わせて全体をずらす
  // (両方に合わせると枠の大きさが変わってしまう)
  function snapSpan(start: number, length: number, positions: number[] | undefined): number {
    const head = snapToLine(start, positions, snapTolerance);
    const tail = snapToLine(start + length, positions, snapTolerance);
    const headDistance = head === null ? Infinity : Math.abs(head - start);
    const tailDistance = tail === null ? Infinity : Math.abs(tail - start - length);
    if (headDistance <= tailDistance) return head ?? start;
    return tail === null ? start : tail - length;
  }

  function handleMouseDown(e: MouseEvent) {
    const pt = toImageCoords(e);
    if (!pt) return;
    e.preventDefault();

    const handle = hitTestHandle(rect, pt.x, pt.y);
    if (handle) {
      dragging = "resize";
      resizeHandle = handle;
      dragStart = pt;
      dragOrigRect = { ...rect };
      return;
    }

    if (canMove && isInsideRect(pt)) {
      dragging = "move";
      dragStart = pt;
      dragOrigRect = { ...rect };
      return;
    }

    // 枠の外からのドラッグは範囲の引き直し。始点も境界線に吸着させる
    dragging = "draw";
    drawMoved = false;
    drawStartRaw = { x: clamp(pt.x, 0, imageWidth), y: clamp(pt.y, 0, imageHeight) };
    dragStart = {
      x: snapped(clamp(pt.x, 0, imageWidth), snapXs),
      y: snapped(clamp(pt.y, 0, imageHeight), snapYs),
    };
    dragOrigRect = { ...rect };
  }

  function updateHoverCursor(pt: { x: number; y: number }) {
    const handle = hitTestHandle(rect, pt.x, pt.y);
    if (handle) {
      hoverCursor = cursorForHandle(handle);
      return;
    }
    hoverCursor = canMove && isInsideRect(pt) ? "move" : "crosshair";
  }

  // ドラッグ中の move/up は window で拾い、画像の外でマウスを離しても
  // ドラッグ状態が残らないようにする (画像の端まで一気に広げる操作も自然になる)
  function handleWindowMouseMove(e: MouseEvent) {
    const pt = toImageCoords(e);
    if (!pt) return;

    if (!dragging || !dragStart || !dragOrigRect) {
      updateHoverCursor(pt);
      return;
    }

    const dx = pt.x - dragStart.x;
    const dy = pt.y - dragStart.y;

    if (dragging === "resize" && resizeHandle) {
      emitRect(computeResize(dragOrigRect, resizeHandle, dx, dy));
    } else if (dragging === "move") {
      const x = clamp(dragOrigRect.x + dx, 0, imageWidth - dragOrigRect.width);
      const y = clamp(dragOrigRect.y + dy, 0, imageHeight - dragOrigRect.height);
      emitRect({
        ...dragOrigRect,
        x: clamp(snapSpan(x, dragOrigRect.width, snapXs), 0, imageWidth - dragOrigRect.width),
        y: clamp(snapSpan(y, dragOrigRect.height, snapYs), 0, imageHeight - dragOrigRect.height),
      });
    } else if (dragging === "draw") {
      const cx = snapEndpoint(clamp(pt.x, 0, imageWidth), dragStart.x, snapXs);
      const cy = snapEndpoint(clamp(pt.y, 0, imageHeight), dragStart.y, snapYs);
      // 吸着後の座標ではなく生のポインタで見る。吸着が終点を始点側へ引き戻すと、
      // 実際には引いているのに「動いていない」と判定されてしまう
      if (drawStartRaw) {
        const slop = CLICK_SLOP / scale;
        if (Math.abs(pt.x - drawStartRaw.x) >= slop || Math.abs(pt.y - drawStartRaw.y) >= slop) {
          drawMoved = true;
        }
      }
      if (aspect !== null) {
        // 引き直しは開始点を固定した角として扱えるので、角リサイズと同じ計算になる
        emitRect(aspectRectFromCorner(dragStart, { x: cx, y: cy }, aspect, bounds));
        return;
      }
      emitRect({
        x: Math.min(dragStart.x, cx),
        y: Math.min(dragStart.y, cy),
        width: Math.abs(cx - dragStart.x),
        height: Math.abs(cy - dragStart.y),
      });
    }
  }

  function handleWindowMouseUp() {
    // 引き直しが小さすぎる場合はドラッグ前の範囲に戻す
    // (クリックしただけで範囲が消えるのを防ぐ)
    //
    // **縦横比の固定中は結果の寸法で判定できない。** 比率を保つために枠が最小サイズまで
    // 自動で広がるので、クリックしただけでも MIN_SIZE の枠ができてしまう。
    // ポインタが動いたかどうかで見る
    if (dragging === "draw" && dragOrigRect) {
      const tooSmall =
        aspect !== null ? !drawMoved : rect.width < MIN_SIZE || rect.height < MIN_SIZE;
      if (tooSmall) onRectChange(dragOrigRect);
    }
    dragging = null;
    dragStart = null;
    dragOrigRect = null;
    resizeHandle = null;
  }

  let sizeLabel = $derived(`${Math.round(rect.width)} × ${Math.round(rect.height)}`);
  let visualHandleSize = $derived(HANDLE_SIZE / scale);

  // 枠が画像いっぱいの間は動かす余地が無い。この時だけ内側のドラッグを
  // 「範囲の引き直し」に回す (そうしないと枠の外が存在せず、引き直す手段が無くなる)
  let canMove = $derived(rect.width < imageWidth || rect.height < imageHeight);

  // ドラッグ中、境界線の上に乗っている辺にだけガイドを出す。
  // 「吸着したか」を別途覚えるのではなく候補と一致するかで判定するので、
  // クランプで吸着先から押し戻された辺には出ない
  let activeGuides = $derived.by(() => {
    const guides = { xs: [] as number[], ys: [] as number[] };
    if (!dragging) return guides;
    const xs = new Set(snapXs ?? []);
    const ys = new Set(snapYs ?? []);
    for (const v of [rect.x, rect.x + rect.width]) if (xs.has(v)) guides.xs.push(v);
    for (const v of [rect.y, rect.y + rect.height]) if (ys.has(v)) guides.ys.push(v);
    return guides;
  });

  function isInsideRect(pt: { x: number; y: number }): boolean {
    return (
      pt.x >= rect.x && pt.x <= rect.x + rect.width &&
      pt.y >= rect.y && pt.y <= rect.y + rect.height
    );
  }
</script>

<svelte:window onmousemove={handleWindowMouseMove} onmouseup={handleWindowMouseUp} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg
  bind:this={svgEl}
  class="crop-overlay"
  onmousedown={handleMouseDown}
  style:cursor={dragging === "move" ? "move" : dragging === "resize" && resizeHandle ? cursorForHandle(resizeHandle) : hoverCursor}
>
  <!-- 切り落とされる範囲を暗くする -->
  <g fill="rgba(0,0,0,0.55)">
    <rect x="0" y="0" width={imageWidth} height={rect.y} />
    <rect x="0" y={rect.y + rect.height} width={imageWidth} height={Math.max(0, imageHeight - rect.y - rect.height)} />
    <rect x="0" y={rect.y} width={rect.x} height={rect.height} />
    <rect x={rect.x + rect.width} y={rect.y} width={Math.max(0, imageWidth - rect.x - rect.width)} height={rect.height} />
  </g>

  <!-- 吸着中のガイド線 -->
  <g stroke="#22d3ee" stroke-width={1 / scale} pointer-events="none">
    {#each activeGuides.xs as gx}
      <line x1={gx} y1="0" x2={gx} y2={imageHeight} />
    {/each}
    {#each activeGuides.ys as gy}
      <line x1="0" y1={gy} x2={imageWidth} y2={gy} />
    {/each}
  </g>

  <rect
    x={rect.x} y={rect.y} width={rect.width} height={rect.height}
    fill="none" stroke="#ffffff" stroke-width={1.5 / scale}
  />

  {#each handlePositions(rect) as [name, hx, hy]}
    <rect
      x={hx - visualHandleSize / 2}
      y={hy - visualHandleSize / 2}
      width={visualHandleSize}
      height={visualHandleSize}
      fill="white"
      stroke="#0066cc"
      stroke-width={1.5 / scale}
      style:cursor={cursorForHandle(name)}
    />
  {/each}

  <!-- 縁取りで背景を選ばず読めるようにする (背景矩形だと文字幅の実測が必要になる) -->
  <text
    x={rect.x + 8 / scale}
    y={rect.y + 8 / scale}
    font-size={13 / scale}
    fill="#ffffff"
    stroke="rgba(0,0,0,0.85)"
    stroke-width={3 / scale}
    style="paint-order: stroke; dominant-baseline: hanging;"
    class="select-none"
  >{sizeLabel}</text>
</svg>

<style>
  @reference "../app.css";

  .crop-overlay {
    @apply absolute top-0 left-0 w-full h-full z-60;
  }
</style>
