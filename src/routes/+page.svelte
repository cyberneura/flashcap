<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { listen, emit } from "@tauri-apps/api/event";
  import { load } from "@tauri-apps/plugin-store";
  import { writeText, writeImage, readImage } from "@tauri-apps/plugin-clipboard-manager";
  import { confirm } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { startDrag } from "@crabnebula/tauri-plugin-drag";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import ArrowOverlay from "$lib/ArrowOverlay.svelte";
  import MaskOverlay from "$lib/MaskOverlay.svelte";
  import ShapeOverlay from "$lib/ShapeOverlay.svelte";
  import TextOverlay from "$lib/TextOverlay.svelte";
  import OcrSelectionOverlay from "$lib/OcrSelectionOverlay.svelte";
  import CropOverlay from "$lib/CropOverlay.svelte";
  import { detectEdgeSnapLines, type EdgeSnapLines } from "$lib/edgeSnap";
  import { largestAspectRect, refitToAspect } from "$lib/cropAspect";
  import Toolbar from "$lib/Toolbar.svelte";
  import VideoTrimmer from "$lib/VideoTrimmer.svelte";

  let arrowOverlayRef = $state<ReturnType<typeof ArrowOverlay> | null>(null);
  let maskOverlayRef = $state<ReturnType<typeof MaskOverlay> | null>(null);
  let shapeOverlayRef = $state<ReturnType<typeof ShapeOverlay> | null>(null);
  let textOverlayRef = $state<ReturnType<typeof TextOverlay> | null>(null);
  import type { Arrow, ArrowSettings, CropRect, MaskRect, MaskSettings, Shape, ShapeSettings, TextAnnotation, TextSettings } from "$lib/types";

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

  // Video capture state
  let videoMode = $state(false);
  let videoUrl = $state<string | null>(null);
  let videoPath = $state<string | null>(null);
  let ffmpegAvailable = $state(true);

  // 録画中の状態
  let isRecording = $state(false);
  let isStopping = $state(false);
  let recordElapsedMs = $state(0);
  let recordTimer: ReturnType<typeof setInterval> | null = null;

  let recordElapsedLabel = $derived.by(() => {
    const total = Math.floor(recordElapsedMs / 1000);
    const m = Math.floor(total / 60);
    const s = total % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  });

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

  // Crop tool state
  const CROP_SNAP_KEY = "flashcap-crop-snap";
  let cropToolActive = $state(false);
  let cropRect = $state<CropRect | null>(null);
  // トリミング枠を画像内の境界線に吸着させるか
  let cropSnapEnabled = $state(true);
  let cropSnapLines = $state<EdgeSnapLines | null>(null);
  // cropSnapLines がどの画像から取られたか。画像を差し替えたら取り直す
  let cropSnapLinesRevision = -1;
  // 固定する縦横比 (width / height)。null なら自由。
  // **セッションを跨いで覚えない** — 次の起動でトリミングが勝手に固定されていると驚くため
  let cropAspect = $state<number | null>(null);
  // 土台画像そのものを変更したか (トリミング)。ファイルへの書き戻しが必要かの判定に使う
  let imageModified = $state(false);
  // 土台画像の差し替え回数。MaskOverlay にモザイクの再サンプリング契機として渡す
  let imageRevision = $state(0);

  // Undo history
  interface EditSnapshot {
    arrows: Arrow[];
    masks: MaskRect[];
    shapes: Shape[];
    texts: TextAnnotation[];
    // トリミングを巻き戻せるように土台画像もスナップショットに含める。
    // 画像データを複製せず同じ base64 文字列を保持するだけなので、
    // 注釈編集のたびに画像がコピーされるわけではない
    image: string | null;
    width: number;
    height: number;
  }
  let undoHistory = $state<EditSnapshot[]>([]);

  function pushUndo() {
    undoHistory.push({
      arrows: structuredClone($state.snapshot(arrows)),
      masks: structuredClone($state.snapshot(masks)),
      shapes: structuredClone($state.snapshot(shapes)),
      texts: structuredClone($state.snapshot(textAnnotations)),
      image: imageBase64,
      width: naturalWidth,
      height: naturalHeight,
    });
  }

  function undo() {
    const entry = undoHistory.pop();
    if (!entry) return;
    arrows = entry.arrows;
    masks = entry.masks;
    shapes = entry.shapes;
    textAnnotations = entry.texts;
    if (entry.image !== imageBase64) {
      imageBase64 = entry.image;
      imageUrl = entry.image ? `data:image/png;base64,${entry.image}` : null;
      naturalWidth = entry.width;
      naturalHeight = entry.height;
      // 土台画像が別物に入れ替わるので、旧画像の座標で作った枠と吸着線は持ち越せない
      // (Cmd+Z は crop ツール表示中でも通るため、閉じないと枠が画像の外にはみ出す)
      if (cropToolActive) cancelCrop();
      bumpImageRevision();
      // imageModified はここで戻さない。トリミング後に保存やコピーでファイルへ
      // 書き戻していた場合、undo してもディスク上は変更済みのままなので、
      // 「メモリとファイルが一致している」とは言えなくなる
    }
  }

  // Image element reference for composite rendering
  let imgEl = $state<HTMLImageElement | null>(null);

  // Natural image dimensions and display scale
  let naturalWidth = $state(0);
  let naturalHeight = $state(0);
  let viewportEl = $state<HTMLDivElement | null>(null);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);

  let noToolActive = $derived(
    !arrowToolActive && !maskToolActive && !shapeToolActive && !textToolActive &&
    !ocrSelectionActive && !cropToolActive
  );

  let hasAnnotations = $derived(
    arrows.length > 0 || masks.length > 0 || shapes.length > 0 || textAnnotations.length > 0
  );

  // メモリ上の画像がファイルと食い違っている可能性がある状態
  let needsFileWrite = $derived(hasAnnotations || imageModified);

  // 表示中の <img> が現在の imageBase64 を持つまで待つ。
  // src 差し替えの DOM 反映は次のフレームで、そこからのデコードも非同期なので、
  // 待たずに <img> を読むと直前の画像を掴む
  async function waitForImageDecode() {
    await tick();
    try {
      await imgEl?.decode();
    } catch {
      // デコード失敗時は待たずに進む (次の描画で追いつく)
    }
  }

  // モザイクは <img> からサンプリングするため、画像を差し替えたら
  // デコード完了後に revision を上げて MaskOverlay に取り直させる
  async function bumpImageRevision() {
    // 吸着線は旧画像の座標系なので、差し替えが決まった時点で捨てる。revision の更新は
    // デコード完了後だが、こちらを待たせると「新しい寸法 + 旧画像の線」で吸着する隙ができる。
    //
    // **キャッシュ世代の無効化も同期的に行う。** imageRevision の更新はデコード待ちの
    // 後なので、この await の間に crop ツールを開き直されると
    // 「cropSnapLinesRevision === imageRevision (どちらも旧世代)」が成立してしまい、
    // detectCropSnapLines が中身空のまま早期 return する。その後 revision が上がっても
    // 検出は再スケジュールされないので、吸着が黙って効かないまま残る
    cropSnapLines = null;
    cropSnapLinesRevision = -1;
    await waitForImageDecode();
    imageRevision++;
  }

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

  /**
   * displayScale の分母になる「画像を置ける実寸」を測る。
   *
   * **clientWidth/Height は padding を含む** (padding box の寸法) ので、そのまま使うと
   * viewport の `p-5` ぶん 40px を余分に使えると見積もる。すると画像が **content box** を
   * 40px はみ出す大きさに拡大され、flex の中央寄せに負の余白が渡る。負の余白を開始側へ
   * 寄せる実装では「左に 20px の余白は残るのに、右は 20px はみ出して `overflow-hidden`
   * で切れる」という非対称なズレになる。content box を測れば余白が負にならないので、
   * 負の余白をどう配る実装でも左右対称の 20px に収まる。
   *
   * **表面化するかは表示スケール次第**なので、直る前も「たまに」に見えていた:
   * - Retina (scale_factor 2) では `naturalWidth` が論理 px の 2 倍あるので
   *   `displayScale` は 0.5 付近になり、`Math.min(..., 1)` の上限に当たらない。
   *   40px ぶんの過大評価がそのまま効くので **常に**はみ出す。
   * - 1x ディスプレイでは希望どおりのサイズで開けた時に比が 1 を超え、上限 1 に
   *   丸められて吸収される。はみ出すのは `resize_window_for_image` が
   *   `.min(max_w/max_h)` で作業領域にクランプされた時 (= 画像が画面より大きい時)。
   */
  function updateViewportSize() {
    if (!viewportEl) return;
    const style = getComputedStyle(viewportEl);
    const paddingX = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
    const paddingY = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
    viewportWidth = Math.max(0, viewportEl.clientWidth - paddingX);
    viewportHeight = Math.max(0, viewportEl.clientHeight - paddingY);
  }

  onMount(() => {
    // システム ffmpeg の有無を確認 (動画書き出しの可否判定)
    invoke<boolean>("check_ffmpeg_available")
      .then((available) => (ffmpegAvailable = available))
      .catch(() => (ffmpegAvailable = false));

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

    cropSnapEnabled = localStorage.getItem(CROP_SNAP_KEY) !== "off";

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

    // 起動時は自動キャプチャーしない (直接キャプチャーは --capture / do-capture で行う)
    updateViewportSize();

    const resizeObserver = new ResizeObserver(() => updateViewportSize());
    if (viewportEl) resizeObserver.observe(viewportEl);

    // アプリ再アクティブ時にキャプチャーボタンを点滅して目立たせる
    const unlisten = listen("reactivate", () => {
      highlightCapture = true;
      // animationend が発火しない環境(prefers-reduced-motion等)へのフォールバック
      setTimeout(() => { highlightCapture = false; }, 1500);
    });

    // --capture フラグ付きで再起動された場合: 点滅させずに直接キャプチャー
    const unlistenDoCapture = listen("do-capture", () => {
      if (!isCapturing) captureScreen();
    });

    // 範囲選択完了で録画が開始された: アプリ内タイマーを動かす
    const unlistenRecStart = listen("recording-started", () => {
      isRecording = true;
      recordElapsedMs = 0;
      const start = performance.now();
      stopRecordTimer();
      recordTimer = setInterval(() => {
        recordElapsedMs = performance.now() - start;
      }, 100);
    });

    // ファイル関連付けや Dock ドロップで開かれた場合
    const unlistenOpenFile = listen<string[]>("open-file", (event) => {
      if (event.payload.length > 0) {
        loadImageFile(event.payload[0]);
      }
    });

    // バックエンドへの準備完了通知。コールド起動 (WebView 未ロード) で取りこぼす
    // イベントはバックエンドが frontend-ready まで預かるので、預かり対象の
    // リスナー (do-capture / open-file) が **全部** 登録され終わってから一度だけ送る。
    // do-capture の登録だけを待って送ると、open-file の登録が間に合う保証が無い。
    // **預かり対象のイベントを増やしたら、この配列にも足すこと。**
    //
    // allSettled にしているのは、片方の listen が失敗した時に frontend-ready ごと
    // 飛ばなくなるのを避けるため。all だと 1 つの reject でキャプチャー予約まで
    // 巻き添えで座礁し、コールド起動が無反応になる。
    //
    // 逆に allSettled では、失敗した側の預かり分が「届け先が無いまま consume される」。
    // それでも all より被害が小さいので許容する (all は両方失われる)。listen が reject
    // するのは IPC 自体が壊れている時で、その状態では下の emit も届かない
    Promise.allSettled([unlistenDoCapture, unlistenOpenFile]).then(() => {
      emit("frontend-ready");
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
        if (isRecording) {
          // 録画中の Esc は停止 (アプリを閉じて録画プロセスを孤立させない)
          stopRecording();
          return;
        }
        if (ocrSelectionActive) {
          ocrSelectionActive = false;
          return;
        }
        if (cropToolActive) {
          cancelCrop();
          return;
        }
        getCurrentWindow().close();
      } else if (e.key === "Enter" && cropToolActive) {
        e.preventDefault();
        applyCrop();
      } else if (e.metaKey && e.shiftKey && e.key === "c") {
        e.preventDefault();
        copyImage();
      } else if (e.metaKey && e.key === "v") {
        const tag = (e.target as HTMLElement)?.tagName;
        const isEditable = (e.target as HTMLElement)?.isContentEditable;
        if (tag === "INPUT" || tag === "TEXTAREA" || isEditable) return;
        e.preventDefault();
        pasteImage();
      } else if (e.metaKey && e.key === "c") {
        e.preventDefault();
        copyPath();
      } else if (e.metaKey && e.key === "s") {
        e.preventDefault();
        saveImage();
      } else if (e.metaKey && e.key === "z") {
        e.preventDefault();
        undo();
      }
    }
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("keydown", handleKeydown);
      unlisten.then((fn) => fn());
      unlistenDoCapture.then((fn) => fn());
      unlistenRecStart.then((fn) => fn());
      unlistenOpenFile.then((fn) => fn());
      unlistenDragDrop.then((fn) => fn());
      stopRecordTimer();
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

  $effect(() => {
    localStorage.setItem(CROP_SNAP_KEY, cropSnapEnabled ? "on" : "off");
  });

  // テキスト属性変更時、編集中/選択中のテキストにも反映する
  function updateTextSetting<K extends keyof TextSettings>(key: K, value: TextSettings[K]) {
    textSettings[key] = value;
    textOverlayRef?.updateActiveAttribute(key, value);
  }

  // ScreenshotResult を画面に反映し、注釈・履歴・寸法キャッシュをリセットする
  function applyScreenshotResult(result: ScreenshotResult) {
    // 画像を表示する際は動画モードを解除する
    videoMode = false;
    videoUrl = null;
    videoPath = null;
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
    cropToolActive = false;
    cropRect = null;
    imageModified = false;
    bumpImageRevision();
  }

  async function loadImageFile(path: string) {
    try {
      const result = await invoke<ScreenshotResult>("load_image_file", { path });
      applyScreenshotResult(result);
    } catch (e) {
      console.error("Failed to load image:", e);
    }
  }

  async function pasteImage() {
    let clipboardImage;
    try {
      clipboardImage = await readImage();
    } catch {
      return;
    }

    if (imageUrl) {
      const confirmed = await confirm(
        "Replace the current image with the one on the clipboard?",
        { title: "Paste Image", kind: "warning" }
      );
      if (!confirmed) return;
    }

    try {
      const [rgba, size] = await Promise.all([
        clipboardImage.rgba(),
        clipboardImage.size(),
      ]);

      // RGBA → PNG (Canvas 経由)
      const canvas = document.createElement("canvas");
      canvas.width = size.width;
      canvas.height = size.height;
      const ctx = canvas.getContext("2d")!;
      const imageData = new ImageData(
        new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.byteLength),
        size.width,
        size.height
      );
      ctx.putImageData(imageData, 0, 0);

      const blob = await new Promise<Blob>((resolve) => {
        canvas.toBlob((b) => resolve(b!), "image/png");
      });
      const pngBytes = new Uint8Array(await blob.arrayBuffer());
      const base64 = uint8ToBase64(pngBytes);

      const result = await invoke<ScreenshotResult>("save_pasted_image", {
        dataBase64: base64,
        width: size.width,
        height: size.height,
      });

      applyScreenshotResult(result);
    } catch (e) {
      console.error("Failed to paste image:", e);
    }
  }

  async function captureScreen(command: string = "take_screenshot_interactive") {
    isCapturing = true;
    // キャプチャ完了までウィンドウを非表示にする
    const appWindow = getCurrentWindow();
    try {
      await appWindow.hide();
      const result = await invoke<ScreenshotResult>(command);
      applyScreenshotResult(result);
    } catch (e) {
      const errorStr = String(e);
      if (!errorStr.includes("cancelled")) {
        console.error("Capture failed:", e);
      }
    } finally {
      // キャプチャ完了・キャンセル後にウィンドウを再表示する。
      // show 直後に setFocus して最前面化する (--capture / single-instance /
      // flashcap://capture いずれの経路でも結果ウィンドウを前面に出すため。
      // Rust 側は do-capture を送るだけで show/focus しない)。
      // isCapturing は show/setFocus 完了後に false へ戻す。先に戻すと、
      // ウィンドウ復元の await 中に届いた do-capture が新キャプチャーを開始し、
      // 旧 show() と新 hide() がインターリーブして撮影に写り込む恐れがある。
      try {
        await appWindow.show();
        await appWindow.setFocus();
      } catch (e) {
        console.error("Failed to restore window after capture:", e);
      } finally {
        isCapturing = false;
      }
    }
  }

  // 動画録画開始: 範囲選択オーバーレイを開く
  // (Rust が範囲選択ウィンドウを出し、選択完了で recording-started イベントが届く)
  async function captureVideo() {
    if (isCapturing || isRecording) return;
    try {
      await invoke("open_region_selector");
    } catch (e) {
      console.error("Failed to open region selector:", e);
    }
  }

  function stopRecordTimer() {
    if (recordTimer != null) {
      clearInterval(recordTimer);
      recordTimer = null;
    }
  }

  // 録画停止 → finalize された動画をトリム画面へ。
  // finalize 完了までは isRecording を立てたままにして UI をビジー維持し、
  // 停止処理中に別キャプチャーで状態が上書きされる race を防ぐ
  async function stopRecording() {
    if (!isRecording || isStopping) return;
    isStopping = true;
    stopRecordTimer();
    try {
      const result = await invoke<{ file_path: string }>("stop_video_recording");
      // 画像状態をクリアして動画モードへ
      imageUrl = null;
      imageBase64 = null;
      filePath = null;
      arrows = [];
      masks = [];
      shapes = [];
      textAnnotations = [];
      undoHistory = [];
      deactivateAllTools();
      videoPath = result.file_path;
      videoUrl = convertFileSrc(result.file_path);
      videoMode = true;
    } catch (e) {
      console.error("Failed to stop recording:", e);
      notify("FlashCap", `Recording failed: ${e}`);
    } finally {
      isRecording = false;
      isStopping = false;
    }
  }

  // 動画書き出し完了: Finder で表示して通知
  async function onVideoExported(outputPath: string) {
    try {
      await revealItemInDir(outputPath);
    } catch (e) {
      console.error("Failed to reveal exported video:", e);
    }
    invoke("show_notification", {
      title: "FlashCap",
      body: "Video exported",
    }).catch(() => {});
  }

  function notify(title: string, body: string) {
    invoke("show_notification", { title, body }).catch(() => {});
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

  // 合成画像をファイルに書き出す（注釈やトリミングでファイルと食い違う場合のみ）
  // メモリ上の元画像 (imageBase64) はそのまま保持する
  // compositeBytes が渡された場合は再レンダリングをスキップする
  async function saveCompositeToFile(compositeBytes?: Uint8Array) {
    if (!filePath || !imageBase64 || !needsFileWrite) return;
    // 注釈が無ければ imageBase64 をそのまま渡し、base64 → bytes → base64 の往復を避ける
    const dataBase64 = compositeBytes
      ? uint8ToBase64(compositeBytes)
      : hasAnnotations
        ? uint8ToBase64(await renderComposite())
        : imageBase64;
    await invoke("write_image_to_file", { path: filePath, dataBase64 });
  }

  async function copyImage() {
    if (!imageBase64) return;

    const bytes = hasAnnotations
      ? await renderComposite()
      : base64ToUint8(imageBase64);

    await saveCompositeToFile(bytes);
    await writeImage(bytes);
    copyImageSuccess = true;
    setTimeout(() => (copyImageSuccess = false), 3000);
  }

  // 注釈をラスタライズして保存フォルダ(元ファイルパス)に書き出す
  async function saveImage() {
    if (!filePath || !imageBase64) return;
    // 注釈がある場合のみ再レンダリング。無ければ元の base64 をそのまま渡し、
    // base64 → Uint8Array → base64 の無駄な往復を避ける
    const dataBase64 = hasAnnotations ? uint8ToBase64(await renderComposite()) : imageBase64;
    try {
      await invoke("write_image_to_file", { path: filePath, dataBase64 });
      invoke("show_notification", {
        title: "FlashCap",
        body: "Saved",
      }).catch(() => {});
    } catch (e) {
      // 保存先ディレクトリ外のファイル等で write が reject される場合がある。
      // 無音だと「保存されたか不明」になるため、失敗理由を通知する
      console.error("Failed to save image:", e);
      invoke("show_notification", {
        title: "FlashCap",
        body: `Save failed: ${e}`,
      }).catch(() => {});
    }
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
    cropToolActive = false;
    cropRect = null;
    arrowOverlayRef?.deselect();
    maskOverlayRef?.deselect();
    shapeOverlayRef?.deselect();
    textOverlayRef?.deselect();
  }

  // 縦横比を固定している時は、画像いっぱいではなくその比で取れる最大の枠を初期値にする
  function fullImageCropRect(): CropRect {
    const bounds = { width: naturalWidth, height: naturalHeight };
    if (cropAspect !== null) return largestAspectRect(cropAspect, bounds);
    return { x: 0, y: 0, width: naturalWidth, height: naturalHeight };
  }

  function toggleCropTool() {
    const wasActive = cropToolActive;
    deactivateAllTools();
    if (wasActive || naturalWidth <= 0 || naturalHeight <= 0) return;
    cropToolActive = true;
    cropRect = fullImageCropRect();
    if (cropSnapEnabled) detectCropSnapLines();
  }

  /** 縦横比ボタン。同じ比を押し直すと解除、別の比を押すと乗り換える (排他) */
  function toggleCropAspect(ratio: number) {
    cropAspect = cropAspect === ratio ? null : ratio;
    // 押した瞬間に今の枠を新しい比へ合わせ直す。解除時は今の枠をそのまま残す
    if (cropAspect !== null && cropRect && naturalWidth > 0 && naturalHeight > 0) {
      cropRect = refitToAspect(cropRect, cropAspect, {
        width: naturalWidth,
        height: naturalHeight,
      });
    }
  }

  function toggleCropSnap() {
    cropSnapEnabled = !cropSnapEnabled;
    // 吸着を後から ON にした時、まだ検出していなければここで走らせる
    if (cropSnapEnabled && cropToolActive) detectCropSnapLines();
  }

  /**
   * トリミングの吸着先になる境界線を検出する (画像 1 枚につき 1 回)。
   *
   * **検出そのものはメインスレッドを止める同期処理。** ワーカーには逃がしていない。
   * 数千万画素のキャンバス確保と getImageData を crop ツールを開くクリックと同じ
   * フレームで走らせるとツールバーの反応が引っかかって見えるので、ツールが開いた
   * 見た目を先に描かせてから走らせている (吸着が要るのは最初のドラッグからなので、
   * 数フレーム遅れても操作には間に合う)。
   */
  async function detectCropSnapLines() {
    // 世代が一致していても中身が無いなら計算し直す (検出が canvas を読めずに null を
    // 返した場合も、次に開いた時にやり直す)
    if (cropSnapLines && cropSnapLinesRevision === imageRevision) return;
    await waitForImageDecode();
    // ツールを開いた見た目が 1 フレーム描かれるまで待つ (rAF 2 回で描画後になる)
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    if (!cropToolActive || !imgEl) return;
    if (cropSnapLines && cropSnapLinesRevision === imageRevision) return;
    cropSnapLinesRevision = imageRevision;
    cropSnapLines = detectEdgeSnapLines(imgEl);
  }

  function resetCropRect() {
    if (cropToolActive && naturalWidth > 0) cropRect = fullImageCropRect();
  }

  function cancelCrop() {
    cropToolActive = false;
    cropRect = null;
  }

  /**
   * 選択範囲で土台画像を切り出す。注釈は焼き込まず、切り出した原点ぶん平行移動して
   * 編集可能なまま残す (mask だけは新しい画像領域へクランプする)
   */
  async function applyCrop() {
    if (!cropToolActive || !cropRect) return;
    // 直前に画像を差し替えていると <img> がまだ古い画を持っている
    await waitForImageDecode();
    // await の間に Esc でのキャンセルや 2 回目の適用が入りうるので状態を取り直す
    if (!cropToolActive || !cropRect || !imageBase64 || !imgEl) return;
    const rect = cropRect;

    const cropX = Math.max(0, Math.round(rect.x));
    const cropY = Math.max(0, Math.round(rect.y));
    const cropW = Math.min(Math.round(rect.width), naturalWidth - cropX);
    const cropH = Math.min(Math.round(rect.height), naturalHeight - cropY);
    if (cropW < 1 || cropH < 1) return;
    if (cropX === 0 && cropY === 0 && cropW === naturalWidth && cropH === naturalHeight) {
      cancelCrop();
      return;
    }

    pushUndo();

    const canvas = document.createElement("canvas");
    canvas.width = cropW;
    canvas.height = cropH;
    const ctx = canvas.getContext("2d")!;
    ctx.drawImage(imgEl, cropX, cropY, cropW, cropH, 0, 0, cropW, cropH);
    const dataUrl = canvas.toDataURL("image/png");

    imageBase64 = dataUrl.slice(dataUrl.indexOf(",") + 1);
    imageUrl = dataUrl;
    naturalWidth = cropW;
    naturalHeight = cropH;
    arrows = arrows.map((a) => ({
      ...a,
      startX: a.startX - cropX, startY: a.startY - cropY,
      endX: a.endX - cropX, endY: a.endY - cropY,
    }));
    // mask は画像の外へはみ出したままにできない。renderComposite() の getImageData は
    // キャンバス外を transparent black で返すため、blur はアルファごと putImageData で
    // 焼き付き、mosaic は端のブロックが半透明になって隠したはずの元画像が透ける。
    // 切り落とされたぶんは undo で元の mask ごと戻る
    masks = masks
      .map((m) => {
        const left = Math.max(0, m.x - cropX);
        const top = Math.max(0, m.y - cropY);
        return {
          ...m,
          x: left,
          y: top,
          width: Math.min(m.x - cropX + m.width, cropW) - left,
          height: Math.min(m.y - cropY + m.height, cropH) - top,
        };
      })
      .filter((m) => m.width > 0 && m.height > 0);
    shapes = shapes.map((s) => ({ ...s, x: s.x - cropX, y: s.y - cropY }));
    textAnnotations = textAnnotations.map((t) => ({ ...t, x: t.x - cropX, y: t.y - cropY }));
    imageModified = true;
    cropToolActive = false;
    cropRect = null;
    bumpImageRevision();
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
      // OCR はクリップボードへのコピーが主目的でウィンドウ操作を伴わないため、
      // captureScreen() と異なり setFocus() でフォーカスを奪わず show のみ行う。
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
    {cropToolActive}
    {cropRect}
    {cropSnapEnabled}
    {cropAspect}
    hasImage={imageUrl != null}
    onToggleCropTool={toggleCropTool}
    onToggleCropSnap={toggleCropSnap}
    onToggleCropAspect={toggleCropAspect}
    onApplyCrop={applyCrop}
    onResetCrop={resetCropRect}
    onCancelCrop={cancelCrop}
    {arrowSettings}
    {maskSettings}
    {shapeSettings}
    {textSettings}
    {filePath}
    {copyPathSuccess}
    {copyImageSuccess}
    isCapturing={isCapturing || isRecording}
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
    onCaptureVideo={captureVideo}
    {videoMode}
    onDragFile={handleDragFile}
    onUpdateTextSetting={updateTextSetting}
    {highlightCapture}
    onHighlightEnd={() => highlightCapture = false}
  />

  <div bind:this={viewportEl} class="flex-1 flex items-center justify-center overflow-hidden p-5">
    {#if isRecording}
      <div class="flex flex-col items-center gap-5">
        <div class="flex items-center gap-3">
          <span class="w-3.5 h-3.5 rounded-full bg-red-500 animate-pulse"></span>
          <span class="text-3xl font-semibold tabular-nums text-white">{recordElapsedLabel}</span>
        </div>
        <button
          class="flex items-center gap-2 px-5 py-2.5 rounded-lg bg-red-600 hover:bg-red-500 text-white text-sm border-none cursor-pointer transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
          onclick={stopRecording}
          disabled={isStopping}
        >
          {#if isStopping}
            <i class="bi bi-arrow-repeat animate-spin text-lg"></i> Stopping…
          {:else}
            <i class="bi bi-stop-fill text-lg"></i> Stop and export
          {/if}
        </button>
        <div class="text-neutral-500 text-xs">You can also press Esc to stop</div>
      </div>
    {:else if videoMode && videoUrl && videoPath}
      <VideoTrimmer
        {videoUrl}
        {videoPath}
        {ffmpegAvailable}
        onExported={onVideoExported}
        onNotify={notify}
      />
    {:else if imageUrl}
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
          {imageRevision}
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
        {#if cropToolActive && cropRect}
          <CropOverlay
            rect={cropRect}
            imageWidth={naturalWidth}
            imageHeight={naturalHeight}
            scale={displayScale}
            snapEnabled={cropSnapEnabled}
            snapLines={cropSnapLines}
            aspect={cropAspect}
            onRectChange={(rect) => (cropRect = rect)}
          />
        {/if}
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
