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
    /** 線の太さ・文字の大きさに掛ける倍率。Retina の撮影なら 2 (AGENTS.md の「Retina の撮影」) */
    annotationScale?: number;
    onStepsChange: (steps: Step[]) => void;
    onBeforeMutate?: () => void;
    /** バッジを選んだ時 (親が他のオーバーレイの選択を外す。Delete が両方に効かないように) */
    onSelect?: () => void;
  }

  let { steps, settings, toolActive, interactive, scale, annotationScale = 1, onStepsChange, onBeforeMutate, onSelect }: Props = $props();

  let selectedId = $state<string | null>(null);
  let dragging = $state(false);
  /** その移動で undo を積んだか。押しただけの空振りで積まないための印 */
  let dragMutated = $state(false);
  let dragStart = $state<{ x: number; y: number } | null>(null);
  let dragOrigPos = $state<{ x: number; y: number } | null>(null);
  let svgEl = $state<SVGSVGElement | null>(null);

  // 座標は svg 要素の矩形から取る。ツールが無効の間は svg 自体がポインタを受けず (下の
  // ArrowOverlay に通すため)、バッジだけが受けるので、currentTarget に頼らない
  function getSvgCoords(e: MouseEvent): { x: number; y: number } | null {
    const svg = svgEl ?? (e.target as Element).closest("svg");
    if (!svg) return null;
    const rect = svg.getBoundingClientRect();
    return { x: (e.clientX - rect.left) / scale, y: (e.clientY - rect.top) / scale };
  }

  // 画面上で 4px の余裕を付けて掴みやすくする (縮小表示でも見た目の距離で揃える)
  const hitSlack = () => 4 / (scale || 1);

  function handleMouseDown(e: MouseEvent) {
    const pt = getSvgCoords(e);
    if (!pt) return;

    const hit = topmostStepAt(steps, pt.x, pt.y, hitSlack(), annotationScale);
    if (hit) {
      // 選ぶと同時に移動を始められる (undo は動いた時に積む)
      selectedId = hit.id;
      onSelect?.();
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
    onSelect?.();
  }

  // ドラッグ中の mousemove は window で受ける。ツールが無効の間は svg がポインタを受けないので、
  // バッジから指が外れた瞬間に移動が止まってしまう
  function handleMouseMove(e: MouseEvent) {
    if (!dragging) return;
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
  }

  function handleMouseUp() {
    dragging = false;
    dragMutated = false;
    dragStart = null;
    dragOrigPos = null;
  }

  // 画像の上で自分の svg の外 (他のオーバーレイの注釈、空いた所) を押したら選択を外す。外さないと、
  // バッジを選んだ後に矢印を選んで Delete を押した時、両方の window keydown が走って
  // (stopPropagation は同じ window の別のリスナーを止めない) バッジまで消える。
  // 画像の外 (ツールバーの設定・サイドバー) のクリックでは外さない — 設定を押した瞬間に選択が
  // 消えると、選択中のバッジへ色や大きさを効かせられなくなる (updateActiveAttribute)
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

<!-- mousemove / mouseup は window で受ける: 画像の外でボタンを離しても移動を終え (戻ってきた時に
     動かさない)、ツールが無効で svg がポインタを受けない間もドラッグが続く -->
<svelte:window
  onkeydown={handleKeyDown}
  onmousedown={handleWindowMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
/>

<!-- ツールが有効な間だけ svg 全体でクリックを受ける (空いた所へのクリック = 追加)。それ以外の間は
     svg はポインタを通し (下の ArrowOverlay が矢印の選択を受け取れる)、バッジだけがポインタを受ける -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg
  bind:this={svgEl}
  class="step-overlay"
  onmousedown={handleMouseDown}
  style:cursor={dragging ? "move" : toolActive ? "crosshair" : "default"}
  style:pointer-events={interactive && toolActive ? "auto" : "none"}
>
  <defs>
    <filter id="step-shadow" x="-30%" y="-30%" width="160%" height="160%">
      <feDropShadow dx={2 * annotationScale} dy={2 * annotationScale} stdDeviation={2 * annotationScale} flood-opacity="0.5" />
    </filter>
  </defs>

  {#each steps as s (s.id)}
    {@const isSelected = s.id === selectedId}
    {@const size = s.size * annotationScale}
    {@const r = size / 2}
    {@const rim = 2 * annotationScale}
    {@const fontSize = stepFontSize(size, s.number)}
    <!-- 白縁は円の外側に 2px (太さ 4 の線の外半分)。影は白縁があればそちらに、無ければ本体に付ける。
         バッジの群だけがポインタを受ける (interactive の間)。当たり判定の円は hitSlack ぶん広い -->
    <g
      class="step-badge"
      style:pointer-events={interactive ? "auto" : "none"}
      style:cursor={dragging ? "move" : isSelected ? "move" : "pointer"}
    >
    <circle cx={s.x} cy={s.y} r={r + rim + hitSlack()} fill="transparent" />
    {#if s.whiteStroke}
      <circle
        cx={s.x} cy={s.y} r={r + rim}
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
        cx={s.x} cy={s.y} r={r + 4 * annotationScale}
        fill="none" stroke="#0066cc" stroke-width={2 / scale} stroke-dasharray="{4 / scale} {2 / scale}"
      />
    {/if}
    </g>
  {/each}
</svg>

<style>
  @reference "../app.css";

  .step-overlay {
    @apply absolute top-0 left-0 w-full h-full;
  }
</style>
