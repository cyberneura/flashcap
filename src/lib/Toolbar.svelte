<script lang="ts">
  import type {
    ArrowSettings,
    CropRect,
    MaskSettings,
    ShapeSettings,
    TextSettings,
  } from "$lib/types";
  import { ASPECT_SQUARE, ASPECT_WIDESCREEN } from "$lib/cropAspect";

  let {
    arrowToolActive,
    maskToolActive,
    shapeToolActive,
    textToolActive,
    cropToolActive,
    cropRect,
    cropSnapEnabled,
    cropAspect,
    hasImage,
    onToggleCropTool,
    onToggleCropSnap,
    onToggleCropAspect,
    onApplyCrop,
    onResetCrop,
    onCancelCrop,
    arrowSettings,
    maskSettings,
    shapeSettings,
    textSettings,
    filePath,
    copyPathSuccess,
    copyImageSuccess,
    isCapturing,
    timerDelay,
    onToggleArrowTool,
    onToggleMaskTool,
    onToggleShapeTool,
    onToggleTextTool,
    onCopyPath,
    onCopyImage,
    onOpenFolder,
    onCapture,
    onCaptureVideo,
    videoMode,
    ocrSuccessButton,
    ocrSelectionActive,
    onOcrFullImage,
    onOcrRegionSelect,
    onOcrCaptureRegion,
    onDragFile,
    onUpdateTextSetting,
    highlightCapture,
    onHighlightEnd,
  }: {
    arrowToolActive: boolean;
    maskToolActive: boolean;
    shapeToolActive: boolean;
    textToolActive: boolean;
    cropToolActive: boolean;
    cropRect: CropRect | null;
    cropSnapEnabled: boolean;
    cropAspect: number | null;
    hasImage: boolean;
    onToggleCropTool: () => void;
    onToggleCropSnap: () => void;
    onToggleCropAspect: (ratio: number) => void;
    onApplyCrop: () => void;
    onResetCrop: () => void;
    onCancelCrop: () => void;
    arrowSettings: ArrowSettings;
    maskSettings: MaskSettings;
    shapeSettings: ShapeSettings;
    textSettings: TextSettings;
    filePath: string | null;
    copyPathSuccess: boolean;
    copyImageSuccess: boolean;
    isCapturing: boolean;
    timerDelay: number;
    onToggleArrowTool: () => void;
    onToggleMaskTool: () => void;
    onToggleShapeTool: (type: "rect" | "ellipse") => void;
    onToggleTextTool: () => void;
    onCopyPath: () => void;
    onCopyImage: () => void;
    onOpenFolder: () => void;
    ocrSuccessButton: "full" | "region" | "capture" | null;
    ocrSelectionActive: boolean;
    onOcrFullImage: () => void;
    onOcrRegionSelect: () => void;
    onOcrCaptureRegion: () => void;
    onCapture: (command?: string) => void;
    onCaptureVideo: () => void;
    videoMode: boolean;
    onDragFile: () => void;
    onUpdateTextSetting: <K extends keyof TextSettings>(key: K, value: TextSettings[K]) => void;
    highlightCapture: boolean;
    onHighlightEnd: () => void;
  } = $props();
</script>

