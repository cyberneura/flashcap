<script lang="ts">
  import type { Shape, ShapeSettings } from "./types";
  import { highlightPaint, hitsHighlight, type LumaMap } from "./highlight";

  interface Props {
    shapes: Shape[];
    settings: ShapeSettings;
    toolActive: boolean;
    interactive: boolean;
    scale: number;
    /** 線の太さ・文字の大きさに掛ける倍率。Retina の撮影なら 2 (AGENTS.md の「Retina の撮影」) */
    annotationScale?: number;
    /** 画像の明るさの地図。蛍光ペンを乗算で重ねるか半透明で重ねるかの判定に使う (highlight.ts) */
    lumaMap?: LumaMap | null;
    onShapesChange: (shapes: Shape[]) => void;
    onBeforeMutate?: () => void;
  }

  let { shapes, settings, toolActive, interactive, scale, annotationScale = 1, lumaMap = null, onShapesChange, onBeforeMutate }: Props = $props();

  let selectedId = $state<string | null>(null);
  let dragging = $state<"draw" | "move" | "resize" | null>(null);
  let drawingShape = $state<Shape | null>(null);
  let dragStart = $state<{ x: number; y: number } | null>(null);
  let hoverCursor = $state<string>("default");
  let shiftHeld = $state(false);

  let dragOrigRect = $state<{ x: number; y: number; width: number; height: number } | null>(null);
  let resizeHandle = $state<string | null>(null);

  const HANDLE_SIZE = 6;

  function getSvgCoords(e: MouseEvent): { x: number; y: number } | null {
    const svg = (e.currentTarget as SVGSVGElement) ?? (e.target as Element).closest("svg");
    if (!svg) return null;
    const rect = svg.getBoundingClientRect();
    return { x: (e.clientX - rect.left) / scale, y: (e.clientY - rect.top) / scale };
  }

  // Shift 押下で正方形/正円に制約
  function constrainSquare(startX: number, startY: number, endX: number, endY: number) {
    const dx = endX - startX;
    const dy = endY - startY;
    const size = Math.max(Math.abs(dx), Math.abs(dy));
    return {
      x: Math.min(startX, startX + Math.sign(dx) * size),
      y: Math.min(startY, startY + Math.sign(dy) * size),
      width: size,
      height: size,
    };
  }

  function handleMouseDown(e: MouseEvent) {
    const pt = getSvgCoords(e);
    if (!pt) return;

    if (selectedId) {
      const sel = shapes.find((s) => s.id === selectedId);
      if (sel) {
        const handle = hitTestHandle(sel, pt.x, pt.y);
        if (handle) {
          onBeforeMutate?.();
          resizeHandle = handle;
          dragging = "resize";
          dragStart = pt;
          dragOrigRect = { x: sel.x, y: sel.y, width: sel.width, height: sel.height };
          return;
        }
        if (hitTestShape(sel, pt.x, pt.y)) {
          onBeforeMutate?.();
          dragging = "move";
          dragStart = pt;
          dragOrigRect = { x: sel.x, y: sel.y, width: sel.width, height: sel.height };
          return;
        }
      }
    }

    const hit = [...shapes].reverse().find((s) => hitTestShape(s, pt.x, pt.y));
    if (hit) {
      selectedId = hit.id;
      return;
    }

    if (!toolActive) {
      selectedId = null;
      return;
    }

    onBeforeMutate?.();
    dragStart = pt;
    // 蛍光ペンは塗りだけなので、枠線の設定 (白縁・影) は持たせない
    const isHighlight = settings.type === "highlight";
    drawingShape = {
      id: crypto.randomUUID(),
      type: settings.type,
      x: pt.x,
      y: pt.y,
      width: 0,
      height: 0,
      color: isHighlight ? settings.highlightColor : settings.color,
      thickness: settings.thickness,
      whiteStroke: isHighlight ? false : settings.whiteStroke,
      dropShadow: isHighlight ? false : settings.dropShadow,
    };
    dragging = "draw";
    selectedId = null;
  }

  // 線(stroke)の近くをクリックで選択
  function hitTestShape(shape: Shape, px: number, py: number): boolean {
    // 蛍光ペンは塗りなので内側のどこでも当たる
    if (shape.type === "highlight") return hitsHighlight(shape, px, py, 6 / scale);
    const threshold = shape.thickness * annotationScale + 6 / scale;
    const { x, y, width: w, height: h } = shape;

    if (shape.type === "rect") {
      // 4辺のいずれかに近いか
      const inXRange = px >= x - threshold && px <= x + w + threshold;
      const inYRange = py >= y - threshold && py <= y + h + threshold;
      const nearLeft = inYRange && Math.abs(px - x) <= threshold;
      const nearRight = inYRange && Math.abs(px - (x + w)) <= threshold;
      const nearTop = inXRange && Math.abs(py - y) <= threshold;
      const nearBottom = inXRange && Math.abs(py - (y + h)) <= threshold;
      return nearLeft || nearRight || nearTop || nearBottom;
    } else {
      // 楕円: stroke 近傍判定
      const cx = x + w / 2;
      const cy = y + h / 2;
      const rx = w / 2;
      const ry = h / 2;
      if (rx <= 0 || ry <= 0) return false;
      const outerRx = rx + threshold;
      const outerRy = ry + threshold;
      const innerRx = Math.max(0, rx - threshold);
      const innerRy = Math.max(0, ry - threshold);
      const outerNorm = ((px - cx) / outerRx) ** 2 + ((py - cy) / outerRy) ** 2;
      const innerNorm = innerRx > 0 && innerRy > 0
        ? ((px - cx) / innerRx) ** 2 + ((py - cy) / innerRy) ** 2
        : Infinity;
      return outerNorm <= 1 && innerNorm >= 1;
    }
  }

  function hitTestHandle(shape: Shape, px: number, py: number): string | null {
    const { x, y, width: w, height: h } = shape;
    const hitRadius = HANDLE_SIZE / scale;
    const handles: [string, number, number][] = [
      ["nw", x, y], ["n", x + w / 2, y], ["ne", x + w, y],
      ["w", x, y + h / 2], ["e", x + w, y + h / 2],
      ["sw", x, y + h], ["s", x + w / 2, y + h], ["se", x + w, y + h],
    ];
    for (const [name, hx, hy] of handles) {
      if (Math.abs(px - hx) <= hitRadius && Math.abs(py - hy) <= hitRadius) return name;
    }
    return null;
  }

  function computeResize(
    orig: { x: number; y: number; width: number; height: number },
    handle: string,
    dx: number,
    dy: number
  ): { x: number; y: number; width: number; height: number } {
    let { x, y, width, height } = orig;
    const minSize = 5;

    if (handle.includes("w")) {
      const newX = Math.min(x + dx, x + width - minSize);
      width = width - (newX - x);
      x = newX;
    }
    if (handle.includes("e")) {
      width = Math.max(minSize, width + dx);
    }
    if (handle.includes("n")) {
      const newY = Math.min(y + dy, y + height - minSize);
      height = height - (newY - y);
      y = newY;
    }
    if (handle.includes("s")) {
      height = Math.max(minSize, height + dy);
    }
    return { x, y, width, height };
  }

  function handleCursorForHandle(handle: string): string {
    const map: Record<string, string> = {
      nw: "nwse-resize", se: "nwse-resize",
      ne: "nesw-resize", sw: "nesw-resize",
      n: "ns-resize", s: "ns-resize",
      e: "ew-resize", w: "ew-resize",
    };
    return map[handle] ?? "default";
  }

  function updateHoverCursor(pt: { x: number; y: number }) {
    if (dragging) return;

    if (selectedId) {
      const sel = shapes.find((s) => s.id === selectedId);
      if (sel) {
        const handle = hitTestHandle(sel, pt.x, pt.y);
        if (handle) {
          hoverCursor = handleCursorForHandle(handle);
          return;
        }
        if (hitTestShape(sel, pt.x, pt.y)) {
          hoverCursor = "move";
          return;
        }
      }
    }

    const hit = [...shapes].reverse().find((s) => hitTestShape(s, pt.x, pt.y));
    if (hit) {
      hoverCursor = "pointer";
      return;
    }

    hoverCursor = toolActive ? "crosshair" : "default";
  }

  function handleMouseMove(e: MouseEvent) {
    shiftHeld = e.shiftKey;
    const pt = getSvgCoords(e);
    if (!pt) return;

    if (!dragging || !dragStart) {
      updateHoverCursor(pt);
      return;
    }

    const dx = pt.x - dragStart.x;
    const dy = pt.y - dragStart.y;

    if (dragging === "draw" && drawingShape && dragStart) {
      if (shiftHeld) {
        const constrained = constrainSquare(dragStart.x, dragStart.y, pt.x, pt.y);
        drawingShape = { ...drawingShape, ...constrained };
      } else {
        const x = Math.min(dragStart.x, pt.x);
        const y = Math.min(dragStart.y, pt.y);
        const width = Math.abs(pt.x - dragStart.x);
        const height = Math.abs(pt.y - dragStart.y);
        drawingShape = { ...drawingShape, x, y, width, height };
      }
    } else if (dragging === "move" && selectedId && dragOrigRect) {
      onShapesChange(
        shapes.map((s) =>
          s.id === selectedId
            ? { ...s, x: dragOrigRect!.x + dx, y: dragOrigRect!.y + dy }
            : s
        )
      );
    } else if (dragging === "resize" && selectedId && dragOrigRect && resizeHandle) {
      const updated = computeResize(dragOrigRect, resizeHandle, dx, dy);
      onShapesChange(
        shapes.map((s) => (s.id === selectedId ? { ...s, ...updated } : s))
      );
    }
  }

  function handleMouseUp() {
    if (dragging === "draw" && drawingShape && drawingShape.width > 3 && drawingShape.height > 3) {
      onShapesChange([...shapes, drawingShape]);
      selectedId = drawingShape.id;
    }
    drawingShape = null;
    dragging = null;
    dragStart = null;
    dragOrigRect = null;
    resizeHandle = null;
  }

  function handleKeyDown(e: KeyboardEvent) {
    shiftHeld = e.shiftKey;
    if ((e.key === "Delete" || e.key === "Backspace") && selectedId) {
      e.preventDefault();
      e.stopPropagation();
      onBeforeMutate?.();
      onShapesChange(shapes.filter((s) => s.id !== selectedId));
      selectedId = null;
    }
  }

  function handleKeyUp(e: KeyboardEvent) {
    shiftHeld = e.shiftKey;
  }

  let allShapes = $derived(drawingShape ? [...shapes, drawingShape] : shapes);

  export function deselect() {
    selectedId = null;
  }
