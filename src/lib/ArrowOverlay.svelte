<script lang="ts">
  import type { Arrow, ArrowSettings } from "./types";
  import { exceedsDragSlop } from "./textHit";
  import { offsetArrowCopy } from "./duplicate";

  interface Props {
    arrows: Arrow[];
    settings: ArrowSettings;
    toolActive: boolean;
    interactive: boolean;
    scale: number;
    /** 線の太さ・文字の大きさに掛ける倍率。Retina の撮影なら 2 (AGENTS.md の「Retina の撮影」) */
    annotationScale?: number;
    onArrowsChange: (arrows: Arrow[]) => void;
    onBeforeMutate?: () => void;
    /** 選択中の注釈の id。+page.svelte が bind して、ツールバーに選択中の注釈の設定を出す */
    selectedId?: string | null;
  }

  let { arrows, settings, toolActive, interactive, scale, annotationScale = 1, onArrowsChange, onBeforeMutate, selectedId = $bindable(null) }: Props = $props();

  // "duplicate" = Option / Alt で押して、動き始めるのを待っている。"move-all" = 複製した矢印を丸ごと動かす
  let dragging = $state<"draw" | "move-start" | "move-end" | "duplicate" | "move-all" | null>(null);
  /** Option / Alt+ドラッグで複製する元と、押した位置 */
  let duplicateSource: Arrow | null = null;
  let duplicateStart: { x: number; y: number } | null = null;
  /** 丸ごと動かしている複製の、動かし始めの位置 */
  let moveOrig: Arrow | null = null;
  let drawingArrow = $state<Arrow | null>(null);
  let hoverCursor = $state<string>("default");

  function headSize(thickness: number): number {
    return thickness * annotationScale * 4;
  }

  // Skitch style: arrowhead at drag start point (start side)
  function arrowHeadPoints(arrow: Arrow): string {
    const dx = arrow.startX - arrow.endX;
    const dy = arrow.startY - arrow.endY;
    const len = Math.sqrt(dx * dx + dy * dy);
    if (len === 0) return "";

    const hs = headSize(arrow.thickness);
    const ux = dx / len;
    const uy = dy / len;
    const tipX = arrow.startX;
    const tipY = arrow.startY;
    const bX = tipX - ux * hs;
    const bY = tipY - uy * hs;
    const perpX = -uy;
    const perpY = ux;
    const halfW = hs * 0.4;

    return `${tipX},${tipY} ${bX + perpX * halfW},${bY + perpY * halfW} ${bX - perpX * halfW},${bY - perpY * halfW}`;
  }

  // 棒の開始点を矢印頭の内側まで延長し、影描画時のシームを防ぐ
  function lineStart(arrow: Arrow): { x: number; y: number } {
    const dx = arrow.startX - arrow.endX;
    const dy = arrow.startY - arrow.endY;
    const len = Math.sqrt(dx * dx + dy * dy);
    if (len === 0) return { x: arrow.startX, y: arrow.startY };

    const hs = headSize(arrow.thickness);
    const ux = dx / len;
    const uy = dy / len;
    // 矢印頭の50%まで入り込ませる（ポリゴンが上に描画されるので見た目は変わらない）
    return {
      x: arrow.startX - ux * hs * 0.5,
      y: arrow.startY - uy * hs * 0.5,
    };
  }

  function distToArrow(arrow: Arrow, px: number, py: number): number {
    const { startX: x1, startY: y1, endX: x2, endY: y2 } = arrow;
    const dx = x2 - x1;
    const dy = y2 - y1;
    const lenSq = dx * dx + dy * dy;
    if (lenSq === 0) return Math.sqrt((px - x1) ** 2 + (py - y1) ** 2);
    const t = Math.max(0, Math.min(1, ((px - x1) * dx + (py - y1) * dy) / lenSq));
    const projX = x1 + t * dx;
    const projY = y1 + t * dy;
    return Math.sqrt((px - projX) ** 2 + (py - projY) ** 2);
  }

  function nearPoint(px: number, py: number, x: number, y: number): boolean {
    return Math.sqrt((px - x) ** 2 + (py - y) ** 2) < 10 / scale;
  }

  function getSvgCoords(e: MouseEvent): { x: number; y: number } | null {
    const svg = (e.currentTarget as SVGSVGElement) ?? (e.target as Element).closest("svg");
    if (!svg) return null;
    const rect = svg.getBoundingClientRect();
    return { x: (e.clientX - rect.left) / scale, y: (e.clientY - rect.top) / scale };
  }

  function handleMouseDown(e: MouseEvent) {
    const pt = getSvgCoords(e);
    if (!pt) return;

    // Option / Alt を押しながら注釈を押した: 動かし始めたら複製して、複製の方を動かす (矢印は端点ではなく丸ごと)
    // (CYBERNEURA-DEV-1321)。押しただけで離したら複製せずに選ぶだけ
    if (e.altKey) {
      const source = [...arrows].reverse().find((a) => distToArrow(a, pt.x, pt.y) < a.thickness * annotationScale + 6 / scale);
      if (source) {
        duplicateSource = source;
        duplicateStart = pt;
        dragging = "duplicate";
        return;
      }
    }

    // Drag endpoints of selected arrow
    if (selectedId) {
      const sel = arrows.find((a) => a.id === selectedId);
      if (sel) {
        if (nearPoint(pt.x, pt.y, sel.startX, sel.startY)) {
          onBeforeMutate?.();
          dragging = "move-start";
          return;
        }
        if (nearPoint(pt.x, pt.y, sel.endX, sel.endY)) {
          onBeforeMutate?.();
          dragging = "move-end";
          return;
        }
      }
    }

    // Click existing arrow to select
    const hit = [...arrows].reverse().find((a) => distToArrow(a, pt.x, pt.y) < a.thickness * annotationScale + 6 / scale);
    if (hit) {
      selectedId = hit.id;
      return;
    }

    if (!toolActive) {
      selectedId = null;
      return;
    }

    // Start drawing new arrow
    onBeforeMutate?.();
    const id = crypto.randomUUID();
    drawingArrow = {
      id,
      startX: pt.x,
      startY: pt.y,
      endX: pt.x,
      endY: pt.y,
      ...settings,
    };
    dragging = "draw";
    selectedId = null;
  }

  function updateHoverCursor(pt: { x: number; y: number }) {
    // ドラッグ中はカーソルを変えない
    if (dragging) return;

    // 選択中の矢印のエンドポイント上 → grab
    if (selectedId) {
      const sel = arrows.find((a) => a.id === selectedId);
      if (sel) {
        if (nearPoint(pt.x, pt.y, sel.startX, sel.startY) ||
            nearPoint(pt.x, pt.y, sel.endX, sel.endY)) {
          hoverCursor = "move";
          return;
        }
      }
    }

    // 既存の矢印の上 → pointer（再選択可能）
    const hit = [...arrows].reverse().find((a) => distToArrow(a, pt.x, pt.y) < a.thickness * annotationScale + 6 / scale);
    if (hit) {
      hoverCursor = "pointer";
      return;
    }

    hoverCursor = toolActive ? "crosshair" : "default";
  }

  function handleMouseMove(e: MouseEvent) {
    const pt = getSvgCoords(e);
    if (!pt) return;

    if (!dragging) {
      updateHoverCursor(pt);
      return;
    }

    if (dragging === "duplicate" && duplicateSource && duplicateStart) {
      const dx = pt.x - duplicateStart.x;
      const dy = pt.y - duplicateStart.y;
      if (!exceedsDragSlop(dx, dy, scale)) return;
      onBeforeMutate?.();
      const copy = offsetArrowCopy(duplicateSource, 0, 0, crypto.randomUUID());
      moveOrig = copy;
      onArrowsChange([...arrows, offsetArrowCopy(copy, dx, dy, copy.id)]);
      selectedId = copy.id;
      duplicateSource = null;
      dragging = "move-all";
    } else if (dragging === "move-all" && selectedId && moveOrig && duplicateStart) {
      const moved = offsetArrowCopy(moveOrig, pt.x - duplicateStart.x, pt.y - duplicateStart.y, moveOrig.id);
      onArrowsChange(arrows.map((a) => (a.id === selectedId ? moved : a)));
    } else if (dragging === "draw" && drawingArrow) {
      drawingArrow = { ...drawingArrow, endX: pt.x, endY: pt.y };
    } else if (dragging === "move-start" && selectedId) {
      onArrowsChange(
        arrows.map((a) => (a.id === selectedId ? { ...a, startX: pt.x, startY: pt.y } : a))
      );
    } else if (dragging === "move-end" && selectedId) {
      onArrowsChange(
        arrows.map((a) => (a.id === selectedId ? { ...a, endX: pt.x, endY: pt.y } : a))
      );
    }
  }

  function handleMouseUp() {
    // Option / Alt を押して離しただけ: 複製せずに選ぶ
    if (dragging === "duplicate" && duplicateSource) selectedId = duplicateSource.id;
    duplicateSource = null;
    duplicateStart = null;
    moveOrig = null;
    if (dragging === "draw" && drawingArrow) {
      const dx = drawingArrow.endX - drawingArrow.startX;
      const dy = drawingArrow.endY - drawingArrow.startY;
      // Minimum drag distance check
      if (Math.sqrt(dx * dx + dy * dy) > 5) {
        onArrowsChange([...arrows, drawingArrow]);
        selectedId = drawingArrow.id;
      }
    }
    drawingArrow = null;
    dragging = null;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if ((e.key === "Delete" || e.key === "Backspace") && selectedId) {
      e.preventDefault();
      e.stopPropagation();
      onBeforeMutate?.();
      onArrowsChange(arrows.filter((a) => a.id !== selectedId));
      selectedId = null;
    }
  }

  let allArrows = $derived(drawingArrow ? [...arrows, drawingArrow] : arrows);

  export function deselect() {
    selectedId = null;
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg
  class="arrow-overlay"
  onmousedown={handleMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
  style:cursor={dragging === "move-start" || dragging === "move-end" ? "move" : hoverCursor}
  style:pointer-events={interactive ? "auto" : "none"}
>
  <defs>
    <filter id="arrow-shadow" x="-20%" y="-20%" width="140%" height="140%">
      <feDropShadow dx={2 * annotationScale} dy={2 * annotationScale} stdDeviation={2 * annotationScale} flood-opacity="0.5" />
    </filter>
  </defs>

  {#each allArrows as arrow (arrow.id)}
    {@const ls = lineStart(arrow)}
    {@const hp = arrowHeadPoints(arrow)}
    {@const isSelected = arrow.id === selectedId}

    {#if arrow.dropShadow && arrow.whiteStroke}
      <!-- 白枠のみに影を適用 -->
      <g filter="url(#arrow-shadow)">
        <line
          x1={ls.x} y1={ls.y} x2={arrow.endX} y2={arrow.endY}
          stroke="white"
          stroke-width={(arrow.thickness + 4) * annotationScale}
          stroke-linecap="round"
        />
        <polygon points={hp} fill="white" stroke="white" stroke-width={4 * annotationScale} stroke-linejoin="round" />
      </g>
      <!-- 本体は影なし -->
      <line
        x1={ls.x} y1={ls.y} x2={arrow.endX} y2={arrow.endY}
        stroke={arrow.color}
        stroke-width={arrow.thickness * annotationScale}
        stroke-linecap="round"
      />
      <polygon points={hp} fill={arrow.color} />
    {:else if arrow.dropShadow}
      <!-- 影あり・白枠なし -->
      <g filter="url(#arrow-shadow)">
        <line
          x1={ls.x} y1={ls.y} x2={arrow.endX} y2={arrow.endY}
          stroke={arrow.color}
          stroke-width={arrow.thickness * annotationScale}
          stroke-linecap="round"
        />
        <polygon points={hp} fill={arrow.color} />
      </g>
    {:else}
      <!-- 影なし -->
      {#if arrow.whiteStroke}
        <line
          x1={ls.x} y1={ls.y} x2={arrow.endX} y2={arrow.endY}
          stroke="white"
          stroke-width={(arrow.thickness + 4) * annotationScale}
          stroke-linecap="round"
        />
        <polygon points={hp} fill="white" stroke="white" stroke-width={4 * annotationScale} stroke-linejoin="round" />
      {/if}
      <line
        x1={ls.x} y1={ls.y} x2={arrow.endX} y2={arrow.endY}
        stroke={arrow.color}
        stroke-width={arrow.thickness * annotationScale}
        stroke-linecap="round"
      />
      <polygon points={hp} fill={arrow.color} />
    {/if}

    {#if isSelected}
      <circle cx={arrow.startX} cy={arrow.startY} r={6 / scale} fill="white" stroke="#0066cc" stroke-width={2 / scale} class="handle" />
      <circle cx={arrow.endX} cy={arrow.endY} r={6 / scale} fill="white" stroke="#0066cc" stroke-width={2 / scale} class="handle" />
    {/if}
  {/each}
</svg>

<style>
  @reference "../app.css";

  .arrow-overlay {
    @apply absolute top-0 left-0 w-full h-full;
  }

  .handle {
    @apply cursor-grab;
  }

  .handle:active {
    @apply cursor-grabbing;
  }
</style>
