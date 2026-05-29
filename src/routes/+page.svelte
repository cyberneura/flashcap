<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { load } from "@tauri-apps/plugin-store";
  import { writeText, writeImage } from "@tauri-apps/plugin-clipboard-manager";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { startDrag } from "@crabnebula/tauri-plugin-drag";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import ArrowOverlay from "$lib/ArrowOverlay.svelte";
  import MaskOverlay from "$lib/MaskOverlay.svelte";
  import ShapeOverlay from "$lib/ShapeOverlay.svelte";
  import TextOverlay from "$lib/TextOverlay.svelte";
  import OcrSelectionOverlay from "$lib/OcrSelectionOverlay.svelte";
  import Toolbar from "$lib/Toolbar.svelte";

  let arrowOverlayRef = $state<ReturnType<typeof ArrowOverlay> | null>(null);
  let maskOverlayRef = $state<ReturnType<typeof MaskOverlay> | null>(null);
  let shapeOverlayRef = $state<ReturnType<typeof ShapeOverlay> | null>(null);
  let textOverlayRef = $state<ReturnType<typeof TextOverlay> | null>(null);
  import type { Arrow, ArrowSettings, MaskRect, MaskSettings, Shape, ShapeSettings, TextAnnotation, TextSettings } from "$lib/types";

  interface ScreenshotResult {
    width: number;
    height: number;
    data: string;
    file_path: string;
  }

  const SUPPORTED_IMAGE_EXTENSIONS = ["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif", "heic", "heif"];

  let isCapturing = $state(false);
  let timerDelay = $state(5);
  let imageUrl = $state<string | null>(null);
  let imageBase64 = $state<string | null>(null);
  let filePath = $state<string | null>(null);
  let copyPathSuccess = $state(false);
  let copyImageSuccess = $state(false);
  let ocrSuccessButton = $state<"full" | "region" | "capture" | null>(null);
  let ocrSelectionActive = $state(false);
  let highlightCapture = $state(false);

  // Arrow tool state
  let arrowToolActive = $state(false);
  let arrows = $state<Arrow[]>([]);
  const ARROW_SETTINGS_KEY = "flashcap-arrow-settings";
  let arrowSettings = $state<ArrowSettings>({
    color: "#FF0000",
    thickness: 4,
    whiteStroke: true,
    dropShadow: true,
  });

  // Mask tool state
  let maskToolActive = $state(false);
  let masks = $state<MaskRect[]>([]);
  const MASK_SETTINGS_KEY = "flashcap-mask-settings";
  let maskSettings = $state<MaskSettings>({
    mode: "mosaic",
    color: "#000000",
    blurRadius: 5,
    mosaicBlockSize: 7,
  });

  // Shape tool state (rect / ellipse)
  let shapeToolActive = $state(false);
  let shapes = $state<Shape[]>([]);
  const SHAPE_SETTINGS_KEY = "flashcap-shape-settings";
  let shapeSettings = $state<ShapeSettings>({
    type: "rect",
    color: "#FF0000",
    thickness: 4,
    whiteStroke: true,
    dropShadow: true,
  });

  // Text tool state
  let textToolActive = $state(false);
  let textAnnotations = $state<TextAnnotation[]>([]);
  const TEXT_SETTINGS_KEY = "flashcap-text-settings";
  let textSettings = $state<TextSettings>({
    fontSize: 24,
    color: "#FF0000",
    bold: true,
    italic: false,
    whiteStroke: true,
    dropShadow: true,
  });

  // Undo history
  let undoHistory = $state<{ arrows: Arrow[]; masks: MaskRect[]; shapes: Shape[]; texts: TextAnnotation[] }[]>([]);

  function pushUndo() {
    undoHistory.push({
      arrows: structuredClone($state.snapshot(arrows)),
      masks: structuredClone($state.snapshot(masks)),
      shapes: structuredClone($state.snapshot(shapes)),
      texts: structuredClone($state.snapshot(textAnnotations)),
    });
  }

  function undo() {
    const entry = undoHistory.pop();
    if (!entry) return;
    arrows = entry.arrows;
    masks = entry.masks;
    shapes = entry.shapes;
    textAnnotations = entry.texts;
  }

  // Image element reference for composite rendering
  let imgEl = $state<HTMLImageElement | null>(null);

  // Natural image dimensions and display scale
  let naturalWidth = $state(0);
  let naturalHeight = $state(0);
  let viewportEl = $state<HTMLDivElement | null>(null);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);

  let noToolActive = $derived(!arrowToolActive && !maskToolActive && !shapeToolActive && !textToolActive && !ocrSelectionActive);

  // CSS scale to fit the natural-size wrapper into the viewport
  let displayScale = $derived(
    naturalWidth > 0 && naturalHeight > 0 && viewportWidth > 0 && viewportHeight > 0
      ? Math.min(viewportWidth / naturalWidth, viewportHeight / naturalHeight, 1)
      : 1
  );

  function onImageLoad() {
    if (imgEl) {
      naturalWidth = imgEl.naturalWidth;
      naturalHeight = imgEl.naturalHeight;
    }
  }

  function updateViewportSize() {
    if (viewportEl) {
      viewportWidth = viewportEl.clientWidth;
      viewportHeight = viewportEl.clientHeight;
    }
  }

  onMount(() => {
    // タイマー設定を読み込み
    load("settings.json").then(async (settingsStore) => {
      const applyStoreSettings = async () => {
        const savedTimer = await settingsStore.get<number>("timer_delay");
        if (savedTimer != null) timerDelay = savedTimer;
        const savedBlur = await settingsStore.get<number>("blur_radius");
        if (savedBlur != null) maskSettings.blurRadius = savedBlur;
        const savedMosaic = await settingsStore.get<number>("mosaic_block_size");
        if (savedMosaic != null) maskSettings.mosaicBlockSize = savedMosaic;
      };
      await applyStoreSettings();
      // Preferences ウィンドウでの変更を即時反映
      settingsStore.onChange(async (key) => {
        if (["timer_delay", "blur_radius", "mosaic_block_size"].includes(key)) {
          await applyStoreSettings();
        }
      });
    });

    // Restore arrow settings from localStorage
    const saved = localStorage.getItem(ARROW_SETTINGS_KEY);
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        arrowSettings.color = parsed.color ?? arrowSettings.color;
        arrowSettings.thickness = parsed.thickness ?? arrowSettings.thickness;
        arrowSettings.whiteStroke = parsed.whiteStroke ?? arrowSettings.whiteStroke;
        arrowSettings.dropShadow = parsed.dropShadow ?? arrowSettings.dropShadow;
      } catch { /* ignore invalid JSON */ }
    }

    const savedMask = localStorage.getItem(MASK_SETTINGS_KEY);
    if (savedMask) {
      try {
        const parsed = JSON.parse(savedMask);
        maskSettings.mode = parsed.mode ?? maskSettings.mode;
        maskSettings.color = parsed.color ?? maskSettings.color;
      } catch { /* ignore invalid JSON */ }
    }

    const savedShape = localStorage.getItem(SHAPE_SETTINGS_KEY);
    if (savedShape) {
      try {
        const parsed = JSON.parse(savedShape);
        shapeSettings.type = parsed.type ?? shapeSettings.type;
        shapeSettings.color = parsed.color ?? shapeSettings.color;
        shapeSettings.thickness = parsed.thickness ?? shapeSettings.thickness;
        shapeSettings.whiteStroke = parsed.whiteStroke ?? shapeSettings.whiteStroke;
        shapeSettings.dropShadow = parsed.dropShadow ?? shapeSettings.dropShadow;
      } catch { /* ignore invalid JSON */ }
    }

    const savedText = localStorage.getItem(TEXT_SETTINGS_KEY);
    if (savedText) {
      try {
        const parsed = JSON.parse(savedText);
        textSettings.fontSize = parsed.fontSize ?? textSettings.fontSize;
        textSettings.color = parsed.color ?? textSettings.color;
        textSettings.bold = parsed.bold ?? textSettings.bold;
        textSettings.italic = parsed.italic ?? textSettings.italic;
        textSettings.whiteStroke = parsed.whiteStroke ?? textSettings.whiteStroke;
        textSettings.dropShadow = parsed.dropShadow ?? textSettings.dropShadow;
      } catch { /* ignore invalid JSON */ }
    }

    captureScreen();
    updateViewportSize();

    const resizeObserver = new ResizeObserver(() => updateViewportSize());
    if (viewportEl) resizeObserver.observe(viewportEl);

    // アプリ再アクティブ時にキャプチャーボタンを点滅して目立たせる
    const unlisten = listen("reactivate", () => {
      highlightCapture = true;
      // animationend が発火しない環境(prefers-reduced-motion等)へのフォールバック
      setTimeout(() => { highlightCapture = false; }, 1500);
    });

    // ファイル関連付けや Dock ドロップで開かれた場合
    const unlistenOpenFile = listen<string[]>("open-file", (event) => {
      if (event.payload.length > 0) {
        loadImageFile(event.payload[0]);
      }
    });

    // ウインドウへのファイルドロップ
    const unlistenDragDrop = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "drop" && event.payload.paths.length > 0) {
        const path = event.payload.paths[0];
        const ext = path.split(".").pop()?.toLowerCase() ?? "";
        if (SUPPORTED_IMAGE_EXTENSIONS.includes(ext)) {
          loadImageFile(path);
        }
      }
    });

    function handleKeydown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        if (ocrSelectionActive) {
          ocrSelectionActive = false;
          return;
        }
        getCurrentWindow().close();
      } else if (e.metaKey && e.shiftKey && e.key === "c") {
        e.preventDefault();
        copyImage();
      } else if (e.metaKey && e.key === "c") {
        e.preventDefault();
        copyPath();
      } else if (e.metaKey && e.key === "z") {
        e.preventDefault();
        undo();
      }
    }
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("keydown", handleKeydown);
      unlisten.then((fn) => fn());
      unlistenOpenFile.then((fn) => fn());
      unlistenDragDrop.then((fn) => fn());
      resizeObserver.disconnect();
    };
  });

  // Persist arrow settings to localStorage on change
  $effect(() => {
    const { color, thickness, whiteStroke, dropShadow } = arrowSettings;
    localStorage.setItem(
      ARROW_SETTINGS_KEY,
      JSON.stringify({ color, thickness, whiteStroke, dropShadow })
    );
  });

  $effect(() => {
    const { mode, color } = maskSettings;
    localStorage.setItem(MASK_SETTINGS_KEY, JSON.stringify({ mode, color }));
  });

  $effect(() => {
    const { type, color, thickness, whiteStroke, dropShadow } = shapeSettings;
    localStorage.setItem(SHAPE_SETTINGS_KEY, JSON.stringify({ type, color, thickness, whiteStroke, dropShadow }));
  });

  $effect(() => {
    const { fontSize, color, bold, italic, whiteStroke, dropShadow } = textSettings;
    localStorage.setItem(TEXT_SETTINGS_KEY, JSON.stringify({ fontSize, color, bold, italic, whiteStroke, dropShadow }));
  });

  // テキスト属性変更時、編集中/選択中のテキストにも反映する
  function updateTextSetting<K extends keyof TextSettings>(key: K, value: TextSettings[K]) {
    textSettings[key] = value;
    textOverlayRef?.updateActiveAttribute(key, value);
  }

  async function loadImageFile(path: string) {
    try {
      const result = await invoke<ScreenshotResult>("load_image_file", { path });
      imageBase64 = result.data;
      imageUrl = `data:image/png;base64,${result.data}`;
      filePath = result.file_path;
      arrows = [];
      masks = [];
      shapes = [];
      textAnnotations = [];
      undoHistory = [];
      naturalWidth = 0;
      naturalHeight = 0;
    } catch (e) {
      console.error("Failed to load image:", e);
    }
  }

  async function captureScreen(command: string = "take_screenshot_interactive") {
    isCapturing = true;
    // キャプチャ完了までウィンドウを非表示にする
    const appWindow = getCurrentWindow();
    await appWindow.hide();
    try {
      const result = await invoke<ScreenshotResult>(command);
      imageBase64 = result.data;
      imageUrl = `data:image/png;base64,${result.data}`;
      filePath = result.file_path;
      arrows = [];
      masks = [];
      shapes = [];
      textAnnotations = [];
      undoHistory = [];
      naturalWidth = 0;
      naturalHeight = 0;
    } catch (e) {
      const errorStr = String(e);
      if (!errorStr.includes("cancelled")) {
        console.error("Capture failed:", e);
      }
    } finally {
      isCapturing = false;
      // キャプチャ完了・キャンセル後にウィンドウを再表示する
      await appWindow.show();
    }
  }

  async function copyPath() {
    if (!filePath) return;
    await saveCompositeToFile();
    await writeText(filePath);
    copyPathSuccess = true;
    setTimeout(() => (copyPathSuccess = false), 3000);
  }

  /** Box blur を1回適用（水平→垂直の分離フィルタ） */
  function boxBlurPass(data: Uint8ClampedArray, w: number, h: number, radius: number) {
    const size = radius * 2 + 1;
    const inv = 1 / size;
    const tmp = new Uint8ClampedArray(data.length);

    // 水平パス
    for (let y = 0; y < h; y++) {
      let ri = y * w * 4;
      let sumR = 0, sumG = 0, sumB = 0, sumA = 0;
      // 初期ウィンドウ: [-radius, radius]
      for (let x = -radius; x <= radius; x++) {
        const idx = (y * w + Math.min(Math.max(x, 0), w - 1)) * 4;
        sumR += data[idx]; sumG += data[idx + 1]; sumB += data[idx + 2]; sumA += data[idx + 3];
      }
      for (let x = 0; x < w; x++) {
        tmp[ri] = (sumR * inv + 0.5) | 0;
        tmp[ri + 1] = (sumG * inv + 0.5) | 0;
        tmp[ri + 2] = (sumB * inv + 0.5) | 0;
        tmp[ri + 3] = (sumA * inv + 0.5) | 0;
        ri += 4;
        // スライディングウィンドウ: 右端を追加、左端を除去
        const addIdx = (y * w + Math.min(x + radius + 1, w - 1)) * 4;
        const remIdx = (y * w + Math.max(x - radius, 0)) * 4;
        sumR += data[addIdx] - data[remIdx];
        sumG += data[addIdx + 1] - data[remIdx + 1];
        sumB += data[addIdx + 2] - data[remIdx + 2];
        sumA += data[addIdx + 3] - data[remIdx + 3];
      }
    }

    // 垂直パス
    for (let x = 0; x < w; x++) {
      let sumR = 0, sumG = 0, sumB = 0, sumA = 0;
      for (let y = -radius; y <= radius; y++) {
        const idx = (Math.min(Math.max(y, 0), h - 1) * w + x) * 4;
        sumR += tmp[idx]; sumG += tmp[idx + 1]; sumB += tmp[idx + 2]; sumA += tmp[idx + 3];
      }
      for (let y = 0; y < h; y++) {
        const wi = (y * w + x) * 4;
        data[wi] = (sumR * inv + 0.5) | 0;
        data[wi + 1] = (sumG * inv + 0.5) | 0;
        data[wi + 2] = (sumB * inv + 0.5) | 0;
        data[wi + 3] = (sumA * inv + 0.5) | 0;
        const addIdx = (Math.min(y + radius + 1, h - 1) * w + x) * 4;
        const remIdx = (Math.max(y - radius, 0) * w + x) * 4;
        sumR += tmp[addIdx] - tmp[remIdx];
        sumG += tmp[addIdx + 1] - tmp[remIdx + 1];
        sumB += tmp[addIdx + 2] - tmp[remIdx + 2];
        sumA += tmp[addIdx + 3] - tmp[remIdx + 3];
      }
    }
  }

  /** Box blur を複数回適用して Gaussian blur を近似 */
  function boxBlur(imageData: ImageData, radius: number, passes: number = 3) {
    for (let i = 0; i < passes; i++) {
      boxBlurPass(imageData.data, imageData.width, imageData.height, radius);
    }
  }

  // Render arrows onto a canvas and return PNG bytes
  function renderComposite(): Promise<Uint8Array> {
    return new Promise((resolve) => {
      const img = new Image();
      img.onload = () => {
        const canvas = document.createElement("canvas");
        canvas.width = img.naturalWidth;
        canvas.height = img.naturalHeight;
        const ctx = canvas.getContext("2d")!;
        ctx.drawImage(img, 0, 0);

        // Coords are already in natural pixels, no scale conversion needed

        // 影付き描画用の共有オフスクリーンキャンバス
        // 各描画を1レイヤーにまとめることで、影のシームや二重影を防ぐ
        let offCanvas: HTMLCanvasElement | null = null;
        let offCtx: CanvasRenderingContext2D | null = null;
        const needsOffscreen =
          arrows.some((a) => a.dropShadow) ||
          shapes.some((s) => s.dropShadow) ||
          textAnnotations.some((t) => t.dropShadow && t.whiteStroke);
        if (needsOffscreen) {
          offCanvas = document.createElement("canvas");
          offCanvas.width = canvas.width;
          offCanvas.height = canvas.height;
          offCtx = offCanvas.getContext("2d")!;
        }

        // オフスクリーンに描画 → 影付きで main canvas に転写するヘルパー
        function transferWithShadow(
          color: string, blur: number, dx: number, dy: number,
        ) {
          ctx.shadowColor = color;
          ctx.shadowBlur = blur;
          ctx.shadowOffsetX = dx;
          ctx.shadowOffsetY = dy;
          ctx.drawImage(offCanvas!, 0, 0);
          ctx.shadowColor = "transparent";
          ctx.shadowBlur = 0;
          ctx.shadowOffsetX = 0;
          ctx.shadowOffsetY = 0;
        }

        for (const arrow of arrows) {
          const sx = arrow.startX;
          const sy = arrow.startY;
          const ex = arrow.endX;
          const ey = arrow.endY;
          const t = arrow.thickness;
          const hs = t * 4;

          const dx = sx - ex;
          const dy = sy - ey;
          const len = Math.sqrt(dx * dx + dy * dy);
          if (len === 0) continue;

          const ux = dx / len;
          const uy = dy / len;

          // 矢印頭の50%まで入り込ませてシームを防ぐ
          const lsX = sx - ux * hs * 0.5;
          const lsY = sy - uy * hs * 0.5;

          const perpX = -uy;
          const perpY = ux;
          const halfW = hs * 0.4;
          const bX = sx - ux * hs;
          const bY = sy - uy * hs;

          // 描画ヘルパー: 白枠を指定コンテキストに描画
          function drawWhiteStroke(c: CanvasRenderingContext2D) {
            c.strokeStyle = "white";
            c.lineWidth = t + 4;
            c.lineCap = "round";
            c.beginPath();
            c.moveTo(lsX, lsY);
            c.lineTo(ex, ey);
            c.stroke();

            c.fillStyle = "white";
            c.lineJoin = "round";
            c.beginPath();
            c.moveTo(sx, sy);
            c.lineTo(bX + perpX * halfW, bY + perpY * halfW);
            c.lineTo(bX - perpX * halfW, bY - perpY * halfW);
            c.closePath();
            c.fill();
            c.lineWidth = 4;
            c.stroke();
          }

          // 描画ヘルパー: 矢印本体を指定コンテキストに描画
          function drawArrowBody(c: CanvasRenderingContext2D) {
            c.strokeStyle = arrow.color;
            c.lineWidth = t;
            c.lineCap = "round";
            c.beginPath();
            c.moveTo(lsX, lsY);
            c.lineTo(ex, ey);
            c.stroke();

            c.fillStyle = arrow.color;
            c.beginPath();
            c.moveTo(sx, sy);
            c.lineTo(bX + perpX * halfW, bY + perpY * halfW);
            c.lineTo(bX - perpX * halfW, bY - perpY * halfW);
            c.closePath();
            c.fill();
          }

          if (arrow.dropShadow && offCtx) {
            offCtx.clearRect(0, 0, offCanvas!.width, offCanvas!.height);
            if (arrow.whiteStroke) {
              drawWhiteStroke(offCtx);
              transferWithShadow("rgba(0,0,0,0.5)", 4, 2, 2);
              drawArrowBody(ctx);
            } else {
              drawArrowBody(offCtx);
              transferWithShadow("rgba(0,0,0,0.5)", 4, 2, 2);
            }
          } else {
            if (arrow.whiteStroke) drawWhiteStroke(ctx);
            drawArrowBody(ctx);
          }
        }

        for (const mask of masks) {
          const mx = Math.round(mask.x);
          const my = Math.round(mask.y);
          const mw = Math.round(mask.width);
          const mh = Math.round(mask.height);
          if (mw <= 0 || mh <= 0) continue;

          if (mask.mode === "fill") {
            ctx.fillStyle = mask.color;
            ctx.fillRect(mx, my, mw, mh);
          } else if (mask.mode === "blur") {
            // WebKit (Tauri WKWebView) は ctx.filter 非対応のため、
            // box blur 3回適用で Gaussian blur を近似
            const regionData = ctx.getImageData(mx, my, mw, mh);
            boxBlur(regionData, maskSettings.blurRadius, 3);
            ctx.putImageData(regionData, mx, my);
          } else if (mask.mode === "mosaic") {
            // Pixelate: scale down then scale up
            const blockSize = maskSettings.mosaicBlockSize;
            const regionData = ctx.getImageData(mx, my, mw, mh);
            const small = document.createElement("canvas");
            const sw = Math.max(1, Math.ceil(mw / blockSize));
            const sh = Math.max(1, Math.ceil(mh / blockSize));
            small.width = sw;
            small.height = sh;
            const sCtx = small.getContext("2d")!;
            // Draw original at small size
            const tmpCanvas = document.createElement("canvas");
            tmpCanvas.width = mw;
            tmpCanvas.height = mh;
            const tmpCtx = tmpCanvas.getContext("2d")!;
            tmpCtx.putImageData(regionData, 0, 0);
            sCtx.drawImage(tmpCanvas, 0, 0, sw, sh);
            // Scale back up with nearest-neighbor
            ctx.imageSmoothingEnabled = false;
            ctx.drawImage(small, 0, 0, sw, sh, mx, my, mw, mh);
            ctx.imageSmoothingEnabled = true;
          }
        }

        // Shapes (rect / ellipse)
        for (const shape of shapes) {
          function drawShapeWhiteStroke(c: CanvasRenderingContext2D) {
            if (shape.type === "rect") {
              c.strokeStyle = "white";
              c.lineWidth = shape.thickness + 4;
              c.lineJoin = "round";
              c.strokeRect(shape.x, shape.y, shape.width, shape.height);
            } else {
              const cx = shape.x + shape.width / 2;
              const cy = shape.y + shape.height / 2;
              c.strokeStyle = "white";
              c.lineWidth = shape.thickness + 4;
              c.beginPath();
              c.ellipse(cx, cy, shape.width / 2, shape.height / 2, 0, 0, Math.PI * 2);
              c.stroke();
            }
          }

          function drawShapeBody(c: CanvasRenderingContext2D) {
            if (shape.type === "rect") {
              c.strokeStyle = shape.color;
              c.lineWidth = shape.thickness;
              c.lineJoin = "round";
              c.strokeRect(shape.x, shape.y, shape.width, shape.height);
            } else {
              const cx = shape.x + shape.width / 2;
              const cy = shape.y + shape.height / 2;
              c.strokeStyle = shape.color;
              c.lineWidth = shape.thickness;
              c.beginPath();
              c.ellipse(cx, cy, shape.width / 2, shape.height / 2, 0, 0, Math.PI * 2);
              c.stroke();
            }
          }

          if (shape.dropShadow && offCtx) {
            offCtx.clearRect(0, 0, offCanvas!.width, offCanvas!.height);
            if (shape.whiteStroke) {
              drawShapeWhiteStroke(offCtx);
              transferWithShadow("rgba(0,0,0,0.5)", 4, 2, 2);
              drawShapeBody(ctx);
            } else {
              drawShapeBody(offCtx);
              transferWithShadow("rgba(0,0,0,0.5)", 4, 2, 2);
            }
          } else {
            if (shape.whiteStroke) drawShapeWhiteStroke(ctx);
            drawShapeBody(ctx);
          }
        }

        // Text annotations
        for (const t of textAnnotations) {
          if (!t.text) continue;
          const fontStyle = t.italic ? "italic" : "";
          const fontWeight = t.bold ? "900" : "normal";
          const fontStr = `${fontStyle} ${fontWeight} ${t.fontSize}px -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif`.trim();
          const lineHeight = t.fontSize * 1.3;
          const lines = t.text.split("\n");

          if (t.dropShadow && t.whiteStroke && offCtx) {
            // 白枠をオフスクリーンに描画 → 影付きで転写
            offCtx.clearRect(0, 0, offCanvas!.width, offCanvas!.height);
            offCtx.font = fontStr;
            offCtx.textBaseline = "top";
            offCtx.strokeStyle = "white";
            offCtx.lineWidth = 3;
            offCtx.lineJoin = "round";
            for (let i = 0; i < lines.length; i++) {
              offCtx.strokeText(lines[i], t.x + 4, t.y + i * lineHeight);
            }
            transferWithShadow("rgba(0,0,0,0.6)", 3, 1, 1);
            // 本体は影なしで直接描画
            ctx.font = fontStr;
            ctx.textBaseline = "top";
            ctx.fillStyle = t.color;
            for (let i = 0; i < lines.length; i++) {
              ctx.fillText(lines[i], t.x + 4, t.y + i * lineHeight);
            }
          } else {
            ctx.save();
            if (t.dropShadow) {
              ctx.shadowColor = "rgba(0,0,0,0.6)";
              ctx.shadowBlur = 3;
              ctx.shadowOffsetX = 1;
              ctx.shadowOffsetY = 1;
            }
            ctx.font = fontStr;
            ctx.textBaseline = "top";
            for (let i = 0; i < lines.length; i++) {
              const ly = t.y + i * lineHeight;
              if (t.whiteStroke) {
                ctx.strokeStyle = "white";
                ctx.lineWidth = 3;
                ctx.lineJoin = "round";
                ctx.strokeText(lines[i], t.x + 4, ly);
              }
              ctx.fillStyle = t.color;
              ctx.fillText(lines[i], t.x + 4, ly);
            }
            ctx.restore();
          }
        }

        canvas.toBlob((blob) => {
          blob!.arrayBuffer().then((buf) => resolve(new Uint8Array(buf)));
        }, "image/png");
      };
      img.src = `data:image/png;base64,${imageBase64}`;
    });
  }

  function uint8ToBase64(bytes: Uint8Array): string {
    const chunks: string[] = [];
    for (let i = 0; i < bytes.length; i += 8192) {
      chunks.push(String.fromCharCode(...bytes.subarray(i, i + 8192)));
    }
    return btoa(chunks.join(""));
  }

  function base64ToUint8(base64: string): Uint8Array {
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  }

  // 合成画像をファイルに書き出す（矢印がある場合のみ）
  // メモリ上の元画像 (imageBase64) はそのまま保持する
  // compositeBytes が渡された場合は再レンダリングをスキップする
  async function saveCompositeToFile(compositeBytes?: Uint8Array) {
    if (!filePath || !imageBase64 || (arrows.length === 0 && masks.length === 0 && shapes.length === 0 && textAnnotations.length === 0)) return;
    const bytes = compositeBytes ?? await renderComposite();
    await invoke("write_image_to_file", {
      path: filePath,
      dataBase64: uint8ToBase64(bytes),
    });
  }

  async function copyImage() {
    if (!imageBase64) return;

    const hasAnnotations = arrows.length > 0 || masks.length > 0 || shapes.length > 0 || textAnnotations.length > 0;
    const bytes = hasAnnotations
      ? await renderComposite()
      : base64ToUint8(imageBase64);

    await saveCompositeToFile(bytes);
    await writeImage(bytes);
    copyImageSuccess = true;
    setTimeout(() => (copyImageSuccess = false), 3000);
  }

  async function openFolder() {
    try {
      if (filePath) {
        // ファイルを指定することでフォルダ内のファイル一覧が表示される
        await revealItemInDir(filePath);
      } else {
        // 未キャプチャ時は保存先フォルダを開く
        await invoke("open_save_directory");
      }
    } catch (e) {
      console.error("Failed to open folder:", e);
    }
  }

  function deactivateAllTools() {
    arrowToolActive = false;
    maskToolActive = false;
    shapeToolActive = false;
    textToolActive = false;
    ocrSelectionActive = false;
    arrowOverlayRef?.deselect();
    maskOverlayRef?.deselect();
    shapeOverlayRef?.deselect();
    textOverlayRef?.deselect();
  }

  function toggleArrowTool() {
    const wasActive = arrowToolActive;
    deactivateAllTools();
    arrowToolActive = !wasActive;
  }

  function toggleMaskTool() {
    const wasActive = maskToolActive;
    deactivateAllTools();
    maskToolActive = !wasActive;
  }

  function toggleShapeTool(type: "rect" | "ellipse") {
    const wasActive = shapeToolActive && shapeSettings.type === type;
    deactivateAllTools();
    if (!wasActive) {
      shapeToolActive = true;
      shapeSettings.type = type;
    }
  }

  function toggleTextTool() {
    const wasActive = textToolActive;
    deactivateAllTools();
    textToolActive = !wasActive;
  }

  async function ocrCopyAndNotify(text: string, button: "full" | "region" | "capture") {
    await writeText(text);
    ocrSuccessButton = button;
    setTimeout(() => (ocrSuccessButton = null), 3000);
    const charCount = [...text].length;
    invoke("show_notification", {
      title: "FlashCap",
      body: `Copied ${charCount} characters`,
    }).catch(() => {});
  }

  async function ocrFullImage() {
    if (!imageBase64) return;
    try {
      const result = await invoke<string>("ocr_image", { dataBase64: imageBase64 });
      if (result) await ocrCopyAndNotify(result, "full");
    } catch (e) {
      console.error("OCR failed:", e);
    }
  }

  function ocrRegionSelect() {
    deactivateAllTools();
    ocrSelectionActive = true;
  }

  async function onOcrRegionSelected(region: { x: number; y: number; width: number; height: number }) {
    ocrSelectionActive = false;
    if (!imageBase64) return;
    try {
      const result = await invoke<string>("ocr_image", { dataBase64: imageBase64, region });
      if (result) await ocrCopyAndNotify(result, "region");
    } catch (e) {
      console.error("OCR region failed:", e);
    }
  }

  async function ocrCaptureRegion() {
    const appWindow = getCurrentWindow();
    try {
      await appWindow.hide();
      const result = await invoke<string>("ocr_capture_region");
      if (result) await ocrCopyAndNotify(result, "capture");
    } catch (e) {
      const errorStr = String(e);
      if (!errorStr.includes("cancelled")) {
        console.error("OCR capture failed:", e);
      }
    } finally {
      await appWindow.show();
    }
  }

  async function handleDragFile() {
    if (filePath) {
      await saveCompositeToFile();
      startDrag({ item: [filePath], icon: filePath });
    }
  }
