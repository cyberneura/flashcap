<script lang="ts">
  import type { TextAnnotation, TextSettings } from "./types";
  import { isModKey } from "./platform";
  import {
    exceedsDragSlop,
    hitsText,
    LINE_HEIGHT_RATIO,
    textBBox,
    topmostTextAt,
  } from "./textHit";

  interface Props {
    texts: TextAnnotation[];
    settings: TextSettings;
    toolActive: boolean;
    interactive: boolean;
    scale: number;
    onTextsChange: (texts: TextAnnotation[]) => void;
    onBeforeMutate?: () => void;
  }

  let { texts, settings, toolActive, interactive, scale, onTextsChange, onBeforeMutate }: Props = $props();

  let selectedId = $state<string | null>(null);
  let editingId = $state<string | null>(null);
  let dragging = $state<"move" | null>(null);
  /** その移動で undo を積んだか。押しただけの空振りで積まないための印 */
  let dragMutated = $state(false);
  /** その編集で undo を積んだか。開いて閉じただけの空振りで積まないための印 */
  let editMutated = $state(false);
  let dragStart = $state<{ x: number; y: number } | null>(null);
  let dragOrigPos = $state<{ x: number; y: number } | null>(null);
  let hoverCursor = $state<string>("default");

  // 新規テキスト作成中の一時状態
  let pendingText = $state<TextAnnotation | null>(null);

  function getSvgCoords(e: MouseEvent): { x: number; y: number } | null {
    const svg = (e.currentTarget as SVGSVGElement) ?? (e.target as Element).closest("svg");
    if (!svg) return null;
    const rect = svg.getBoundingClientRect();
    return { x: (e.clientX - rect.left) / scale, y: (e.clientY - rect.top) / scale };
  }

  // 箱と当たり判定は `textHit.ts`。選択・ダブルクリック・カーソルの全部がこの
  // 判定で決まるので、DOM もマウスも要らない形にしてテストから直接呼べる場所に
  // 置いてある。
  const getTextBBox = textBBox;
  const hitTestText = hitsText;

  function handleMouseDown(e: MouseEvent) {
    // 編集中の input クリックは透過
    if ((e.target as Element).closest(".text-edit-input")) return;

    const pt = getSvgCoords(e);
    if (!pt) return;

    // 選択済みテキストの移動開始
    //
    // **`onBeforeMutate` はここでは呼ばない。** 押しただけで undo が 1 段積まれると、
    // 選び直しやダブルクリックのたびに「何も変わっていない状態」が履歴に入る。
    // 実際に動いた最初の 1 回で積む (handleMouseMove)。
    if (selectedId && !editingId) {
      const sel = texts.find((t) => t.id === selectedId);
      if (sel && hitTestText(sel, pt.x, pt.y)) {
        dragging = "move";
        dragMutated = false;
        dragStart = pt;
        dragOrigPos = { x: sel.x, y: sel.y };
        return;
      }
    }

    // 既存テキストをクリックで選択。
    //
    // 選択済みのものを押した場合は上の移動で return しているので、ここに来るのは
    // 「まだ選ばれていないものを押した」時だけ。**再編集はダブルクリック**
    // (handleDoubleClick) で、1 回のクリックでは入らない — 選んで動かすつもりの
    // クリックが編集を開くと、位置を直すたびにキーボードが割り込む
    // (CYBERNEURA-DEV-695)。
    const hit = topmostTextAt(texts, pt.x, pt.y);
    if (hit) {
      selectedId = hit.id;
      editingId = null;
      return;
    }

    if (!toolActive) {
      commitEditing();
      selectedId = null;
      editingId = null;
      return;
    }

    // 新規テキスト作成
    commitEditing();
    onBeforeMutate?.();
    const id = crypto.randomUUID();
    pendingText = {
      id,
      x: pt.x,
      y: pt.y,
      text: "",
      fontSize: settings.fontSize,
      color: settings.color,
      bold: settings.bold,
      italic: settings.italic,
      whiteStroke: settings.whiteStroke,
      dropShadow: settings.dropShadow,
    };
    startEditing(id);
    selectedId = id;
  }

  function handleMouseMove(e: MouseEvent) {
    const pt = getSvgCoords(e);
    if (!pt) return;

    if (dragging === "move" && selectedId && dragStart && dragOrigPos) {
      const dx = pt.x - dragStart.x;
      const dy = pt.y - dragStart.y;
      // 動いた最初の 1 回だけ undo を積む。押した時点で積むと、選び直しや
      // ダブルクリックが「変化なし」の履歴を作る。
      //
      // 完全一致 (dx === 0) では足りない。ダブルクリックの 2 回目の押下も移動の
      // 開始なので、指が 1px 揺れただけで文字がずれ、編集を開くたびに undo が
      // 1 段積まれる。CropOverlay と同じ閾値で分ける。
      if (!dragMutated) {
        if (!exceedsDragSlop(dx, dy, scale)) return;
        dragMutated = true;
        onBeforeMutate?.();
      }
      onTextsChange(
        texts.map((t) =>
          t.id === selectedId
            ? { ...t, x: dragOrigPos!.x + dx, y: dragOrigPos!.y + dy }
            : t
        )
      );
      return;
    }

    if (!dragging) {
      updateHoverCursor(pt);
    }
  }

  function handleMouseUp() {
    dragging = null;
    dragMutated = false;
    dragStart = null;
    dragOrigPos = null;
  }

  /**
   * 書いたテキストを直す入口。
   *
   * 1 回のクリックは「選ぶ」「動かす」で、編集はダブルクリック。押しただけで
   * 編集に入ると、位置を直すつもりのクリックがそのたびにキーボードを呼び出す
   * (CYBERNEURA-DEV-695)。
   *
   * 直前の mousedown が始めた移動をここで畳む。ダブルクリックの 2 回目の押下も
   * 移動の開始なので、そのまま編集に入ると掴んだ状態が残る。
   */
  function handleDoubleClick(e: MouseEvent) {
    if ((e.target as Element).closest(".text-edit-input")) return;

    const pt = getSvgCoords(e);
    if (!pt) return;
    const hit = topmostTextAt(texts, pt.x, pt.y);
    if (!hit) return;

    e.preventDefault();
    e.stopPropagation();
    handleMouseUp();
    selectedId = hit.id;
    startEditing(hit.id);
  }

  /** 編集を開く。undo の印はここで下ろす (1 回の編集に 1 つ) */
  function startEditing(id: string) {
    editingId = id;
    editMutated = false;
  }

  function updateHoverCursor(pt: { x: number; y: number }) {
    if (dragging) return;

    if (selectedId && !editingId) {
      const sel = texts.find((t) => t.id === selectedId);
      if (sel && hitTestText(sel, pt.x, pt.y)) {
        hoverCursor = "move";
        return;
      }
    }

    if (topmostTextAt(texts, pt.x, pt.y)) {
      hoverCursor = "pointer";
      return;
    }

    hoverCursor = toolActive ? "text" : "default";
  }

  function commitEditing() {
    if (pendingText) {
      if (pendingText.text.trim()) {
        onTextsChange([...texts, pendingText]);
      }
      pendingText = null;
    }
    editingId = null;
    editMutated = false;
  }

  function handleEditInput(e: Event, id: string) {
    const value = (e.target as HTMLTextAreaElement).value;
    if (pendingText && pendingText.id === id) {
      // 新規は作成時にもう積んである (handleMouseDown)。1 文字ごとに積むと
      // undo が打鍵の巻き戻しになる。
      pendingText = { ...pendingText, text: value };
      return;
    }
    // 既にある注釈を書き換える最初の 1 回だけ積む。押した時ではなく最初の入力で
    // 積むのは移動と同じ理由で、開いて何も変えずに閉じた編集を履歴に残さないため。
    // ここが無いと、書き換えた直後の ⌘Z が**別の操作**を巻き戻す
    // (CYBERNEURA-DEV-695)。
    if (!editMutated) {
      editMutated = true;
      onBeforeMutate?.();
    }
    onTextsChange(texts.map((t) => (t.id === id ? { ...t, text: value } : t)));
  }

  function handleEditKeyDown(e: KeyboardEvent) {
    // Escape で編集を確定して終了
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      commitEditing();
      return;
    }
    // Cmd+Enter (Windows は Ctrl+Enter) で確定
    if (e.key === "Enter" && isModKey(e) && !(e as any).isComposing) {
      e.preventDefault();
      commitEditing();
      return;
    }
    // 通常の Enter はそのまま改行（textarea のデフォルト動作）
    // Backspace/Delete がグローバルに伝播しないようにする
    e.stopPropagation();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (editingId) return; // 編集中はグローバルショートカット無効
    if ((e.key === "Delete" || e.key === "Backspace") && selectedId) {
      e.preventDefault();
      e.stopPropagation();
      onBeforeMutate?.();
      onTextsChange(texts.filter((t) => t.id !== selectedId));
      selectedId = null;
    }
  }

  // 表示用: テキスト一覧 (pending を含む)
  let allTexts = $derived(pendingText ? [...texts, pendingText] : texts);

  function focusOnMount(node: HTMLElement) {
    setTimeout(() => node.focus(), 100);
  }

  // 編集中/選択中のテキストの属性を更新する
  export function updateActiveAttribute(key: string, value: unknown) {
    const targetId = editingId ?? selectedId;
    if (!targetId) return;

    if (pendingText && pendingText.id === targetId) {
      pendingText = { ...pendingText, [key]: value };
    } else {
      onTextsChange(texts.map((t) => (t.id === targetId ? { ...t, [key]: value } : t)));
    }
  }

  export function deselect() {
    commitEditing();
    selectedId = null;
    editingId = null;
  }