<div class="flex items-center gap-2 px-3 py-2 bg-neutral-800 border-b border-neutral-700 min-h-[40px]">
  {#if !videoMode}
  <button
    class="tool-btn"
    class:active={arrowToolActive}
    onclick={onToggleArrowTool}
    aria-label="Arrow tool"
    data-tooltip="Arrow tool"
  >
    <i class="bi bi-arrow-up-right"></i>
  </button>

  {#if arrowToolActive}
    <div class="tool-settings">
      <input
        type="color"
        class="color-picker"
        bind:value={arrowSettings.color}
        data-tooltip="Arrow color"
      />

      <select
        class="bg-neutral-700 text-neutral-300 border border-neutral-600 rounded px-1.5 py-1 text-xs cursor-pointer"
        bind:value={arrowSettings.thickness}
        data-tooltip="Arrow thickness"
      >
        <option value={2}>Thin</option>
        <option value={4}>Medium</option>
        <option value={6}>Thick</option>
        <option value={8}>Extra thick</option>
      </select>

      <button
        class="tool-btn"
        class:active={arrowSettings.whiteStroke}
        onclick={() => (arrowSettings.whiteStroke = !arrowSettings.whiteStroke)}
        aria-label="White border"
        data-tooltip="White border"
      >
        <i class="bi bi-back"></i>
      </button>

      <button
        class="tool-btn"
        class:active={arrowSettings.dropShadow}
        onclick={() => (arrowSettings.dropShadow = !arrowSettings.dropShadow)}
        aria-label="Drop shadow"
        data-tooltip="Drop shadow"
      >
        <i class="bi bi-shadows"></i>
      </button>
    </div>
  {/if}

  {#snippet shapeSettingsPanel()}
    <div class="tool-settings">
      <input
        type="color"
        class="color-picker"
        bind:value={shapeSettings.color}
        data-tooltip="Shape color"
      />

      <i class="bi bi-border-width text-neutral-400 text-xs"></i>
      <select
        class="bg-neutral-700 text-neutral-300 border border-neutral-600 rounded px-1.5 py-1 text-xs cursor-pointer"
        bind:value={shapeSettings.thickness}
        data-tooltip="Shape thickness"
      >
        <option value={2}>Thin</option>
        <option value={4}>Medium</option>
        <option value={6}>Thick</option>
        <option value={8}>Extra thick</option>
      </select>

      <button
        class="tool-btn"
        class:active={shapeSettings.whiteStroke}
        onclick={() => (shapeSettings.whiteStroke = !shapeSettings.whiteStroke)}
        aria-label="White border"
        data-tooltip="White border"
      >
        <i class="bi bi-back"></i>
      </button>

      <button
        class="tool-btn"
        class:active={shapeSettings.dropShadow}
        onclick={() => (shapeSettings.dropShadow = !shapeSettings.dropShadow)}
        aria-label="Drop shadow"
        data-tooltip="Drop shadow"
      >
        <i class="bi bi-shadows"></i>
      </button>
    </div>
  {/snippet}

  <button
    class="tool-btn"
    class:active={shapeToolActive && shapeSettings.type === "rect"}
    onclick={() => onToggleShapeTool("rect")}
    aria-label="Rectangle tool"
    data-tooltip="Rectangle tool"
  >
    <i class="bi bi-square"></i>
  </button>

  {#if shapeToolActive && shapeSettings.type === "rect"}
    {@render shapeSettingsPanel()}
  {/if}

  <button
    class="tool-btn"
    class:active={shapeToolActive && shapeSettings.type === "ellipse"}
    onclick={() => onToggleShapeTool("ellipse")}
    aria-label="Ellipse tool"
    data-tooltip="Ellipse tool"
  >
    <i class="bi bi-circle"></i>
  </button>

  {#if shapeToolActive && shapeSettings.type === "ellipse"}
    {@render shapeSettingsPanel()}
  {/if}

  <button
    class="tool-btn"
    class:active={textToolActive}
    onclick={onToggleTextTool}
    aria-label="Text tool"
    data-tooltip="Text tool"
  >
    <i class="bi bi-fonts" style="font-size: 125%"></i>
  </button>

  {#if textToolActive}
    <div class="tool-settings">
      <input
        type="color"
        class="color-picker"
        value={textSettings.color}
        oninput={(e) => onUpdateTextSetting("color", (e.target as HTMLInputElement).value)}
        data-tooltip="Text color"
      />

      <select
        class="bg-neutral-700 text-neutral-300 border border-neutral-600 rounded px-1.5 py-1 text-xs cursor-pointer"
        value={textSettings.fontSize}
        onchange={(e) => onUpdateTextSetting("fontSize", Number((e.target as HTMLSelectElement).value))}
        data-tooltip="Font size"
      >
        <option value={16}>16px</option>
        <option value={20}>20px</option>
        <option value={24}>24px</option>
        <option value={32}>32px</option>
        <option value={48}>48px</option>
      </select>

      <button
        class="tool-btn"
        class:active={textSettings.bold}
        onclick={() => onUpdateTextSetting("bold", !textSettings.bold)}
        aria-label="Bold"
        data-tooltip="Bold"
      >
        <i class="bi bi-type-bold"></i>
      </button>

      <button
        class="tool-btn"
        class:active={textSettings.italic}
        onclick={() => onUpdateTextSetting("italic", !textSettings.italic)}
        aria-label="Italic"
        data-tooltip="Italic"
      >
        <i class="bi bi-type-italic"></i>
      </button>

      <button
        class="tool-btn"
        class:active={textSettings.whiteStroke}
        onclick={() => onUpdateTextSetting("whiteStroke", !textSettings.whiteStroke)}
        aria-label="White border"
        data-tooltip="White border"
      >
        <i class="bi bi-back"></i>
      </button>

      <button
        class="tool-btn"
        class:active={textSettings.dropShadow}
        onclick={() => onUpdateTextSetting("dropShadow", !textSettings.dropShadow)}
        aria-label="Drop shadow"
        data-tooltip="Drop shadow"
      >
        <i class="bi bi-shadows"></i>
      </button>
    </div>
  {/if}

  <button
    class="tool-btn"
    class:active={maskToolActive}
    onclick={onToggleMaskTool}
    aria-label="Mask tool"
    data-tooltip="Mask tool"
  >
    <i class="bi bi-grid-3x3"></i>
  </button>

  {#if maskToolActive}
    <div class="tool-settings">
      <button
        class="tool-btn text-xs"
        class:active={maskSettings.mode === "mosaic"}
        onclick={() => (maskSettings.mode = "mosaic")}
        aria-label="Mosaic"
        data-tooltip="Mosaic"
      >▦</button>
      <button
        class="tool-btn text-xs"
        class:active={maskSettings.mode === "blur"}
        onclick={() => (maskSettings.mode = "blur")}
        aria-label="Blur"
        data-tooltip="Blur"
      >
        <i class="bi bi-droplet-half"></i>
      </button>
      <button
        class="tool-btn text-xs"
        class:active={maskSettings.mode === "fill"}
        onclick={() => (maskSettings.mode = "fill")}
        aria-label="Fill"
        data-tooltip="Fill"
      >
        <i class="bi bi-paint-bucket"></i>
      </button>

      {#if maskSettings.mode === "fill"}
        <input
          type="color"
          class="color-picker"
          bind:value={maskSettings.color}
          data-tooltip="Fill color"
        />
      {/if}
    </div>
  {/if}

  <button
    class="tool-btn"
    class:active={cropToolActive}
    onclick={onToggleCropTool}
    disabled={!hasImage}
    aria-label="Crop tool"
    data-tooltip="Crop tool"
  >
    <i class="bi bi-crop"></i>
  </button>

  {#if cropToolActive}
    <div class="tool-settings">
      <span class="text-xs text-neutral-200 tabular-nums text-center min-w-[5.5rem]">
        {cropRect ? `${Math.round(cropRect.width)} × ${Math.round(cropRect.height)}` : "—"}
      </span>
      <button
        class="crop-action crop-ratio"
        class:active={cropAspect === ASPECT_SQUARE}
        onclick={() => onToggleCropAspect(ASPECT_SQUARE)}
        aria-pressed={cropAspect === ASPECT_SQUARE}
        data-tooltip="Lock the aspect ratio to a square"
      >
        1:1
      </button>
      <button
        class="crop-action crop-ratio"
        class:active={cropAspect === ASPECT_WIDESCREEN}
        onclick={() => onToggleCropAspect(ASPECT_WIDESCREEN)}
        aria-pressed={cropAspect === ASPECT_WIDESCREEN}
        data-tooltip="Lock the aspect ratio to 16:9"
      >
        16:9
      </button>
      <button
        class="crop-action crop-snap"
        class:active={cropSnapEnabled}
        onclick={onToggleCropSnap}
        aria-pressed={cropSnapEnabled}
        aria-label="Snap to edges"
        data-tooltip="Snap to edges in the image"
      >
        <i class="bi bi-magnet"></i>
      </button>
      <button
        class="crop-action"
        onclick={onResetCrop}
        data-tooltip={cropAspect === null
          ? "Select the whole image"
          : "Select the largest area with this ratio"}
      >
        Reset
      </button>
      <button class="crop-action" onclick={onCancelCrop} data-tooltip="Cancel (Esc)">
        Cancel
      </button>
      <button class="crop-action crop-apply" onclick={onApplyCrop} data-tooltip="Crop (Enter)">
        Apply
      </button>
    </div>
  {/if}
  {/if}

  <div class="toolbar-divider"></div>

  <button
    class="tool-btn"
    onclick={onOpenFolder}
    aria-label="Open save folder"
    data-tooltip="Open save folder"
  >
    <i class="bi bi-folder2-open"></i>
  </button>

  {#if filePath}
    <button
      class="tool-btn cursor-grab active:cursor-grabbing"
      aria-label="Drag to share file"
      data-tooltip="Drag to share file"
      onmousedown={onDragFile}
    >
      <i class="bi bi-grip-vertical"></i>
    </button>
    <input
      type="text"
      class="flex-1 min-w-0 text-[13px] text-neutral-400 bg-transparent border-none outline-none"
      value={filePath}
      readonly
      title={filePath}
    />
    <button
      class="tool-btn"
      onclick={onCopyPath}
      aria-label="Copy file path"
      data-tooltip="Copy file path (⌘C)"
    >
      {#if copyPathSuccess}
        <i class="bi bi-check-lg"></i>
      {:else}
        <i class="bi bi-clipboard"></i>
      {/if}
    </button>
    <button
      class="tool-btn"
      onclick={onCopyImage}
      aria-label="Copy image"
      data-tooltip="Copy image (⌘⇧C)"
    >
      {#if copyImageSuccess}
        <i class="bi bi-check-lg"></i>
      {:else}
        <i class="bi bi-image"></i>
      {/if}
    </button>
  {/if}

  <div class="toolbar-divider"></div>

  {#if !videoMode}
  {#if filePath}
    <button
      class="tool-btn"
      onclick={onOcrFullImage}
      aria-label="Copy text (OCR)"
      data-tooltip="Copy text (OCR)"
    >
      {#if ocrSuccessButton === "full"}
        <i class="bi bi-check-lg"></i>
      {:else}
        <i class="bi bi-body-text"></i>
      {/if}
    </button>
    <button
      class="tool-btn"
      class:active={ocrSelectionActive}
      onclick={onOcrRegionSelect}
      aria-label="Select region & copy text"
      data-tooltip="Select region & copy text"
    >
      {#if ocrSuccessButton === "region"}
        <i class="bi bi-check-lg"></i>
      {:else}
        <i class="bi bi-textarea-t"></i>
      {/if}
    </button>
  {/if}
  <button
    class="tool-btn"
    onclick={onOcrCaptureRegion}
    aria-label="Capture region & copy text"
    data-tooltip="Capture region & copy text"
  >
    {#if ocrSuccessButton === "capture"}
      <i class="bi bi-check-lg"></i>
    {:else}
      <i class="bi bi-type"></i>
    {/if}
  </button>
  {/if}

  <div class="toolbar-divider"></div>

  <button
    class="tool-btn"
    onclick={() => onCapture("take_screenshot_timer")}
    disabled={isCapturing}
    aria-label="Timer capture"
    data-tooltip="Timer capture ({timerDelay}s)"
  >
    <i class="bi bi-stopwatch"></i>
  </button>
  <button
    class="tool-btn"
    class:blink={highlightCapture}
    onclick={() => onCapture()}
    onanimationend={onHighlightEnd}
    disabled={isCapturing}
    aria-label="Capture new area"
    data-tooltip="Capture new area"
  >
    <i class="bi bi-camera"></i>
  </button>
  <button
    class="tool-btn"
    onclick={onCaptureVideo}
    disabled={isCapturing}
    aria-label="Record video"
    data-tooltip="Record video"
  >
    <i class="bi bi-record-circle"></i>
  </button>
</div>

<style>
  @reference "../app.css";

  .tool-settings {
    @apply flex items-center gap-2 bg-neutral-700
      -my-2 px-2 py-2 border-l border-neutral-600;
  }

  .tool-btn {
    @apply flex items-center justify-center w-8 h-8
      border-none rounded-md bg-transparent text-neutral-300
      cursor-pointer transition-[background,color] duration-150 text-base;
  }

  .tool-btn:hover:not(:disabled) {
    @apply bg-neutral-700 text-white;
  }

  .tool-btn:disabled {
    @apply opacity-50 cursor-not-allowed;
  }

  .tool-btn.active {
    @apply bg-blue-600 text-white;
  }

  .tool-btn.active:hover:not(:disabled) {
    @apply bg-blue-500;
  }

  .tool-btn.blink {
    animation: capture-blink 1.2s ease-in-out;
  }

  @keyframes capture-blink {
    0%   { background: transparent; }
    10%  { background: #3b82f6; color: #fff; }
    23%  { background: transparent; color: #d4d4d4; }
    36%  { background: #3b82f6; color: #fff; }
    49%  { background: transparent; color: #d4d4d4; }
    62%  { background: #3b82f6; color: #fff; }
    75%  { background: transparent; color: #d4d4d4; }
    100% { background: transparent; }
  }

  .crop-action {
    @apply px-2 py-1 rounded border-none bg-neutral-600 text-neutral-100
      text-xs cursor-pointer transition-colors duration-150;
  }

  .crop-action:hover {
    @apply bg-neutral-500;
  }

  .crop-snap {
    @apply px-2 text-sm leading-none;
  }

  .crop-ratio {
    @apply px-2 tabular-nums;
  }

  .crop-snap.active,
  .crop-ratio.active {
    @apply bg-blue-600 text-white;
  }

  .crop-snap.active:hover,
  .crop-ratio.active:hover {
    @apply bg-blue-500;
  }

  .crop-apply {
    @apply bg-blue-600 text-white;
  }

  .crop-apply:hover {
    @apply bg-blue-500;
  }

  .toolbar-divider {
    @apply w-px h-6 bg-neutral-700;
  }

  /* Instant tooltip via data-tooltip attribute + ::after */
  [data-tooltip] {
    @apply relative;
  }

  [data-tooltip]::after {
    content: attr(data-tooltip);
    @apply absolute top-[calc(100%+6px)] left-1/2 -translate-x-1/2
      px-2 py-1 bg-black text-neutral-200 text-[11px] leading-tight
      rounded whitespace-nowrap pointer-events-none opacity-0
      transition-opacity duration-100 z-100;
  }

  [data-tooltip]:hover::after {
    @apply opacity-100;
  }

  .color-picker {
    @apply w-7 h-7 border-none rounded p-0 cursor-pointer bg-transparent;
  }

  .color-picker::-webkit-color-swatch-wrapper {
    @apply p-0.5;
  }

  .color-picker::-webkit-color-swatch {
    @apply border border-neutral-600 rounded-sm;
  }
</style>
