<script lang="ts">
  let {
    active,
    scale,
    onRegionSelected,
    onCancel,
  }: {
    active: boolean;
    scale: number;
    onRegionSelected: (region: { x: number; y: number; width: number; height: number }) => void;
    onCancel: () => void;
  } = $props();

  let dragging = $state(false);
  let startX = $state(0);
  let startY = $state(0);
  let currentX = $state(0);
  let currentY = $state(0);

  function getSvgCoords(e: MouseEvent): { x: number; y: number } | null {
    const svg = (e.currentTarget as SVGSVGElement) ?? (e.target as Element).closest("svg");
    if (!svg) return null;
    const rect = svg.getBoundingClientRect();
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

  function handleMouseMove(e: MouseEvent) {
    if (!dragging) return;
    const pt = getSvgCoords(e);
    if (!pt) return;
    currentX = pt.x;
    currentY = pt.y;
  }

  function handleMouseUp() {
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

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg
  class="ocr-selection-overlay"
  onmousedown={handleMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
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