</script>

<svelte:window onkeydown={handleKeyDown} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg
  class="text-overlay"
  onmousedown={handleMouseDown}
  onmousemove={handleMouseMove}
  onmouseup={handleMouseUp}
  ondblclick={handleDoubleClick}
  style:cursor={dragging === "move" ? "move" : hoverCursor}
  style:pointer-events={interactive ? "auto" : "none"}
>
  <defs>
    <filter id="text-shadow" x="-20%" y="-20%" width="140%" height="140%">
      <feDropShadow dx="1" dy="1" stdDeviation="1.5" flood-opacity="0.6" />
    </filter>
  </defs>

  {#each allTexts as t (t.id)}
    {@const isSelected = t.id === selectedId}
    {@const isEditing = t.id === editingId}
    {@const filterAttr = t.dropShadow ? "url(#text-shadow)" : undefined}
    {@const bbox = getTextBBox(t)}
    {@const lineHeight = t.fontSize * LINE_HEIGHT_RATIO}
    {@const fontWeight = t.bold ? "900" : "normal"}
    {@const fontStyle = t.italic ? "italic" : "normal"}

    {#if isEditing}
      <foreignObject
        x={t.x} y={t.y}
        width={Math.max(200, bbox.width + 40)}
        height={Math.max(lineHeight + 16, bbox.height + 16)}
      >
        <textarea
          xmlns="http://www.w3.org/1999/xhtml"
          class="text-edit-input"
          style="font-size:{t.fontSize}px;color:{t.color};line-height:{lineHeight}px;min-width:180px;min-height:{lineHeight}px;font-weight:{fontWeight};font-style:{fontStyle};"
          value={pendingText && pendingText.id === t.id ? pendingText.text : t.text}
          oninput={(e) => handleEditInput(e, t.id)}
          onkeydown={handleEditKeyDown}
          use:focusOnMount
        ></textarea>
      </foreignObject>
    {:else}
      {#if t.text}
        {#if t.dropShadow && t.whiteStroke}
          <!-- 白枠のみに影を適用 -->
          <g filter="url(#text-shadow)">
            {#each t.text.split("\n") as line, i}
              <text
                x={t.x + 4} y={t.y + t.fontSize + i * lineHeight}
                font-size={t.fontSize}
                font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
                font-weight={fontWeight}
                font-style={fontStyle}
                fill="none"
                stroke="white"
                stroke-width={3}
                stroke-linejoin="round"
              >{line}</text>
            {/each}
          </g>
          <!-- 本体は影なし -->
          {#each t.text.split("\n") as line, i}
            <text
              x={t.x + 4} y={t.y + t.fontSize + i * lineHeight}
              font-size={t.fontSize}
              font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
              font-weight={fontWeight}
              font-style={fontStyle}
              fill={t.color}
            >{line}</text>
          {/each}
        {:else}
          <g filter={filterAttr}>
            {#each t.text.split("\n") as line, i}
              <text
                x={t.x + 4} y={t.y + t.fontSize + i * lineHeight}
                font-size={t.fontSize}
                font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif"
                font-weight={fontWeight}
                font-style={fontStyle}
                fill={t.color}
                stroke={t.whiteStroke ? "white" : "none"}
                stroke-width={t.whiteStroke ? 3 : 0}
                stroke-linejoin="round"
                paint-order="stroke"
              >{line}</text>
            {/each}
          </g>
        {/if}
      {/if}

      {#if isSelected}
        <rect
          x={bbox.x} y={bbox.y} width={bbox.width} height={bbox.height}
          fill="none" stroke="#0066cc" stroke-width={2 / scale} stroke-dasharray="{4 / scale} {2 / scale}"
        />
      {/if}
    {/if}
  {/each}
</svg>

<style>
  @reference "../app.css";

  .text-overlay {
    @apply absolute top-0 left-0 w-full h-full;
  }

  .text-edit-input {
    @apply bg-white/15 border border-[#0066cc] rounded-sm outline-none
      px-1 py-0.5 font-sans resize-none overflow-hidden box-border w-full h-full;
  }
</style>
