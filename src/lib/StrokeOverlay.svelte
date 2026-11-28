<script lang="ts">
  import type { ArrowSettings, Stroke, StrokePoint } from "./types";
  import { exceedsDragSlop } from "./textHit";
  import {
    STROKE_MIN_STEP,
    appendStrokePoint,
    finishStrokePoints,
    offsetStrokeCopy,
    strokeAt,
    strokeBounds,
    strokeLength,
    strokePath,
  } from "./stroke";

  // Freehand pen (CYBERNEURA-DEV-1401). The geometry lives in stroke.ts so that the export
  // (renderComposite) draws exactly the same path.

  interface Props {
    strokes: Stroke[];
    /** The pen uses the arrow tool's color / thickness / white border / shadow */
    settings: ArrowSettings;
    toolActive: boolean;
    interactive: boolean;
    scale: number;
    /** Multiplier for thickness and shadow. 2 for Retina captures (AGENTS.md "Annotations on Retina captures") */
    annotationScale?: number;
    onStrokesChange: (strokes: Stroke[]) => void;
    onBeforeMutate?: () => void;
    /** Bound by +page.svelte so the toolbar can show the selected stroke's settings */
    selectedId?: string | null;
  }

  let { strokes, settings, toolActive, interactive, scale, annotationScale = 1, onStrokesChange, onBeforeMutate, selectedId = $bindable(null) }: Props = $props();

  // "press" = pressed on a stroke, waiting to see whether it is a click (select) or a drag (move).
  // "duplicate" = pressed with Option / Alt, the copy is made on the first real movement
  let dragging = $state<"draw" | "press" | "move" | "duplicate" | null>(null);
  let drawingStroke = $state<Stroke | null>(null);
  let pressStart: StrokePoint | null = null;
  /** The stroke being moved, as it was when the move started */
  let moveOrig: Stroke | null = null;
  let duplicateSource: Stroke | null = null;
  let hoverCursor = $state<string>("default");

  let svgEl = $state<SVGSVGElement | null>(null);

  // Read against the svg itself: move / up are received on window (the pointer can leave the line
  // while dragging), so `currentTarget` is not the svg there
  function getSvgCoords(e: MouseEvent): StrokePoint | null {
    const svg = svgEl;
    if (!svg) return null;
    const rect = svg.getBoundingClientRect();
    return { x: (e.clientX - rect.left) / scale, y: (e.clientY - rect.top) / scale };
  }

  function hitAt(pt: StrokePoint): Stroke | undefined {
    return strokeAt(strokes, pt.x, pt.y, scale, annotationScale);
  }

  function handleMouseDown(e: MouseEvent) {
    const pt = getSvgCoords(e);
    if (!pt) return;

    const hit = hitAt(pt);
    if (hit) {
      pressStart = pt;
      if (e.altKey) {
        // Option / Alt+drag duplicates (CYBERNEURA-DEV-1321). Releasing without moving only selects
        duplicateSource = hit;
        dragging = "duplicate";
        return;
      }
      selectedId = hit.id;
      moveOrig = hit;
      dragging = "press";
      return;
    }

    if (!toolActive) {
      selectedId = null;
      return;
    }

    // Undo is pushed when the stroke is committed (mouseup), not here: a click that draws nothing
    // must not add an empty step or drop the Redo history
    drawingStroke = {
      id: crypto.randomUUID(),
      points: [pt],
      color: settings.color,
      thickness: settings.thickness,
      whiteStroke: settings.whiteStroke,
      dropShadow: settings.dropShadow,
    };
    dragging = "draw";
    selectedId = null;
  }

  function updateHoverCursor(pt: StrokePoint) {
    if (dragging) return;
    const hit = hitAt(pt);
    if (hit) {
      hoverCursor = hit.id === selectedId ? "move" : "pointer";
      return;
    }
    hoverCursor = toolActive ? "crosshair" : "default";
  }

  function handleMouseMove(e: MouseEvent) {
    // Received on window: skip the layout read entirely unless something here can use it.
    // The svg's own cursor only matters while the tool is active (it takes the pointer only then)
    if (!dragging && !toolActive) return;
    const pt = getSvgCoords(e);
    if (!pt) return;

    if (!dragging) {
      updateHoverCursor(pt);
      return;
    }

    if (dragging === "draw" && drawingStroke) {
      // Thin out in screen px: the same drag records the same density at any zoom
      const next = appendStrokePoint(drawingStroke.points, pt, STROKE_MIN_STEP / scale);
      if (next !== drawingStroke.points) drawingStroke = { ...drawingStroke, points: next };
      return;
    }

    if (!pressStart) return;
    const dx = pt.x - pressStart.x;
    const dy = pt.y - pressStart.y;

    if (dragging === "duplicate" && duplicateSource) {
      if (!exceedsDragSlop(dx, dy, scale)) return;
      onBeforeMutate?.();
      const copy = offsetStrokeCopy(duplicateSource, 0, 0, crypto.randomUUID());
      moveOrig = copy;
      onStrokesChange([...strokes, offsetStrokeCopy(copy, dx, dy, copy.id)]);
      selectedId = copy.id;
      duplicateSource = null;
      dragging = "move";
    } else if (dragging === "press" && moveOrig) {
      // Push undo on the first real movement, not on press (re-selecting must not add an empty step)
      if (!exceedsDragSlop(dx, dy, scale)) return;
      onBeforeMutate?.();
      dragging = "move";
      moveTo(dx, dy);
    } else if (dragging === "move") {
      moveTo(dx, dy);
    }
  }

  function moveTo(dx: number, dy: number) {
    const orig = moveOrig;
    if (!orig) return;
    const moved = offsetStrokeCopy(orig, dx, dy, orig.id);
    onStrokesChange(strokes.map((s) => (s.id === orig.id ? moved : s)));
  }

  function handleMouseUp(e: MouseEvent) {
    if (dragging === "draw" && drawingStroke) {
      const pt = getSvgCoords(e);
      const points = pt ? finishStrokePoints(drawingStroke.points, pt) : drawingStroke.points;
      // Same minimum as the arrow: a click without drawing does not leave a dot
      if (strokeLength(points) > 5) {
        const done = { ...drawingStroke, points };
        onBeforeMutate?.();
        onStrokesChange([...strokes, done]);
        selectedId = done.id;
      }
    }
    if (dragging === "duplicate" && duplicateSource) selectedId = duplicateSource.id;
    drawingStroke = null;
    duplicateSource = null;
    moveOrig = null;
    pressStart = null;
    dragging = null;
  }

  // Pressing on the image outside this svg (another overlay's annotation, empty space) clears the
  // selection, so that Delete after selecting something else does not remove the stroke too.
  // Presses outside the image (toolbar settings, sidebar) keep it, so the toolbar can restyle it.
  // Same as StepOverlay
  function handleWindowMouseDown(e: MouseEvent) {
    if (!selectedId || !svgEl || !(e.target instanceof Node)) return;
    if (svgEl.contains(e.target)) return;
    const imageArea = svgEl.parentElement;
    if (imageArea && !imageArea.contains(e.target)) return;
    selectedId = null;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if ((e.key === "Delete" || e.key === "Backspace") && selectedId) {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || (e.target as HTMLElement)?.isContentEditable) return;
      e.preventDefault();
      e.stopPropagation();
      onBeforeMutate?.();
      onStrokesChange(strokes.filter((s) => s.id !== selectedId));
      selectedId = null;
    }
  }

  let allStrokes = $derived(drawingStroke ? [...strokes, drawingStroke] : strokes);
  let selectedStroke = $derived(strokes.find((s) => s.id === selectedId) ?? null);
  let selectedBounds = $derived(selectedStroke ? strokeBounds(selectedStroke.points) : null);

  export function deselect() {
    selectedId = null;
  }

  /**
   * Drops a drag in progress without committing it. Called by +page.svelte when the image is replaced,
   * so a line started on the previous image is not added to the new one on mouseup
   */
  export function cancelGesture() {
    drawingStroke = null;
    duplicateSource = null;
    moveOrig = null;
    pressStart = null;
    dragging = null;
  }
