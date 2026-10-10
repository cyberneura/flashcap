<script lang="ts">
  // 左サイドバーのサムネイルブラウザ (CYBERNEURA-DEV-995)。
  // 保存先フォルダの画像を新しい順に並べる。一覧とサムネイルは Rust (thumbnails.rs) が作る。
  // クリックで開く / ドラッグで他のアプリへ渡すのは親 (+page.svelte) の仕事で、ここは知らせるだけ
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    formatFileSize,
    formatRelativeTime,
    thumbnailCacheKey,
    type SavedImage,
  } from "$lib/thumbnailBrowser";

  let {
    currentPath,
    refreshToken,
    onSelect,
    onDragOut,
  }: {
    /** メイン画面に出ている画像のパス (強調表示する) */
    currentPath: string | null;
    /** 変わるたびに一覧を読み直す (撮影・貼り付け・保存先の変更など) */
    refreshToken: unknown;
    onSelect: (path: string) => void;
    onDragOut: (path: string) => void;
  } = $props();

  let images = $state<SavedImage[]>([]);
  let loadError = $state<string | null>(null);
  let loaded = $state(false);
  // サムネイルの取り置き。キーは thumbnailCacheKey (書き戻されたものは作り直す)。
  // null = 作れなかった (読めない・壊れた画像)。何度も作り直しに行かない
  let thumbs = $state<Record<string, string | null>>({});
  let now = $state(Date.now());

  let listEl = $state<HTMLDivElement | null>(null);

  // 一覧の読み込みが重なった時に、古い結果で新しい結果を上書きしないための通し番号
  let listSeq = 0;

  async function reload() {
    const seq = ++listSeq;
    try {
      const result = await invoke<SavedImage[]>("list_saved_images");
      if (seq !== listSeq) return;
      images = result;
      loadError = null;
      now = Date.now();
      // 一覧から消えたもの・中身が変わったものの取り置きを捨てる
      const keep = new Set(result.map(thumbnailCacheKey));
      const next: Record<string, string | null> = {};
      for (const [key, value] of Object.entries(thumbs)) {
        if (keep.has(key)) next[key] = value;
      }
      thumbs = next;
    } catch (e) {
      if (seq !== listSeq) return;
      loadError = String(e);
    } finally {
      if (seq === listSeq) loaded = true;
    }
  }

  // refreshToken が変わるたびに読み直す (初回の表示もここ)
  $effect(() => {
    void refreshToken;
    reload();
  });

  onMount(() => {
    // 他のアプリから戻った時 (Finder で消した・別の手段で撮った等) に追従する
    const onFocus = () => reload();
    window.addEventListener("focus", onFocus);
    // 相対時刻 ("5 minutes ago") を進める
    const timer = setInterval(() => (now = Date.now()), 60 * 1000);
    return () => {
      window.removeEventListener("focus", onFocus);
      clearInterval(timer);
      // 閉じた後まで残りのサムネイルを作り続けない (作りかけの分は捨てられるだけ)
      destroyed = true;
      queue.length = 0;
    };
  });

  // --- サムネイルは見えたものから順に作る ---
  // 保存先には何百枚もありうるうえ、1 枚ごとに原寸の画像をデコードするので、
  // 一度に全部頼まず、画面に入ったものだけを少数ずつ頼む
  const MAX_CONCURRENT = 2;
  let inFlight = 0;
  let destroyed = false;
  const queue: SavedImage[] = [];
  const requested = new Set<string>();

  function requestThumbnail(image: SavedImage) {
    const key = thumbnailCacheKey(image);
    if (key in thumbs || requested.has(key)) return;
    requested.add(key);
    queue.push(image);
    pump();
  }

  function pump() {
    while (!destroyed && inFlight < MAX_CONCURRENT && queue.length > 0) {
      const image = queue.shift()!;
      const key = thumbnailCacheKey(image);
      inFlight++;
      invoke<string>("saved_image_thumbnail", { path: image.path })
        .then((url) => (thumbs[key] = url))
        .catch(() => (thumbs[key] = null))
        .finally(() => {
          requested.delete(key);
          inFlight--;
          pump();
        });
    }
  }

  /** 要素がサイドバーのスクロール範囲に入ったらサムネイルを頼む */
  function lazyThumbnail(node: HTMLElement, image: SavedImage) {
    let current = image;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) requestThumbnail(current);
      },
      // 少し先まで読んでおき、スクロールした時に空の枠が見えにくいようにする
      { root: listEl, rootMargin: "200px 0px" }
    );
    observer.observe(node);
    return {
      update(next: SavedImage) {
        current = next;
        // 同じ要素のまま中身が書き換わった (書き戻された) 時は、見えていれば作り直す
        observer.unobserve(node);
        observer.observe(node);
      },
      destroy() {
        observer.disconnect();
      },
    };
  }

  // --- クリックとドラッグの切り分け ---
  // 押した位置から DRAG_SLOP 以上動いたらドラッグ (OS のファイルドラッグを始める)、
  // 動かずに離したらクリック (メイン画面に開く)
  const DRAG_SLOP = 4;
  let press: { path: string; x: number; y: number } | null = null;
  let dragged = false;

  function handleMouseDown(e: MouseEvent, path: string) {
    if (e.button !== 0) return;
    press = { path, x: e.clientX, y: e.clientY };
    dragged = false;
  }

  function handleWindowMouseMove(e: MouseEvent) {
    if (!press || dragged || (e.buttons & 1) === 0) return;
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < DRAG_SLOP) return;
    dragged = true;
    const path = press.path;
    press = null;
    onDragOut(path);
  }

  function handleWindowMouseUp() {
    press = null;
  }

  function handleClick(path: string) {
    // ドラッグを始めた押下の click は無視する (ドラッグ先によっては click が届く)
    if (dragged) {
      dragged = false;
      return;
    }
    onSelect(path);
  }

  function handleKeydown(e: KeyboardEvent, path: string) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(path);
    }
  }
