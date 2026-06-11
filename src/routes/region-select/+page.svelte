<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

  // 自オーバーレイのウィンドウラベル (region-selector-N)
  const myLabel = getCurrentWebviewWindow().label;

  // このオーバーレイが乗っているディスプレイの Quartz ポイント原点 (Rust から URL クエリ)
  let quartzX = $state(0);
  let quartzY = $state(0);

  // ディスプレイのサイズ (CSS px = ポイント) と物理ピクセル倍率
  let dispW = $state(0);
  let dispH = $state(0);
  let dpr = $state(1);

  // 選択矩形 (CSS px / ポイント)。l,t,r,b で保持
  let l = $state(0);
  let t = $state(0);
  let r = $state(0);
  let b = $state(0);
  let hasSelection = $state(false);

  let rectX = $derived(Math.min(l, r));
  let rectY = $derived(Math.min(t, b));
  let rectW = $derived(Math.abs(r - l));
  let rectH = $derived(Math.abs(b - t));

  // ドラッグ状態
  type DragKind =
    | null
    | { type: "create" }
    | { type: "move"; offX: number; offY: number }
    | { type: "resize"; edges: { left: boolean; top: boolean; right: boolean; bottom: boolean } };
  let drag = $state<DragKind>(null);

  let countdown = $state<number | null>(null);

  // ウィンドウスナップモード
  interface CaptureWindow {
    x: number;
    y: number;
    width: number;
    height: number;
    app: string;
    title: string;
  }
  let windowMode = $state(false);
  let captureWindows = $state<CaptureWindow[]>([]);
  let hoverWin = $state<CaptureWindow | null>(null);

  // カーソルがこのディスプレイ上にあるか (モード選択パネルはカーソルのある画面のみ表示)
  let cursorInside = $state(false);

  // グローバル Quartz 座標 → このディスプレイのローカル座標 (表示範囲にクランプ)
  function toLocalRect(w: CaptureWindow) {
    const x1 = clamp(w.x - quartzX, 0, dispW);
    const y1 = clamp(w.y - quartzY, 0, dispH);
    const x2 = clamp(w.x - quartzX + w.width, 0, dispW);
    const y2 = clamp(w.y - quartzY + w.height, 0, dispH);
    return { x: x1, y: y1, w: x2 - x1, h: y2 - y1 };
  }

  async function enterWindowMode() {
    try {
      captureWindows = await invoke<CaptureWindow[]>("list_capture_windows");
    } catch (e) {
      console.error("Failed to list windows:", e);
      captureWindows = [];
    }
    hoverWin = null;
    hasSelection = false;
    windowMode = true;
  }

  // カーソル位置にある最前面のウィンドウ (リストは前面→背面順)
  function windowAtPoint(px: number, py: number): CaptureWindow | null {
    for (const w of captureWindows) {
      const r = toLocalRect(w);
      if (r.w < 8 || r.h < 8) continue; // このディスプレイにほぼ無いウィンドウ
      if (px >= r.x && px < r.x + r.w && py >= r.y && py < r.y + r.h) return w;
    }
    return null;
  }

  function snapToWindow(w: CaptureWindow) {
    const rect = toLocalRect(w);
    l = rect.x;
    t = rect.y;
    r = rect.x + rect.w;
    b = rect.y + rect.h;
    hasSelection = true;
    windowMode = false;
    hoverWin = null;
    invoke("broadcast_region_selecting", { origin: myLabel }).catch(() => {});
  }

  const MIN_SIZE = 16;

  // 8 ハンドル: 動かす辺を示す
  const HANDLES = [
    { id: "nw", left: true, top: true, right: false, bottom: false, cursor: "nwse-resize" },
    { id: "n", left: false, top: true, right: false, bottom: false, cursor: "ns-resize" },
    { id: "ne", left: false, top: true, right: true, bottom: false, cursor: "nesw-resize" },
    { id: "e", left: false, top: false, right: true, bottom: false, cursor: "ew-resize" },
    { id: "se", left: false, top: false, right: true, bottom: true, cursor: "nwse-resize" },
    { id: "s", left: false, top: false, right: false, bottom: true, cursor: "ns-resize" },
    { id: "sw", left: true, top: false, right: false, bottom: true, cursor: "nesw-resize" },
    { id: "w", left: true, top: false, right: false, bottom: false, cursor: "ew-resize" },
  ];

  function clamp(v: number, lo: number, hi: number) {
    return Math.min(Math.max(v, lo), hi);
  }

  function normalize() {
    // l<r, t<b を保証
    if (l > r) [l, r] = [r, l];
    if (t > b) [t, b] = [b, t];
  }

  // カーソル所在の追跡 + ウィンドウモード中のホバー判定
  function onBackgroundPointerMove(e: PointerEvent) {
    cursorInside = true;
    if (!windowMode || countdown != null) return;
    hoverWin = windowAtPoint(e.clientX, e.clientY);
  }

  // --- 新規ドラッグ (空き領域から) / ウィンドウモード時はウィンドウ選択 ---
  function onBackgroundPointerDown(e: PointerEvent) {
    if (countdown != null) return;
    if (windowMode) {
      const w = windowAtPoint(e.clientX, e.clientY);
      if (w) snapToWindow(w);
      return;
    }
    const x = clamp(e.clientX, 0, dispW);
    const y = clamp(e.clientY, 0, dispH);
    l = x;
    t = y;
    r = x;
    b = y;
    hasSelection = true;
    drag = { type: "create" };
    // 他ディスプレイのオーバーレイの選択をクリアさせる (Rust 経由で確実に届ける)
    invoke("broadcast_region_selecting", { origin: myLabel }).catch(() => {});
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  // --- 矩形内ドラッグで移動 ---
  function onRectPointerDown(e: PointerEvent) {
    if (countdown != null) return;
    e.stopPropagation();
    drag = { type: "move", offX: e.clientX - rectX, offY: e.clientY - rectY };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  // --- ハンドルでリサイズ ---
  function onHandlePointerDown(e: PointerEvent, h: (typeof HANDLES)[number]) {
    if (countdown != null) return;
    e.stopPropagation();
    normalize();
    drag = {
      type: "resize",
      edges: { left: h.left, top: h.top, right: h.right, bottom: h.bottom },
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  function onMove(e: PointerEvent) {
    if (!drag) return;
    const x = clamp(e.clientX, 0, dispW);
    const y = clamp(e.clientY, 0, dispH);
    if (drag.type === "create") {
      r = x;
      b = y;
    } else if (drag.type === "move") {
      const w = rectW;
      const h = rectH;
      let nx = clamp(e.clientX - drag.offX, 0, dispW - w);
      let ny = clamp(e.clientY - drag.offY, 0, dispH - h);
      l = nx;
      t = ny;
      r = nx + w;
      b = ny + h;
    } else if (drag.type === "resize") {
      if (drag.edges.left) l = Math.min(x, r - MIN_SIZE);
      if (drag.edges.right) r = Math.max(x, l + MIN_SIZE);
      if (drag.edges.top) t = Math.min(y, b - MIN_SIZE);
      if (drag.edges.bottom) b = Math.max(y, t + MIN_SIZE);
    }
  }

  function onUp() {
    if (drag?.type === "create") {
      normalize();
      // 小さすぎる選択は破棄
      if (rectW < MIN_SIZE || rectH < MIN_SIZE) {
        hasSelection = false;
      }
    } else {
      normalize();
    }
    drag = null;
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
  }

  function selectFullscreen() {
    l = 0;
    t = 0;
    r = dispW;
    b = dispH;
    hasSelection = true;
    invoke("broadcast_region_selecting", { origin: myLabel }).catch(() => {});
  }

  async function startRecording() {
    if (!hasSelection || rectW < MIN_SIZE || rectH < MIN_SIZE) return;
    // 3-2-1 カウントダウン
    for (let n = 3; n >= 1; n--) {
      countdown = n;
      await new Promise((res) => setTimeout(res, 700));
    }
    countdown = null;
    // ローカル CSS px (= ポイント) に Quartz 原点を足してグローバル Quartz 座標へ
    await invoke("start_video_recording", {
      x: Math.round(quartzX + rectX),
      y: Math.round(quartzY + rectY),
      width: Math.round(rectW),
      height: Math.round(rectH),
    }).catch((err) => console.error("Failed to start recording:", err));
  }

  function cancel() {
    invoke("cancel_region_selection").catch(() => {});
  }

  // コントロールバーの配置 (枠の下、入らなければ上)
  let barTop = $derived(rectY + rectH + 10 + 44 > dispH ? rectY - 54 : rectY + rectH + 10);
  let barLeft = $derived(clamp(rectX + rectW / 2 - 130, 8, Math.max(8, dispW - 268)));

  onMount(() => {
    const params = new URLSearchParams(window.location.search);
    quartzX = Number(params.get("qx") ?? 0) || 0;
    quartzY = Number(params.get("qy") ?? 0) || 0;
    dispW = window.innerWidth;
    dispH = window.innerHeight;
    dpr = window.devicePixelRatio || 1;

    // 他のオーバーレイが選択を始めたら、自分の選択はクリアする。
    // Tauri のイベント配信はオーバーレイ webview に届かなかったため、
    // Rust が各 webview に eval で直接呼ぶグローバル関数として公開する
    (window as Window & { __regionClear?: (origin: string) => void }).__regionClear = (
      origin: string
    ) => {
      if (origin === myLabel) return;
      hasSelection = false;
      drag = null;
      windowMode = false;
      hoverWin = null;
    };

    function onResize() {
      dispW = window.innerWidth;
      dispH = window.innerHeight;
    }
    function onKeydown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        cancel();
      } else if (e.key === "Enter" && hasSelection && countdown == null) {
        e.preventDefault();
        startRecording();
      }
    }
    window.addEventListener("resize", onResize);
    window.addEventListener("keydown", onKeydown);
    return () => {
      window.removeEventListener("resize", onResize);
      window.removeEventListener("keydown", onKeydown);
      delete (window as Window & { __regionClear?: (origin: string) => void }).__regionClear;
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 select-none"
  class:cursor-crosshair={!hasSelection && !windowMode}
  class:cursor-pointer={windowMode}
  onpointerdown={onBackgroundPointerDown}
  onpointermove={onBackgroundPointerMove}
  onpointerenter={() => (cursorInside = true)}
  onpointerleave={() => (cursorInside = false)}
>
  <!-- 全体を薄暗くし、選択範囲だけ穴を開ける -->
  <div
    class="absolute inset-0 bg-black/35"
    style={hasSelection
      ? `clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%, 0 0, ${rectX}px ${rectY}px, ${rectX}px ${rectY + rectH}px, ${rectX + rectW}px ${rectY + rectH}px, ${rectX + rectW}px ${rectY}px, ${rectX}px ${rectY}px);`
      : ""}
  ></div>

  {#if hasSelection}
    <!-- 選択枠 (内側はドラッグで移動) -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="absolute border-2 border-red-500"
      class:cursor-move={countdown == null}
      style="left:{rectX}px;top:{rectY}px;width:{rectW}px;height:{rectH}px;"
      onpointerdown={onRectPointerDown}
    >
      <!-- サイズ表示 (出力ピクセル) -->
      <div
        class="absolute left-1 top-1 bg-black/80 text-white text-xs px-1.5 py-0.5 rounded pointer-events-none"
      >
        {Math.round(rectW * dpr)} × {Math.round(rectH * dpr)}
      </div>
    </div>

    {#if countdown == null}
      <!-- リサイズハンドル -->
      {#each HANDLES as h (h.id)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="absolute w-3 h-3 bg-white border border-red-500 rounded-sm -translate-x-1/2 -translate-y-1/2"
          style="left:{rectX + (h.right ? rectW : h.left ? 0 : rectW / 2)}px;top:{rectY + (h.bottom ? rectH : h.top ? 0 : rectH / 2)}px;cursor:{h.cursor};"
          onpointerdown={(e) => onHandlePointerDown(e, h)}
        ></div>
      {/each}

      <!-- コントロールバー -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="absolute flex items-center gap-2 bg-neutral-900/95 rounded-lg px-2 py-1.5 shadow-lg"
        style="left:{barLeft}px;top:{barTop}px;"
        onpointerdown={(e) => e.stopPropagation()}
      >
        <button class="ctl-btn" onclick={enterWindowMode} title="Snap to window">
          <i class="bi bi-window"></i>
        </button>
        <button class="ctl-btn" onclick={selectFullscreen} title="Fullscreen">
          <i class="bi bi-fullscreen"></i>
        </button>
        <button class="rec-btn" onclick={startRecording}>
          <span class="w-2.5 h-2.5 rounded-full bg-white"></span>
          Record
        </button>
        <button class="ctl-btn" onclick={cancel} title="Cancel (Esc)">
          <i class="bi bi-x-lg"></i>
        </button>
      </div>
    {/if}
  {/if}

  <!-- ウィンドウモード: ホバー中のウィンドウをハイライト -->
  {#if windowMode && hoverWin}
    {@const hr = toLocalRect(hoverWin)}
    <div
      class="absolute bg-blue-500/25 border-2 border-blue-400 rounded pointer-events-none"
      style="left:{hr.x}px;top:{hr.y}px;width:{hr.w}px;height:{hr.h}px;"
    >
      <div class="absolute left-1 top-1 bg-black/80 text-white text-xs px-1.5 py-0.5 rounded max-w-full truncate">
        {hoverWin.app}{hoverWin.title ? ` — ${hoverWin.title}` : ""}
      </div>
    </div>
  {/if}

  <!-- カウントダウン -->
  {#if countdown != null}
    <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
      <div class="text-white text-[120px] font-bold drop-shadow-lg">{countdown}</div>
    </div>
  {/if}

  <!-- 初期ヒント / モード選択パネル (カーソルのあるディスプレイのみ) -->
  {#if !hasSelection && countdown == null && cursorInside}
    <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="flex flex-col items-center gap-3 bg-black/70 text-white px-5 py-4 rounded-xl pointer-events-auto"
        onpointerdown={(e) => e.stopPropagation()}
      >
        <div class="text-sm">
          {windowMode
            ? "録画するウィンドウをクリック"
            : "ドラッグして録画範囲を選択"} ・ Esc でキャンセル
        </div>
        <div class="flex items-center gap-2">
          <button class="hint-btn" class:active={windowMode} onclick={enterWindowMode}>
            <i class="bi bi-window"></i> ウィンドウ
          </button>
          <button class="hint-btn" onclick={selectFullscreen}>
            <i class="bi bi-fullscreen"></i> 全画面
          </button>
          <button class="hint-btn" onclick={cancel}>
            <i class="bi bi-x-lg"></i> キャンセル
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  @reference "../../app.css";

  :global(html),
  :global(body) {
    background: transparent !important;
  }

  .ctl-btn {
    @apply flex items-center justify-center w-8 h-8 rounded-md bg-neutral-700
      text-neutral-200 cursor-pointer border-none text-sm transition-colors;
  }
  .ctl-btn:hover {
    @apply bg-neutral-600 text-white;
  }

  .rec-btn {
    @apply flex items-center gap-1.5 px-3 h-8 rounded-md bg-red-600 text-white
      text-sm cursor-pointer border-none transition-colors;
  }
  .rec-btn:hover {
    @apply bg-red-500;
  }

  .hint-btn {
    @apply flex items-center gap-1.5 px-3 h-8 rounded-md bg-neutral-700
      text-neutral-200 text-xs cursor-pointer border-none transition-colors;
  }
  .hint-btn:hover {
    @apply bg-neutral-600 text-white;
  }
  .hint-btn.active {
    @apply bg-blue-600 text-white;
  }
</style>
