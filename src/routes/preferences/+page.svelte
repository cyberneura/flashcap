<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { load, type Store } from "@tauri-apps/plugin-store";
  import { open } from "@tauri-apps/plugin-dialog";

  type SaveMode = "tmp" | "macos_default" | "custom";

  let saveMode = $state<SaveMode>("tmp");
  // 既定の保存先は $TMPDIR 配下でユーザーごとに異なるため、
  // ここに書かずバックエンドが解決したパスを表示する
  let defaultSavePath = $state("");
  let customPath = $state("");
  let timerDelay = $state(5);
  let excludeShadow = $state(true);
  let blurRadius = $state(5);
  let mosaicBlockSize = $state(7);
  let store = $state<Store | null>(null);

  onMount(async () => {
    defaultSavePath = await invoke<string>("get_default_save_directory");
    store = await load("settings.json");
    const saved = await store.get<string>("save_directory");
    if (saved) {
      if (saved === "tmp") {
        saveMode = "tmp";
      } else if (saved === "macos_default") {
        saveMode = "macos_default";
      } else if (saved.startsWith("custom:")) {
        saveMode = "custom";
        customPath = saved.slice("custom:".length);
      }
    }
    const savedTimer = await store.get<number>("timer_delay");
    if (savedTimer != null) timerDelay = savedTimer;
    const savedShadow = await store.get<boolean>("exclude_shadow");
    if (savedShadow != null) excludeShadow = savedShadow;
    const savedBlur = await store.get<number>("blur_radius");
    if (savedBlur != null) blurRadius = savedBlur;
    const savedMosaic = await store.get<number>("mosaic_block_size");
    if (savedMosaic != null) mosaicBlockSize = savedMosaic;
  });

  async function save() {
    if (!store) return;
    let value: string;
    if (saveMode === "tmp") {
      value = "tmp";
    } else if (saveMode === "macos_default") {
      value = "macos_default";
    } else {
      value = `custom:${customPath}`;
    }
    await store.set("save_directory", value);
    await store.save();
  }

  async function selectFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      customPath = selected as string;
      saveMode = "custom";
      await save();
    }
  }

  async function onBlurRadiusChange(value: number) {
    blurRadius = value;
    if (!store) return;
    await store.set("blur_radius", value);
    await store.save();
  }

  async function onMosaicBlockSizeChange(value: number) {
    mosaicBlockSize = value;
    if (!store) return;
    await store.set("mosaic_block_size", value);
    await store.save();
  }

  async function onExcludeShadowChange(value: boolean) {
    excludeShadow = value;
    if (!store) return;
    await store.set("exclude_shadow", value);
    await store.save();
  }

  async function onTimerDelayChange(value: number) {
    timerDelay = value;
    if (!store) return;
    await store.set("timer_delay", value);
    await store.save();
  }

  async function onModeChange(mode: SaveMode) {
    saveMode = mode;
    if (mode === "custom" && !customPath) {
      // フォルダ未選択なら選択ダイアログを開く
      await selectFolder();
      return;
    }
    await save();
  }
</script>

