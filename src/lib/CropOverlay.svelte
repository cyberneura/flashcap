<script lang="ts">
  import type { CropRect } from "./types";

  interface Props {
    rect: CropRect;
    imageWidth: number;
    imageHeight: number;
    scale: number;
    onRectChange: (rect: CropRect) => void;
  }

  let { rect, imageWidth, imageHeight, scale, onRectChange }: Props = $props();

  // 画面上の見た目を一定に保つため、スケール済みの値を使う箇所では / scale する
  const HANDLE_SIZE = 8;
  const MIN_SIZE = 8;

  let svgEl = $state<SVGSVGElement | null>(null);
  let dragging = $state<"draw" | "move" | "resize" | null>(null);
  let dragStart = $state<{ x: number; y: number } | null>(null);
  let dragOrigRect = $state<CropRect | null>(null);
  let resizeHandle = $state<string | null>(null);
  let hoverCursor = $state("crosshair");

  const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);

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

  // ハンドル方向のリサイズ。最小サイズを保ちつつ画像の外へは出さない
  function computeResize(orig: CropRect, handle: string, dx: number, dy: number): CropRect {
    let { x, y, width, height } = orig;

    if (handle.includes("w")) {
      const left = clamp(orig.x + dx, 0, orig.x + orig.width - MIN_SIZE);
      x = left;
      width = orig.x + orig.width - left;
    }
    if (handle.includes("e")) {
      const right = clamp(orig.x + orig.width + dx, orig.x + MIN_SIZE, imageWidth);
      width = right - orig.x;
    }
    if (handle.includes("n")) {
      const top = clamp(orig.y + dy, 0, orig.y + orig.height - MIN_SIZE);
      y = top;
      height = orig.y + orig.height - top;
    }
    if (handle.includes("s")) {
      const bottom = clamp(orig.y + orig.height + dy, orig.y + MIN_SIZE, imageHeight);
      height = bottom - orig.y;
    }
    return { x, y, width, height };
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

    // 枠の外からのドラッグは範囲の引き直し
    dragging = "draw";
    dragStart = { x: clamp(pt.x, 0, imageWidth), y: clamp(pt.y, 0, imageHeight) };
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
      onRectChange(computeResize(dragOrigRect, resizeHandle, dx, dy));
    } else if (dragging === "move") {
      onRectChange({
        ...dragOrigRect,
        x: clamp(dragOrigRect.x + dx, 0, imageWidth - dragOrigRect.width),
        y: clamp(dragOrigRect.y + dy, 0, imageHeight - dragOrigRect.height),
      });
    } else if (dragging === "draw") {
      const cx = clamp(pt.x, 0, imageWidth);
      const cy = clamp(pt.y, 0, imageHeight);
      onRectChange({
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
    if (dragging === "draw" && dragOrigRect && (rect.width < MIN_SIZE || rect.height < MIN_SIZE)) {
      onRectChange(dragOrigRect);
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
