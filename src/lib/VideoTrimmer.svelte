<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type {
    VideoExportFormat,
    VideoSizePreset,
    VideoResizeMode,
  } from "$lib/types";

  let {
    videoUrl,
    videoPath,
    ffmpegAvailable,
    onExported,
    onNotify,
  }: {
    videoUrl: string;
    videoPath: string;
    ffmpegAvailable: boolean;
    onExported: (outputPath: string) => void;
    onNotify: (title: string, body: string) => void;
  } = $props();

  let videoEl = $state<HTMLVideoElement | null>(null);
  let trackEl = $state<HTMLDivElement | null>(null);

  let duration = $state(0);
  let videoWidth = $state(0);
  let videoHeight = $state(0);
  let trimStart = $state(0);
  let trimEnd = $state(0);
  let currentTime = $state(0);
  let playing = $state(false);

  // 書き出し設定
  let format = $state<VideoExportFormat>("mp4");
  let sizePreset = $state<VideoSizePreset>("original");
  let resizeMode = $state<VideoResizeMode>("fill");
  let pad = $state(true);
  let padColor = $state("#000000");
  let fps = $state(15);

  let exporting = $state(false);
  let exportError = $state<string | null>(null);
  let exportedPath = $state<string | null>(null);

  // 固定サイズ (リサイズモードの選択が必要なプリセット)
  const FIXED_SIZES: VideoSizePreset[] = ["hd", "720p", "square512"];
  let isFixedSize = $derived(FIXED_SIZES.includes(sizePreset));

  let dragging = $state<null | "start" | "end" | "seek">(null);

  function onLoadedMetadata() {
    if (!videoEl) return;
    duration = isFinite(videoEl.duration) ? videoEl.duration : 0;
    videoWidth = videoEl.videoWidth;
    videoHeight = videoEl.videoHeight;
    trimStart = 0;
    trimEnd = duration;
    currentTime = 0;
  }

  function onTimeUpdate() {
    if (!videoEl) return;
    currentTime = videoEl.currentTime;
    // トリム範囲内でループ再生
    if (playing && currentTime >= trimEnd) {
      videoEl.currentTime = trimStart;
    }
  }

  function togglePlay() {
    if (!videoEl) return;
    if (playing) {
      videoEl.pause();
    } else {
      if (currentTime < trimStart || currentTime >= trimEnd) {
        videoEl.currentTime = trimStart;
      }
      videoEl.play().catch(() => {});
    }
  }

  function pct(t: number): number {
    return duration > 0 ? (t / duration) * 100 : 0;
  }

  function timeFromClientX(clientX: number): number {
    if (!trackEl || duration <= 0) return 0;
    const rect = trackEl.getBoundingClientRect();
    const ratio = Math.min(Math.max((clientX - rect.left) / rect.width, 0), 1);
    return ratio * duration;
  }

  function seekTo(t: number) {
    if (!videoEl) return;
    videoEl.currentTime = t;
    currentTime = t;
  }

  function onDragMove(e: PointerEvent) {
    const t = timeFromClientX(e.clientX);
    if (dragging === "start") {
      trimStart = Math.max(0, Math.min(t, trimEnd - 0.05));
      seekTo(trimStart);
    } else if (dragging === "end") {
      trimEnd = Math.min(duration, Math.max(t, trimStart + 0.05));
      seekTo(trimEnd);
    } else if (dragging === "seek") {
      seekTo(t);
    }
  }

  function onDragUp() {
    dragging = null;
    window.removeEventListener("pointermove", onDragMove);
    window.removeEventListener("pointerup", onDragUp);
  }

  function startDrag(which: "start" | "end" | "seek", e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    dragging = which;
    if (which === "seek") seekTo(timeFromClientX(e.clientX));
    window.addEventListener("pointermove", onDragMove);
    window.addEventListener("pointerup", onDragUp);
  }

  function formatTime(t: number): string {
    if (!isFinite(t) || t < 0) t = 0;
    const m = Math.floor(t / 60);
    const s = t - m * 60;
    return `${m}:${s.toFixed(1).padStart(4, "0")}`;
  }

  let trimDuration = $derived(Math.max(0, trimEnd - trimStart));

  async function doExport() {
    if (!ffmpegAvailable) {
      exportError = "ffmpeg not found. Install it with `brew install ffmpeg`.";
      return;
    }
    if (trimEnd <= trimStart) {
      exportError = "Invalid trim range.";
      return;
    }
    exporting = true;
    exportError = null;
    exportedPath = null;
    try {
      const out = await invoke<string>("export_video", {
        input: videoPath,
        startSec: trimStart,
        endSec: trimEnd,
        format,
        sizePreset,
        resizeMode,
        pad,
        padColor,
        fps,
      });
      exportedPath = out;
      onExported(out);
    } catch (e) {
      exportError = String(e);
      onNotify("FlashCap", `Export failed: ${e}`);
    } finally {
      exporting = false;
    }
  }
