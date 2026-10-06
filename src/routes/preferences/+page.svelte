<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { homeDir } from "@tauri-apps/api/path";
  import { open, confirm } from "@tauri-apps/plugin-dialog";
  import { Toaster, toast } from "svelte-sonner";
  import { isWindows } from "$lib/platform";
  import { flushConfig, loadConfig, rememberConfig, setConfig, setConfigDebounced } from "$lib/config";
  import { collapseHome } from "$lib/homePath";
  import {
    SHELL_COMMANDS_KEY,
    SHELL_PRESETS,
    parseShellCommands,
    type ShellCommand,
  } from "$lib/shellCommands";

  type SaveMode = "tmp" | "macos_default" | "custom";

  let saveMode = $state<SaveMode>("tmp");
  // 既定の保存先は $TMPDIR 配下でユーザーごとに異なるため、
  // ここに書かずバックエンドが解決したパスを表示する
  let defaultSavePath = $state("");
  // ホームフォルダの下なら `~/...` の形 (設定ファイルに書く形のまま表示する)
  let customPath = $state("");
  let timerDelay = $state(5);
  let excludeShadow = $state(true);
  let showInMenuBar = $state(false);
  let blurRadius = $state(5);
  let mosaicBlockSize = $state(7);
  let shellCommands = $state<ShellCommand[]>([]);
  // 設定ファイルを読めるまでは書かない (読めない時に書くと既存の設定を上書きしてしまう)
  let loaded = $state(false);

  onMount(() => {
    init();
    window.addEventListener("beforeunload", flushConfig);
    return () => window.removeEventListener("beforeunload", flushConfig);
  });

  async function init() {
    defaultSavePath = await invoke<string>("get_default_save_directory");
    let config: Record<string, unknown>;
    try {
      config = await loadConfig();
    } catch (e) {
      toast.error("Could not read the config file", { description: String(e), duration: Infinity });
      return;
    }
    const saved = config.save_directory;
    if (typeof saved === "string") {
      if (saved === "tmp") {
        saveMode = "tmp";
      } else if (saved === "macos_default") {
        saveMode = "macos_default";
      } else if (saved.startsWith("custom:")) {
        saveMode = "custom";
        customPath = saved.slice("custom:".length);
      }
    }
    if (typeof config.timer_delay === "number") timerDelay = config.timer_delay;
    if (typeof config.exclude_shadow === "boolean") excludeShadow = config.exclude_shadow;
    showInMenuBar = config.show_in_menu_bar === true;
    if (typeof config.blur_radius === "number") blurRadius = config.blur_radius;
    if (typeof config.mosaic_block_size === "number") mosaicBlockSize = config.mosaic_block_size;
    shellCommands = parseShellCommands(config[SHELL_COMMANDS_KEY]);
    rememberConfig(SHELL_COMMANDS_KEY, config[SHELL_COMMANDS_KEY]);
    loaded = true;
  }

  async function saveSetting(key: string, value: unknown): Promise<boolean> {
    if (!loaded) return false;
    try {
      await setConfig(key, value);
      return true;
    } catch (e) {
      console.error(`Failed to save "${key}":`, e);
      toast.error("Could not save the setting", { description: String(e) });
      return false;
    }
  }

  async function save() {
    let value: string;
    if (saveMode === "tmp") {
      value = "tmp";
    } else if (saveMode === "macos_default") {
      value = "macos_default";
    } else {
      value = `custom:${customPath}`;
    }
    await saveSetting("save_directory", value);
  }

  async function selectFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (selected) {
      customPath = collapseHome(selected as string, await homeDir());
      saveMode = "custom";
      await save();
    }
  }

  async function onBlurRadiusChange(value: number) {
    blurRadius = value;
    await saveSetting("blur_radius", value);
  }

  async function onMosaicBlockSizeChange(value: number) {
    mosaicBlockSize = value;
    await saveSetting("mosaic_block_size", value);
  }

  async function onExcludeShadowChange(value: boolean) {
    excludeShadow = value;
    await saveSetting("exclude_shadow", value);
  }

  async function onTimerDelayChange(value: number) {
    timerDelay = value;
    if (!(await saveSetting("timer_delay", value))) return;
    // メニューバーのメニューの「with Timer (5s)」の秒数を追従させる
    await syncMenuBar();
  }

  async function onShowInMenuBarChange(value: boolean) {
    if (!loaded) return;
    showInMenuBar = value;
    if (!(await saveSetting("show_in_menu_bar", value))) {
      // チェックの見た目と、閉じた時に終了するか隠れるかの実際の挙動を食い違わせない
      showInMenuBar = !value;
      return;
    }
    if (!(await syncMenuBar()) && value) {
      // アイコンを置けなかった。保存値だけ ON のまま残すと、次の起動でも置けずに
      // 「常駐するはずなのに閉じると終了する」になるので、OFF に戻す
      showInMenuBar = false;
      await saveSetting("show_in_menu_bar", false);
    }
  }

  // シェルコマンドは入力のたびに (まとめて) 書く
  function saveShellCommands() {
    if (!loaded) return;
    setConfigDebounced(SHELL_COMMANDS_KEY, $state.snapshot(shellCommands), 400);
  }

  function addShellCommand() {
    shellCommands.push({
      id: crypto.randomUUID(),
      name: "",
      shell: SHELL_PRESETS[0],
      command: 'echo "${IMAGE_PATH}"',
    });
    saveShellCommands();
  }

  async function removeShellCommand(index: number) {
    const label = shellCommands[index].name.trim() || "this command";
    if (!(await confirm(`Delete ${label}?`, { title: "Shell Commands", kind: "warning" }))) return;
    shellCommands.splice(index, 1);
    saveShellCommands();
  }

  function moveShellCommand(index: number, delta: number) {
    const target = index + delta;
    if (target < 0 || target >= shellCommands.length) return;
    const [item] = shellCommands.splice(index, 1);
    shellCommands.splice(target, 0, item);
    saveShellCommands();
  }

  const OTHER_SHELL = "__other__";

  function onShellPresetChange(command: ShellCommand, value: string) {
    // 「Other」を選んだら空にして、下の入力欄にパスを書かせる
    command.shell = value === OTHER_SHELL ? "" : value;
    saveShellCommands();
  }

  function openShellLogDir() {
    invoke("open_shell_log_dir").catch((e) =>
      toast.error("Could not open the logs folder", { description: String(e) })
    );
  }

  // Rust は設定ファイルの値を読んでアイコンを置く / 外す (src-tauri/src/menu_bar.rs)。
  // 置けなかった (失敗した) 時は false を返す
  async function syncMenuBar(): Promise<boolean> {
    try {
      await invoke("sync_menu_bar");
      return true;
    } catch (e) {
      console.error("Failed to update the menu bar icon:", e);
      return false;
    }
  }

  // Windows にはアプリメニューが無いので、ライセンス一覧の入口をここにも置く
  // (macOS はアプリメニューの About の直下にもある)
  async function openThirdPartyLicenses() {
    try {
      await invoke("open_third_party_licenses");
    } catch (e) {
      console.error("Failed to open Third-Party Licenses:", e);
    }
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
  <Toaster theme="dark" position="bottom-center" richColors closeButton />
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
        {#if isWindows}
          <div class="text-sm font-medium">Windows Default</div>
          <div class="text-xs text-gray-500 mt-0.5">Pictures\Screenshots (where Windows saves Win+PrtScn screenshots)</div>
        {:else}
          <div class="text-sm font-medium">macOS Default</div>
          <div class="text-xs text-gray-500 mt-0.5">System screenshot save location (Desktop or custom)</div>
        {/if}
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
    <!-- Windows はモニター全体を撮るので、ウインドウの影という概念が無い -->
    {#if !isWindows}
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
    {/if}
  </section>

  <section class="mt-8">
    <h3 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
      {isWindows ? "Notification Area" : "Menu Bar"}
    </h3>
    <label class="flex items-center gap-3 px-3 py-2.5 rounded-lg cursor-pointer hover:bg-[#2d2d2d] transition-colors">
      <input
        type="checkbox"
        checked={showInMenuBar}
        onchange={(e) => onShowInMenuBarChange((e.target as HTMLInputElement).checked)}
        class="accent-blue-600"
      />
      <div>
        {#if isWindows}
          <div class="text-sm font-medium">Keep FlashCap in the notification area</div>
          <div class="text-xs text-gray-500 mt-0.5">
            Capture from the notification area icon. Closing the window keeps FlashCap running.
          </div>
        {:else}
          <div class="text-sm font-medium">Keep FlashCap in the menu bar</div>
          <div class="text-xs text-gray-500 mt-0.5">
            Capture, record, and copy text from the menu bar. Closing the window keeps FlashCap running.
          </div>
        {/if}
      </div>
    </label>
  </section>

  <!-- シェル実行は macOS のみ (src-tauri/src/shell_command.rs) -->
  {#if !isWindows}
  <section class="mt-8">
    <h3 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-2">
      Shell Commands
    </h3>
    <p class="text-xs text-gray-500 px-3 mb-3 leading-relaxed">
      Run a command on the image from the <i class="bi bi-terminal"></i> button in the toolbar.
      The image path is passed in the <code class="text-gray-300">IMAGE_PATH</code> environment variable &mdash;
      write <code class="text-gray-300">"${"{"}IMAGE_PATH{"}"}"</code> (with the quotes) in the command.
      The shell runs as a login shell (<code class="text-gray-300">-l -c</code>) in your home folder.
    </p>

    {#each shellCommands as command, index (command.id)}
      <div class="mx-3 mb-3 p-3 rounded-lg bg-[#242424] border border-[#333]">
        <div class="flex items-center gap-2">
          <input
            type="text"
            placeholder="Name"
            bind:value={command.name}
            oninput={saveShellCommands}
            class="field flex-1 min-w-0"
            aria-label="Name"
          />
          <select
            value={SHELL_PRESETS.includes(command.shell) ? command.shell : OTHER_SHELL}
            onchange={(e) => onShellPresetChange(command, (e.target as HTMLSelectElement).value)}
            class="field cursor-pointer"
            aria-label="Shell"
          >
            {#each SHELL_PRESETS as shell}
              <option value={shell}>{shell}</option>
            {/each}
            <option value={OTHER_SHELL}>Other...</option>
          </select>
          <button class="icon-btn" onclick={() => moveShellCommand(index, -1)} disabled={index === 0} aria-label="Move up">
            <i class="bi bi-arrow-up"></i>
          </button>
          <button
            class="icon-btn"
            onclick={() => moveShellCommand(index, 1)}
            disabled={index === shellCommands.length - 1}
            aria-label="Move down"
          >
            <i class="bi bi-arrow-down"></i>
          </button>
          <button class="icon-btn hover:text-red-400" onclick={() => removeShellCommand(index)} aria-label="Delete">
            <i class="bi bi-trash"></i>
          </button>
        </div>
        {#if !SHELL_PRESETS.includes(command.shell)}
          <input
            type="text"
            placeholder="/opt/homebrew/bin/fish"
            bind:value={command.shell}
            oninput={saveShellCommands}
            class="field w-full mt-2 font-mono"
            aria-label="Shell path"
            spellcheck="false"
          />
        {/if}
        <textarea
          rows="3"
          placeholder={'open -a Preview "${IMAGE_PATH}"'}
          bind:value={command.command}
          oninput={saveShellCommands}
          class="field w-full mt-2 font-mono text-[12px] resize-y"
          aria-label="Command"
          spellcheck="false"
        ></textarea>
      </div>
    {/each}

    <div class="flex items-center gap-3 px-3">
      <button
        class="px-3.5 py-1.5 bg-blue-600 hover:bg-blue-500 text-white text-[13px] rounded-md cursor-pointer whitespace-nowrap border-none disabled:opacity-50"
        onclick={addShellCommand}
        disabled={!loaded}
      >
        <i class="bi bi-plus-lg mr-1"></i>Add Command
      </button>
      <button
        class="px-3.5 py-1.5 bg-[#2d2d2d] hover:bg-[#3d3d3d] text-white text-[13px] rounded-md cursor-pointer whitespace-nowrap border border-[#3d3d3d]"
        onclick={openShellLogDir}
      >
        Open Logs Folder
      </button>
    </div>
  </section>
  {/if}

  <section class="mt-8">
    <h3 class="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
      About
    </h3>
    <div class="flex items-center gap-3 px-3 py-2.5">
      <div class="flex-1">
        <div class="text-sm font-medium">Open source licenses</div>
        <div class="text-xs text-gray-500 mt-0.5">Libraries bundled with FlashCap and their licenses</div>
      </div>
      <button
        class="px-3.5 py-1.5 bg-[#2d2d2d] hover:bg-[#3d3d3d] text-white text-[13px] rounded-md cursor-pointer whitespace-nowrap border border-[#3d3d3d]"
        onclick={openThirdPartyLicenses}
      >
        Third-Party Licenses
      </button>
    </div>
  </section>
</div>

<style>
  @reference "../../app.css";

  .field {
    @apply bg-[#2d2d2d] text-white text-[13px] rounded-md px-2.5 py-1.5
      border border-[#3d3d3d] outline-none;
  }

  .field:focus {
    @apply border-blue-600;
  }

  .icon-btn {
    @apply flex items-center justify-center w-7 h-7 rounded-md border-none
      bg-transparent text-gray-400 cursor-pointer;
  }

  .icon-btn:hover:not(:disabled) {
    @apply bg-[#3d3d3d] text-white;
  }

  .icon-btn:disabled {
    @apply opacity-30 cursor-not-allowed;
  }
</style>
