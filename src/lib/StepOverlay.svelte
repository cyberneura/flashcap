<script lang="ts">
  import type { Step, StepSettings } from "./types";
  import { exceedsDragSlop } from "./textHit";
  import { nextStep, renumber, stepFontSize, stepLabelBaseline, topmostStepAt } from "./steps";

  /**
   * 番号付きステップ (カウンター) のオーバーレイ (CYBERNEURA-DEV-1209)。
   * ツールが有効な時のクリックで 1, 2, 3… のバッジを末尾に足し、選択中のバッジは
   * ドラッグで動かし、Delete / Backspace で消す (消したら後続の番号を詰め直す)。
   * 押しただけでは undo を積まない (TextOverlay と同じく、動いた最初の 1 回で積む)。
   */
  interface Props {
    steps: Step[];
    settings: StepSettings;
    toolActive: boolean;
    interactive: boolean;
    scale: number;
    onStepsChange: (steps: Step[]) => void;
    onBeforeMutate?: () => void;
  }

  let { steps, settings, toolActive, interactive, scale, onStepsChange, onBeforeMutate }: Props = $props();

  let selectedId = $state<string | null>(null);
  let dragging = $state(false);
  /** その移動で undo を積んだか。押しただけの空振りで積まないための印 */
  let dragMutated = $state(false);
  let dragStart = $state<{ x: number; y: number } | null>(null);
  let dragOrigPos = $state<{ x: number; y: number } | null>(null);
  let hoverCursor = $state<string>("default");

  function getSvgCoords(e: MouseEvent): { x: number; y: number } | null {
    const svg = (e.currentTarget as SVGSVGElement) ?? (e.target as Element).closest("svg");
    if (!svg) return null;
    const rect = svg.getBoundingClientRect();
    return { x: (e.clientX - rect.left) / scale, y: (e.clientY - rect.top) / scale };
  }

  // 画面上で 4px の余裕を付けて掴みやすくする (縮小表示でも見た目の距離で揃える)
  const hitSlack = () => 4 / (scale || 1);

  function handleMouseDown(e: MouseEvent) {
    const pt = getSvgCoords(e);
    if (!pt) return;

    const hit = topmostStepAt(steps, pt.x, pt.y, hitSlack());
    if (hit) {
      // 選ぶと同時に移動を始められる (undo は動いた時に積む)
      selectedId = hit.id;
      dragging = true;
      dragMutated = false;
      dragStart = pt;
      dragOrigPos = { x: hit.x, y: hit.y };
      return;
    }

    if (!toolActive) {
      selectedId = null;
      return;
    }

    onBeforeMutate?.();
    const step = nextStep(steps, pt.x, pt.y, settings);
    onStepsChange([...steps, step]);
    selectedId = step.id;
  }

  function handleMouseMove(e: MouseEvent) {
    const pt = getSvgCoords(e);
    if (!pt) return;

    if (dragging && selectedId && dragStart && dragOrigPos) {
      const dx = pt.x - dragStart.x;
      const dy = pt.y - dragStart.y;
      if (!dragMutated) {
        if (!exceedsDragSlop(dx, dy, scale)) return;
        dragMutated = true;
        onBeforeMutate?.();
      }
      onStepsChange(
        steps.map((s) =>
          s.id === selectedId ? { ...s, x: dragOrigPos!.x + dx, y: dragOrigPos!.y + dy } : s
        )
      );
      return;
    }

    if (!dragging) {
      const hit = topmostStepAt(steps, pt.x, pt.y, hitSlack());
      hoverCursor = hit ? (hit.id === selectedId ? "move" : "pointer") : toolActive ? "crosshair" : "default";
    }
  }

  function handleMouseUp() {
    dragging = false;
    dragMutated = false;
    dragStart = null;
    dragOrigPos = null;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if ((e.key === "Delete" || e.key === "Backspace") && selectedId) {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || (e.target as HTMLElement)?.isContentEditable) return;
      e.preventDefault();
      e.stopPropagation();
      onBeforeMutate?.();
      onStepsChange(renumber(steps.filter((s) => s.id !== selectedId)));
      selectedId = null;
    }
  }

  /** 選択中のバッジの属性を更新する (ツールバーの設定変更を選択中のものにも効かせる) */
  export function updateActiveAttribute<K extends keyof StepSettings>(key: K, value: StepSettings[K]) {
    if (!selectedId) return;
    onStepsChange(steps.map((s) => (s.id === selectedId ? { ...s, [key]: value } : s)));
  }

  export function deselect() {
    selectedId = null;
  }
</script>

<!-- mouseup は window でも受ける: 画像の外でボタンを離しても移動を終える (戻ってきた時に動かさない) -->
<svelte:window onkeydown={handleKeyDown} onmouseup={handleMouseUp} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg
  class="step-overlay"
  onmousedown={handleMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
  style:cursor={dragging ? "move" : hoverCursor}
  style:pointer-events={interactive ? "auto" : "none"}
>
  <defs>
    <filter id="step-shadow" x="-30%" y="-30%" width="160%" height="160%">
      <feDropShadow dx="2" dy="2" stdDeviation="2" flood-opacity="0.5" />
    </filter>
  </defs>

  {#each steps as s (s.id)}
    {@const isSelected = s.id === selectedId}
    {@const r = s.size / 2}
    {@const fontSize = stepFontSize(s.size, s.number)}
    <!-- 白縁は円の外側に 2px (太さ 4 の線の外半分)。影は白縁があればそちらに、無ければ本体に付ける -->
    {#if s.whiteStroke}
      <circle
        cx={s.x} cy={s.y} r={r + 2}
        fill="white"
        filter={s.dropShadow ? "url(#step-shadow)" : undefined}
      />
    {/if}
    <circle
      cx={s.x} cy={s.y} r={r}
      fill={s.color}
      filter={s.dropShadow && !s.whiteStroke ? "url(#step-shadow)" : undefined}
    />
    <!-- 数字は alphabetic baseline + 固定の下げ幅で中央に置く (canvas 側と同じ幾何。steps.ts 参照) -->
    <text
      x={s.x} y={stepLabelBaseline(s.y, fontSize)}
      fill="white"
      font-size={fontSize}
      font-weight="700"
      font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
      text-anchor="middle"
      style="pointer-events: none; user-select: none;"
    >{s.number}</text>
    {#if isSelected}
      <circle
        cx={s.x} cy={s.y} r={r + 4}
        fill="none" stroke="#0066cc" stroke-width={2 / scale} stroke-dasharray="{4 / scale} {2 / scale}"
      />
    {/if}
  {/each}
</svg>

<style>
  @reference "../app.css";

  .step-overlay {
    @apply absolute top-0 left-0 w-full h-full;
  }
</style>
