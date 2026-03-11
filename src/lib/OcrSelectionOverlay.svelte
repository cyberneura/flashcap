<script lang="ts">
  let {
    active,
    scale,
    onRegionSelected,
  }: {
    active: boolean;
    scale: number;
    onRegionSelected: (region: { x: number; y: number; width: number; height: number }) => void;
  } = $props();

  let svgEl = $state<SVGSVGElement | null>(null);
  let dragging = $state(false);
  let startX = $state(0);
  let startY = $state(0);
  let currentX = $state(0);
  let currentY = $state(0);

  function getSvgCoords(e: MouseEvent): { x: number; y: number } | null {
    if (!svgEl) return null;
    const rect = svgEl.getBoundingClientRect();
    return { x: (e.clientX - rect.left) / scale, y: (e.clientY - rect.top) / scale };
  }

  function handleMouseDown(e: MouseEvent) {
    if (!active) return;
    const pt = getSvgCoords(e);
    if (!pt) return;
    dragging = true;
    startX = pt.x;
    startY = pt.y;
    currentX = pt.x;
    currentY = pt.y;
  }

  // ドラッグ中の mousemove/mouseup はウィンドウレベルで処理し、
  // SVG 外でマウスを離しても dragging が残らないようにする
  function handleWindowMouseMove(e: MouseEvent) {
    if (!dragging) return;
    const pt = getSvgCoords(e);
    if (!pt) return;
    currentX = pt.x;
    currentY = pt.y;
  }

  function handleWindowMouseUp() {
    if (!dragging) return;
    dragging = false;
    const x = Math.min(startX, currentX);
    const y = Math.min(startY, currentY);
    const width = Math.abs(currentX - startX);
    const height = Math.abs(currentY - startY);
    if (width > 5 && height > 5) {
      onRegionSelected({ x, y, width, height });
    }
  }

  let rectX = $derived(Math.min(startX, currentX));
  let rectY = $derived(Math.min(startY, currentY));
  let rectW = $derived(Math.abs(currentX - startX));
  let rectH = $derived(Math.abs(currentY - startY));
</script>

<svelte:window onmousemove={handleWindowMouseMove} onmouseup={handleWindowMouseUp} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg
  bind:this={svgEl}
  class="ocr-selection-overlay"
  onmousedown={handleMouseDown}
  style:pointer-events={active ? "auto" : "none"}
>
  {#if active && dragging}
    <rect
      x={rectX} y={rectY} width={rectW} height={rectH}
      fill="rgba(59, 130, 246, 0.2)"
      stroke="#3b82f6"
      stroke-width={2 / scale}
      stroke-dasharray="{4 / scale} {2 / scale}"
    />
  {/if}
</svg>

<style>
  @reference "../app.css";

  .ocr-selection-overlay {
    @apply absolute top-0 left-0 w-full h-full z-50;
    cursor: crosshair;
  }
</style>