</script>

<svelte:window onkeydown={handleKeyDown} onkeyup={handleKeyUp} />

<!--
  置いた蛍光ペンの塗りは HighlightLayer (マスクより下) が描く。ここで描くのは引いている途中の
  1 つだけ (確定するまでの間だけ、マスクの上に見える)。svg の中には選択枠とハンドルだけを描く。
-->
{#if drawingShape && drawingShape.type === "highlight"}
  {@const paint = highlightPaint(lumaMap, drawingShape)}
  <div
    class="highlight-fill"
    style:left="{drawingShape.x}px"
    style:top="{drawingShape.y}px"
    style:width="{drawingShape.width}px"
    style:height="{drawingShape.height}px"
    style:background-color={drawingShape.color}
    style:mix-blend-mode={paint.blend}
    style:opacity={paint.opacity}
  ></div>
{/if}

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg
  class="shape-overlay"
  onmousedown={handleMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
  style:cursor={dragging === "move" ? "move" : dragging === "resize" && resizeHandle ? handleCursorForHandle(resizeHandle) : hoverCursor}
  style:pointer-events={interactive ? "auto" : "none"}
>
  <defs>
    <filter id="shape-shadow" x="-20%" y="-20%" width="140%" height="140%">
      <feDropShadow dx={2 * annotationScale} dy={2 * annotationScale} stdDeviation={2 * annotationScale} flood-opacity="0.5" />
    </filter>
  </defs>

  {#each allShapes as shape (shape.id)}
    {@const isSelected = shape.id === selectedId}

    {#if shape.type === "highlight"}
      <!-- 蛍光ペンの塗りは HighlightLayer。ここでは選択枠とハンドルだけを描く -->
    {:else if shape.dropShadow && shape.whiteStroke}
      <!-- 白枠のみに影を適用 -->
      {#if shape.type === "rect"}
        <rect
          x={shape.x} y={shape.y} width={shape.width} height={shape.height}
          fill="none" stroke="white" stroke-width={(shape.thickness + 4) * annotationScale}
          stroke-linejoin="round" filter="url(#shape-shadow)"
        />
        <rect
          x={shape.x} y={shape.y} width={shape.width} height={shape.height}
          fill="none" stroke={shape.color} stroke-width={shape.thickness * annotationScale}
          stroke-linejoin="round"
        />
      {:else}
        {@const cx = shape.x + shape.width / 2}
        {@const cy = shape.y + shape.height / 2}
        {@const rx = shape.width / 2}
        {@const ry = shape.height / 2}
        <ellipse
          cx={cx} cy={cy} rx={rx} ry={ry}
          fill="none" stroke="white" stroke-width={(shape.thickness + 4) * annotationScale}
          filter="url(#shape-shadow)"
        />
        <ellipse
          cx={cx} cy={cy} rx={rx} ry={ry}
          fill="none" stroke={shape.color} stroke-width={shape.thickness * annotationScale}
        />
      {/if}
    {:else}
      {@const filterAttr = shape.dropShadow ? "url(#shape-shadow)" : undefined}
      {#if shape.type === "rect"}
        {#if shape.whiteStroke}
          <rect
            x={shape.x} y={shape.y} width={shape.width} height={shape.height}
            fill="none" stroke="white" stroke-width={(shape.thickness + 4) * annotationScale}
            stroke-linejoin="round"
          />
        {/if}
        <rect
          x={shape.x} y={shape.y} width={shape.width} height={shape.height}
          fill="none" stroke={shape.color} stroke-width={shape.thickness * annotationScale}
          stroke-linejoin="round" filter={filterAttr}
        />
      {:else}
        {@const cx = shape.x + shape.width / 2}
        {@const cy = shape.y + shape.height / 2}
        {@const rx = shape.width / 2}
        {@const ry = shape.height / 2}
        {#if shape.whiteStroke}
          <ellipse
            cx={cx} cy={cy} rx={rx} ry={ry}
            fill="none" stroke="white" stroke-width={(shape.thickness + 4) * annotationScale}
          />
        {/if}
        <ellipse
          cx={cx} cy={cy} rx={rx} ry={ry}
          fill="none" stroke={shape.color} stroke-width={shape.thickness * annotationScale}
          filter={filterAttr}
        />
      {/if}
    {/if}

    {#if isSelected}
      <rect
        x={shape.x} y={shape.y} width={shape.width} height={shape.height}
        fill="none" stroke="#0066cc" stroke-width={2 / scale} stroke-dasharray="{4 / scale} {2 / scale}"
      />
      {@const visualHandleSize = HANDLE_SIZE / scale}
      {#each [
        ["nw", shape.x, shape.y],
        ["n", shape.x + shape.width / 2, shape.y],
        ["ne", shape.x + shape.width, shape.y],
        ["w", shape.x, shape.y + shape.height / 2],
        ["e", shape.x + shape.width, shape.y + shape.height / 2],
        ["sw", shape.x, shape.y + shape.height],
        ["s", shape.x + shape.width / 2, shape.y + shape.height],
        ["se", shape.x + shape.width, shape.y + shape.height],
      ] as [name, hx, hy]}
        <rect
          x={Number(hx) - visualHandleSize / 2}
          y={Number(hy) - visualHandleSize / 2}
          width={visualHandleSize}
          height={visualHandleSize}
          fill="white"
          stroke="#0066cc"
          stroke-width={1.5 / scale}
          style:cursor={handleCursorForHandle(String(name))}
          class="handle"
        />
      {/each}
    {/if}
  {/each}
</svg>

<style>
  @reference "../app.css";

  .shape-overlay {
    @apply absolute top-0 left-0 w-full h-full;
  }

  .handle {
    @apply pointer-events-auto;
  }

  .highlight-fill {
    @apply absolute pointer-events-none;
  }
</style>