</script>

<div class="flex flex-col gap-3 w-full h-full items-center justify-start p-2">
  <!-- プレビュー -->
  <div class="flex-1 min-h-0 w-full flex items-center justify-center">
    <!-- svelte-ignore a11y_media_has_caption -->
    <video
      bind:this={videoEl}
      src={videoUrl}
      class="max-w-full max-h-full rounded shadow-[0_4px_20px_rgba(0,0,0,0.5)] bg-black"
      onloadedmetadata={onLoadedMetadata}
      ontimeupdate={onTimeUpdate}
      onplay={() => (playing = true)}
      onpause={() => (playing = false)}
    ></video>
  </div>

  <!-- トリムタイムライン -->
  <div class="w-full max-w-3xl flex items-center gap-3">
    <button
      class="trim-btn shrink-0"
      onclick={togglePlay}
      aria-label={playing ? "Pause" : "Play"}
      data-tooltip={playing ? "Pause" : "Play"}
    >
      {#if playing}
        <i class="bi bi-pause-fill"></i>
      {:else}
        <i class="bi bi-play-fill"></i>
      {/if}
    </button>

    <div class="flex-1">
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        bind:this={trackEl}
        class="relative h-8 rounded bg-neutral-700 cursor-pointer select-none"
        onpointerdown={(e) => startDrag("seek", e)}
      >
        <!-- 選択範囲 -->
        <div
          class="absolute top-0 bottom-0 bg-blue-600/40 border-x-2 border-blue-500"
          style="left:{pct(trimStart)}%;width:{Math.max(0, pct(trimEnd) - pct(trimStart))}%;"
        ></div>
        <!-- 再生ヘッド -->
        <div
          class="absolute top-0 bottom-0 w-0.5 bg-white pointer-events-none"
          style="left:{pct(currentTime)}%;"
        ></div>
        <!-- 開始ハンドル -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="trim-handle"
          style="left:{pct(trimStart)}%;"
          onpointerdown={(e) => startDrag("start", e)}
          title="Trim start"
        ></div>
        <!-- 終了ハンドル -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="trim-handle"
          style="left:{pct(trimEnd)}%;"
          onpointerdown={(e) => startDrag("end", e)}
          title="Trim end"
        ></div>
      </div>
      <div class="flex justify-between text-[11px] text-neutral-400 mt-1 tabular-nums">
        <span>In {formatTime(trimStart)}</span>
        <span>Length {formatTime(trimDuration)}</span>
        <span>Out {formatTime(trimEnd)}</span>
      </div>
    </div>
  </div>

  <!-- 書き出しオプション -->
  <div class="w-full max-w-3xl flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-neutral-300">
    <label class="flex items-center gap-1.5">
      <span class="text-neutral-400">Format</span>
      <select class="export-select" bind:value={format}>
        <option value="mp4">MP4</option>
        <option value="gif">Animated GIF</option>
      </select>
    </label>

    <label class="flex items-center gap-1.5">
      <span class="text-neutral-400">Size</span>
      <select class="export-select" bind:value={sizePreset}>
        <option value="original">
          Original{videoWidth ? ` (${videoWidth}×${videoHeight})` : ""}
        </option>
        <option value="half">
          1/2{videoWidth ? ` (${Math.round(videoWidth / 2)}×${Math.round(videoHeight / 2)})` : ""}
        </option>
        <option value="hd">HD (1920×1080)</option>
        <option value="720p">720p (1280×720)</option>
        <option value="square512">512×512</option>
      </select>
    </label>

    {#if isFixedSize}
      <label class="flex items-center gap-1.5">
        <span class="text-neutral-400">Fit</span>
        <select class="export-select" bind:value={resizeMode}>
          <option value="fill">Fill (crop overflow)</option>
          <option value="fit">Fit (inside)</option>
        </select>
      </label>

      {#if resizeMode === "fit"}
        <label class="flex items-center gap-1.5">
          <input type="checkbox" bind:checked={pad} />
          <span>Pad</span>
        </label>
        {#if pad}
          <input
            type="color"
            class="w-7 h-7 border-none rounded p-0 cursor-pointer bg-transparent"
            bind:value={padColor}
            title="Pad color"
          />
        {/if}
      {/if}
    {/if}

    {#if format === "gif"}
      <label class="flex items-center gap-1.5">
        <span class="text-neutral-400">FPS</span>
        <select class="export-select" bind:value={fps}>
          <option value={10}>10</option>
          <option value={15}>15</option>
          <option value={24}>24</option>
        </select>
      </label>
    {/if}

    <div class="flex-1"></div>

    <button
      class="export-btn"
      onclick={doExport}
      disabled={exporting || !ffmpegAvailable || trimEnd <= trimStart}
    >
      {#if exporting}
        <i class="bi bi-arrow-repeat animate-spin"></i> Exporting…
      {:else}
        <i class="bi bi-box-arrow-down"></i> Export
      {/if}
    </button>
  </div>

  {#if !ffmpegAvailable}
    <div class="text-[11px] text-amber-400">
      ffmpeg not found. Install it with <code class="bg-neutral-800 px-1 rounded">brew install ffmpeg</code> to enable export.
    </div>
  {/if}
  {#if exportError}
    <div class="text-[11px] text-red-400 max-w-3xl whitespace-pre-wrap break-words">{exportError}</div>
  {/if}
  {#if exportedPath}
    <div class="text-[11px] text-green-400">Exported: {exportedPath}</div>
  {/if}
</div>

<style>
  @reference "../app.css";

  .trim-btn {
    @apply flex items-center justify-center w-9 h-9 border-none rounded-md
      bg-neutral-700 text-neutral-200 cursor-pointer text-lg
      transition-colors duration-150;
  }
  .trim-btn:hover {
    @apply bg-neutral-600 text-white;
  }

  .trim-handle {
    @apply absolute top-1/2 -translate-x-1/2 -translate-y-1/2 w-3 h-10
      rounded bg-blue-400 border border-white cursor-ew-resize z-10;
  }
  .trim-handle:hover {
    @apply bg-blue-300;
  }

  .export-select {
    @apply bg-neutral-700 text-neutral-200 border border-neutral-600
      rounded px-1.5 py-1 text-xs cursor-pointer;
  }

  .export-btn {
    @apply flex items-center gap-1.5 px-3 py-1.5 rounded-md
      bg-blue-600 text-white text-xs cursor-pointer border-none
      transition-colors duration-150;
  }
  .export-btn:hover:not(:disabled) {
    @apply bg-blue-500;
  }
  .export-btn:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  [data-tooltip] {
    @apply relative;
  }
  [data-tooltip]::after {
    content: attr(data-tooltip);
    @apply absolute bottom-[calc(100%+6px)] left-1/2 -translate-x-1/2
      px-2 py-1 bg-black text-neutral-200 text-[11px] leading-tight
      rounded whitespace-nowrap pointer-events-none opacity-0
      transition-opacity duration-100 z-100;
  }
  [data-tooltip]:hover::after {
    @apply opacity-100;
  }
</style>