</script>

<div class="flex flex-col h-screen bg-neutral-900 text-white font-[-apple-system,BlinkMacSystemFont,'Segoe_UI',Roboto,sans-serif]">
  <Toolbar
    {arrowToolActive}
    {maskToolActive}
    {shapeToolActive}
    {textToolActive}
    {arrowSettings}
    {maskSettings}
    {shapeSettings}
    {textSettings}
    {filePath}
    {copyPathSuccess}
    {copyImageSuccess}
    {isCapturing}
    {timerDelay}
    onToggleArrowTool={toggleArrowTool}
    onToggleMaskTool={toggleMaskTool}
    onToggleShapeTool={toggleShapeTool}
    onToggleTextTool={toggleTextTool}
    onCopyPath={copyPath}
    onCopyImage={copyImage}
    onOpenFolder={openFolder}
    {ocrSuccessButton}
    {ocrSelectionActive}
    onOcrFullImage={ocrFullImage}
    onOcrRegionSelect={ocrRegionSelect}
    onOcrCaptureRegion={ocrCaptureRegion}
    onCapture={captureScreen}
    onDragFile={handleDragFile}
    onUpdateTextSetting={updateTextSetting}
    {highlightCapture}
    onHighlightEnd={() => highlightCapture = false}
  />

  <div bind:this={viewportEl} class="flex-1 flex items-center justify-center overflow-hidden p-5">
    {#if imageUrl}
      <div
        class="relative rounded shadow-[0_4px_20px_rgba(0,0,0,0.5)] overflow-hidden"
        style="width:{naturalWidth}px;height:{naturalHeight}px;transform:scale({displayScale});transform-origin:top left;margin-right:{naturalWidth * (displayScale - 1)}px;margin-bottom:{naturalHeight * (displayScale - 1)}px;"
      >
        <img
          bind:this={imgEl}
          src={imageUrl}
          alt="Screenshot"
          class="block w-full h-full select-none pointer-events-none"
          draggable="false"
          onload={onImageLoad}
        />
        <MaskOverlay
          bind:this={maskOverlayRef}
          {masks}
          settings={maskSettings}
          toolActive={maskToolActive}
          interactive={maskToolActive || (noToolActive && masks.length > 0)}
          scale={displayScale}
          onBeforeMutate={pushUndo}
          onMasksChange={(newMasks) => (masks = newMasks)}
        />
        <ShapeOverlay
          bind:this={shapeOverlayRef}
          {shapes}
          settings={shapeSettings}
          toolActive={shapeToolActive}
          interactive={shapeToolActive || (noToolActive && shapes.length > 0)}
          scale={displayScale}
          onBeforeMutate={pushUndo}
          onShapesChange={(newShapes) => (shapes = newShapes)}
        />
        <TextOverlay
          bind:this={textOverlayRef}
          texts={textAnnotations}
          settings={textSettings}
          toolActive={textToolActive}
          interactive={textToolActive || (noToolActive && textAnnotations.length > 0)}
          scale={displayScale}
          onBeforeMutate={pushUndo}
          onTextsChange={(newTexts) => (textAnnotations = newTexts)}
        />
        <ArrowOverlay
          bind:this={arrowOverlayRef}
          {arrows}
          settings={arrowSettings}
          toolActive={arrowToolActive}
          interactive={arrowToolActive || noToolActive}
          scale={displayScale}
          onBeforeMutate={pushUndo}
          onArrowsChange={(newArrows) => (arrows = newArrows)}
        />
        <OcrSelectionOverlay
          active={ocrSelectionActive}
          scale={displayScale}
          onRegionSelected={onOcrRegionSelected}
        />
      </div>
    {:else if isCapturing}
      <div class="text-neutral-500 text-sm">Capturing...</div>
    {:else}
      <div class="text-neutral-500 text-sm">No image</div>
    {/if}
  </div>
</div>

<style>
  @reference "../app.css";

  :global(body) {
    @apply m-0 p-0 overflow-hidden;
  }
</style>