<div class="min-h-screen bg-[#1a1a1a] text-white p-6 font-[-apple-system,BlinkMacSystemFont,'Segoe_UI',Roboto,sans-serif]">
  <h2 class="text-xl font-semibold mb-6">Preferences</h2>

  <section>
    <h3 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
      Save Location
    </h3>

    <label class="flex items-start gap-3 px-3 py-2.5 rounded-lg cursor-pointer hover:bg-[#2d2d2d] transition-colors">
      <input
        type="radio"
        name="saveMode"
        value="tmp"
        checked={saveMode === "tmp"}
        onchange={() => onModeChange("tmp")}
        class="mt-0.5 accent-blue-600"
      />
      <div>
        <div class="text-sm font-medium break-all">
          {defaultSavePath || "Temporary directory"}
        </div>
        <div class="text-xs text-gray-500 mt-0.5">Temporary directory (default)</div>
      </div>
    </label>

    <label class="flex items-start gap-3 px-3 py-2.5 rounded-lg cursor-pointer hover:bg-[#2d2d2d] transition-colors">
      <input
        type="radio"
        name="saveMode"
        value="macos_default"
        checked={saveMode === "macos_default"}
        onchange={() => onModeChange("macos_default")}
        class="mt-0.5 accent-blue-600"
      />
      <div>
        <div class="text-sm font-medium">macOS Default</div>
        <div class="text-xs text-gray-500 mt-0.5">System screenshot save location (Desktop or custom)</div>
      </div>
    </label>

    <label class="flex items-start gap-3 px-3 py-2.5 rounded-lg cursor-pointer hover:bg-[#2d2d2d] transition-colors">
      <input
        type="radio"
        name="saveMode"
        value="custom"
        checked={saveMode === "custom"}
        onchange={() => onModeChange("custom")}
        class="mt-0.5 accent-blue-600"
      />
      <div>
        <div class="text-sm font-medium">Custom Folder</div>
        <div class="text-xs text-gray-500 mt-0.5">Choose a specific folder</div>
      </div>
    </label>

    {#if saveMode === "custom"}
      <div class="flex items-center gap-3 ml-9 mt-1 px-3 py-2 bg-[#2d2d2d] rounded-md">
        <span class="text-[13px] text-gray-400 flex-1 overflow-hidden text-ellipsis whitespace-nowrap">
          {customPath || "(not selected)"}
        </span>
        <button
          class="px-3.5 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-[13px] rounded-md cursor-pointer whitespace-nowrap border-none"
          onclick={selectFolder}
        >
          Browse...
        </button>
      </div>
    {/if}
  </section>

  <section class="mt-8">
    <h3 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
      Mask
    </h3>
    <div class="flex items-center gap-3 px-3 py-2.5">
      <label for="blurRadius" class="text-sm font-medium w-32">Blur intensity</label>
      <input
        id="blurRadius"
        type="range"
        min="1"
        max="20"
        value={blurRadius}
        oninput={(e) => onBlurRadiusChange(Number((e.target as HTMLInputElement).value))}
        class="flex-1 accent-blue-600"
      />
      <span class="text-sm text-gray-400 w-8 text-right">{blurRadius}</span>
    </div>
    <div class="flex items-center gap-3 px-3 py-2.5">
      <label for="mosaicBlockSize" class="text-sm font-medium w-32">Mosaic size</label>
      <input
        id="mosaicBlockSize"
        type="range"
        min="3"
        max="30"
        value={mosaicBlockSize}
        oninput={(e) => onMosaicBlockSizeChange(Number((e.target as HTMLInputElement).value))}
        class="flex-1 accent-blue-600"
      />
      <span class="text-sm text-gray-400 w-8 text-right">{mosaicBlockSize}</span>
    </div>
  </section>

  <section class="mt-8">
    <h3 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
      Capture
    </h3>
    <div class="flex items-center gap-3 px-3 py-2.5">
      <label for="timerDelay" class="text-sm font-medium">Timer delay</label>
      <select
        id="timerDelay"
        value={timerDelay}
        onchange={(e) => onTimerDelayChange(Number((e.target as HTMLSelectElement).value))}
        class="bg-[#2d2d2d] text-white text-sm rounded-md px-3 py-1.5 border border-[#3d3d3d] cursor-pointer"
      >
        {#each [3, 5, 10] as sec}
          <option value={sec}>{sec} seconds</option>
        {/each}
      </select>
    </div>
    <label class="flex items-center gap-3 px-3 py-2.5 rounded-lg cursor-pointer hover:bg-[#2d2d2d] transition-colors">
      <input
        type="checkbox"
        checked={excludeShadow}
        onchange={(e) => onExcludeShadowChange((e.target as HTMLInputElement).checked)}
        class="accent-blue-600"
      />
      <div>
        <div class="text-sm font-medium">Exclude window shadow</div>
        <div class="text-xs text-gray-500 mt-0.5">Remove drop shadow when capturing a window</div>
      </div>
    </label>
  </section>
</div>
