<script lang="ts">
  // Windows の撮影範囲を選ぶオーバーレイ (CYBERNEURA-DEV-882)。
  //
  // Rust (src-tauri/src/region_capture.rs) が撮っておいたモニター全体の静止画を
  // ウインドウいっぱいに映し、その上で矩形を引かせる。離した時点で撮影が確定する
  // (macOS の screencapture -i と同じ)。Enter は選ばずにモニター全体、
  // Esc と右クリックはやめる。
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { dragRect, isDrag, toImageRect, type Rect } from "$lib/regionSelect";

  let imageUrl = $state<string | null>(null);
  let imageEl = $state<HTMLImageElement | null>(null);
  let failed = $state<string | null>(null);

  // ドラッグ中の始点と現在点 (CSS px)
  let anchor = $state<{ x: number; y: number } | null>(null);
  let pointer = $state<{ x: number; y: number } | null>(null);
  // 一度確定したら、後から届いた操作は無視する (finish の二重送信を防ぐ)
  let finished = false;

  let viewW = $state(0);
  let viewH = $state(0);

  let rect = $derived<Rect | null>(
    anchor && pointer ? dragRect(anchor.x, anchor.y, pointer.x, pointer.y, viewW, viewH) : null,
  );
  // 表示する寸法は撮れる画像のピクセル数 (高 DPI では CSS px と違う)
  let imageRect = $derived(
    rect && imageEl ? toImageRect(rect, viewW, viewH, imageEl.naturalWidth, imageEl.naturalHeight) : null,
  );

  async function finish(selection: Rect | null) {
    if (finished) return;
    finished = true;
    try {
      await invoke("capture_region_finish", { selection });
    } catch (e) {
      console.error("Failed to finish the area selection:", e);
    }
  }

  function wholeScreen() {
    if (!imageEl) return;
    finish({ x: 0, y: 0, width: imageEl.naturalWidth, height: imageEl.naturalHeight });
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button === 2) {
      e.preventDefault();
      finish(null);
      return;
    }
    if (e.button !== 0 || !imageEl) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    anchor = { x: e.clientX, y: e.clientY };
    pointer = { x: e.clientX, y: e.clientY };
  }

  function onPointerMove(e: PointerEvent) {
    if (!anchor) return;
    pointer = { x: e.clientX, y: e.clientY };
  }

  function onPointerUp(e: PointerEvent) {
    if (!anchor || e.button !== 0) return;
    const start = anchor;
    pointer = { x: e.clientX, y: e.clientY };
    const moved = isDrag(start.x, start.y, e.clientX, e.clientY);
    const selected = imageRect;
    anchor = null;
    pointer = null;
    // クリックしただけなら何もしない (引き直せるように選択前へ戻す)
    if (!moved || !selected || selected.width < 1 || selected.height < 1) return;
    finish(selected);
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      finish(null);
    } else if (e.key === "Enter") {
      // ボタンにフォーカスがある時の Enter はそのボタンの操作 (Tab で Cancel を選んで
      // Enter を押したのに、モニター全体を撮ってしまわないように)
      if (e.target instanceof Element && e.target.closest("button")) return;
      e.preventDefault();
      wholeScreen();
    }
  }

  function updateViewSize() {
    viewW = window.innerWidth;
    viewH = window.innerHeight;
  }

  async function onImageLoad() {
    try {
      await invoke("capture_region_ready");
    } catch (e) {
      console.error("Failed to show the area selection:", e);
    }
  }

  onMount(() => {
    updateViewSize();
    invoke<string>("capture_region_preview")
      .then((url) => {
        imageUrl = url;
      })
      .catch((e) => {
        // 画像が無いまま選ばせても撮れないので、やめたことにして戻す
        console.error("Failed to load the screenshot for the area selection:", e);
        failed = String(e);
        finish(null);
      });
  });
</script>

<svelte:window onkeydown={onKeyDown} onresize={updateViewSize} oncontextmenu={(e) => e.preventDefault()} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 overflow-hidden select-none cursor-crosshair bg-black"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
>
  {#if imageUrl}
    <img
      bind:this={imageEl}
      src={imageUrl}
      alt=""
      draggable="false"
      class="absolute inset-0 w-full h-full pointer-events-none"
      onload={onImageLoad}
    />
  {/if}

  <!-- 全体を薄暗くし、選択範囲だけ穴を開ける -->
  <div
    class="absolute inset-0 bg-black/35 pointer-events-none"
    style={rect
      ? `clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%, 0 0, ${rect.x}px ${rect.y}px, ${rect.x}px ${rect.y + rect.height}px, ${rect.x + rect.width}px ${rect.y + rect.height}px, ${rect.x + rect.width}px ${rect.y}px, ${rect.x}px ${rect.y}px);`
      : ""}
  ></div>

  {#if rect}
    <div
      class="absolute border border-white outline outline-1 outline-black/60 pointer-events-none"
      style="left:{rect.x}px;top:{rect.y}px;width:{rect.width}px;height:{rect.height}px;"
    >
      {#if imageRect}
        <div
          class="absolute left-1 top-1 bg-black/80 text-white text-xs px-1.5 py-0.5 rounded tabular-nums whitespace-nowrap"
        >
          {imageRect.width} × {imageRect.height}
        </div>
      {/if}
    </div>
  {:else}
    <div class="absolute inset-x-0 top-8 flex justify-center pointer-events-none">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="flex items-center gap-3 bg-black/75 text-white text-sm px-4 py-2 rounded-xl shadow-lg pointer-events-auto cursor-default"
        onpointerdown={(e) => e.stopPropagation()}
      >
        {#if failed}
          <span>Could not prepare the capture: {failed}</span>
        {:else}
          <span>Drag to select an area · Enter: whole screen · Esc: cancel</span>
          <button class="hint-btn" onclick={wholeScreen}>
            <i class="bi bi-fullscreen"></i> Whole screen
          </button>
          <button class="hint-btn" onclick={() => finish(null)}>
            <i class="bi bi-x-lg"></i> Cancel
          </button>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  @reference "../../app.css";

  .hint-btn {
    @apply flex items-center gap-1.5 px-3 h-8 rounded-md bg-neutral-700
      text-neutral-200 text-xs cursor-pointer border-none transition-colors;
  }
  .hint-btn:hover {
    @apply bg-neutral-600 text-white;
  }
</style>