</script>

<svelte:window onmousemove={handleWindowMouseMove} onmouseup={handleWindowMouseUp} />

<aside class="w-56 shrink-0 flex flex-col bg-neutral-800/60 border-r border-neutral-700 min-h-0">
  <div class="flex items-center gap-2 px-3 py-2 border-b border-neutral-700 text-xs text-neutral-400">
    <span class="flex-1 truncate">Saved images{loaded && !loadError ? ` (${images.length})` : ""}</span>
    <button
      class="flex items-center justify-center w-6 h-6 rounded border-none bg-transparent text-neutral-400 hover:bg-neutral-700 hover:text-white cursor-pointer"
      onclick={reload}
      aria-label="Reload"
      title="Reload"
    >
      <i class="bi bi-arrow-clockwise"></i>
    </button>
  </div>

  <div bind:this={listEl} class="flex-1 overflow-y-auto p-2 flex flex-col gap-2">
    {#if loadError}
      <div class="text-xs text-red-400 break-words p-1">Could not read the save folder: {loadError}</div>
    {:else if loaded && images.length === 0}
      <div class="text-xs text-neutral-500 p-1">No images in the save folder</div>
    {/if}

    {#each images as image (image.path)}
      {@const key = thumbnailCacheKey(image)}
      {@const thumb = thumbs[key]}
      <div
        class="group rounded-md p-1.5 cursor-pointer select-none outline-none hover:bg-neutral-700/70 focus-visible:ring-2 focus-visible:ring-blue-500"
        class:selected={image.path === currentPath}
        role="button"
        tabindex="0"
        title={image.path}
        use:lazyThumbnail={image}
        onmousedown={(e) => handleMouseDown(e, image.path)}
        onclick={() => handleClick(image.path)}
        onkeydown={(e) => handleKeydown(e, image.path)}
      >
        <div class="h-28 flex items-center justify-center rounded bg-neutral-900/70 overflow-hidden">
          {#if thumb}
            <img src={thumb} alt={image.name} class="max-w-full max-h-full object-contain pointer-events-none" draggable="false" />
          {:else if thumb === null}
            <i class="bi bi-file-earmark-image text-2xl text-neutral-600"></i>
          {:else}
            <i class="bi bi-image text-2xl text-neutral-700"></i>
          {/if}
        </div>
        <div class="mt-1 text-[11px] text-neutral-200 truncate">{image.name}</div>
        <div class="text-[11px] text-neutral-500 truncate">
          {formatRelativeTime(image.created_ms, now)} · {formatFileSize(image.size)}
        </div>
      </div>
    {/each}
  </div>
</aside>

<style>
  @reference "../app.css";

  .selected {
    @apply bg-blue-600/30;
  }

  .selected:hover {
    @apply bg-blue-600/40;
  }
</style>