</script>

<svelte:window
  onkeydown={handleKeyDown}
  onmousedown={handleWindowMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
/>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- The whole svg receives presses only while the pen tool is active (a press on empty space draws).
     Otherwise only the invisible hit paths do, so this overlay (above ArrowOverlay) does not steal
     presses meant for the arrows below. Same approach as StepOverlay -->
<svg
  bind:this={svgEl}
  class="stroke-overlay"
  onmousedown={handleMouseDown}
  style:cursor={dragging === "move" ? "move" : hoverCursor}
  style:pointer-events={toolActive ? "auto" : "none"}
>
  <defs>
    <!-- userSpaceOnUse for the same reason as the arrow: a straight horizontal stroke has a zero-height bounding box -->
    <filter id="stroke-shadow" filterUnits="userSpaceOnUse" x="0" y="0" width="100%" height="100%">
      <feDropShadow dx={2 * annotationScale} dy={2 * annotationScale} stdDeviation={2 * annotationScale} flood-opacity="0.5" />
    </filter>
  </defs>

  {#each allStrokes as stroke (stroke.id)}
    {@const d = strokePath(stroke.points)}
    {@const w = stroke.thickness * annotationScale}
    {#if stroke.whiteStroke}
      <!-- The shadow goes on the white border if there is one, otherwise on the body (same as the arrow and the export) -->
      <path
        {d}
        fill="none"
        stroke="white"
        stroke-width={(stroke.thickness + 4) * annotationScale}
        stroke-linecap="round"
        stroke-linejoin="round"
        filter={stroke.dropShadow ? "url(#stroke-shadow)" : undefined}
      />
    {/if}
    <path
      {d}
      fill="none"
      stroke={stroke.color}
      stroke-width={w}
      stroke-linecap="round"
      stroke-linejoin="round"
      filter={stroke.dropShadow && !stroke.whiteStroke ? "url(#stroke-shadow)" : undefined}
    />
    {#if interactive && stroke !== drawingStroke}
      <!-- Hit area: the same tolerance as hitsStroke (drawn width + 6 screen px on each side) -->
      <path
        {d}
        fill="none"
        stroke="transparent"
        stroke-width={w * 2 + 12 / scale}
        stroke-linecap="round"
        stroke-linejoin="round"
        pointer-events="stroke"
        style:cursor={stroke.id === selectedId ? "move" : "pointer"}
      />
    {/if}
  {/each}

  {#if selectedStroke && selectedBounds}
    <!-- Selection frame around the drawn line (half the drawn width + 6 screen px) -->
    {@const pad = (selectedStroke.thickness * annotationScale) / 2 + 6 / scale}
    <rect
      x={selectedBounds.x - pad}
      y={selectedBounds.y - pad}
      width={selectedBounds.width + pad * 2}
      height={selectedBounds.height + pad * 2}
      fill="none"
      stroke="#0066cc"
      stroke-width={1.5 / scale}
      stroke-dasharray="{4 / scale} {3 / scale}"
      pointer-events="none"
    />
  {/if}
</svg>

<style>
  @reference "../app.css";

  .stroke-overlay {
    @apply absolute top-0 left-0 w-full h-full;
  }
</style>
