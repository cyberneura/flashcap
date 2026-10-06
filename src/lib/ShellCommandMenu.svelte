<script lang="ts">
  import { shellCommandLabel, type ShellCommand } from "$lib/shellCommands";

  let {
    commands,
    onRun,
    onOpenLogs,
  }: {
    /** 実行できるものだけ (runnableShellCommands 済み) */
    commands: ShellCommand[];
    onRun: (command: ShellCommand) => void;
    onOpenLogs: () => void;
  } = $props();

  let open = $state(false);
  let rootEl = $state<HTMLDivElement | null>(null);

  function run(command: ShellCommand) {
    open = false;
    onRun(command);
  }

  // 開いている間だけ、外側のクリックと Esc で閉じる
  $effect(() => {
    if (!open) return;

    function onPointerDown(e: PointerEvent) {
      if (rootEl && !rootEl.contains(e.target as Node)) open = false;
    }
    // **キャプチャーフェーズで拾って伝播を止める。** +page.svelte は window の keydown で
    // Esc を「ウインドウを閉じる」に割り当てているので、ここで止めないとメニューを
    // 閉じるつもりの Esc でアプリごと終了する
    function onKeydown(e: KeyboardEvent) {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      open = false;
    }
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeydown, true);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeydown, true);
    };
  });
</script>

<div class="relative" bind:this={rootEl}>
  <button
    class="menu-btn"
    class:active={open}
    onclick={() => (open = !open)}
    aria-label="Run shell command"
    aria-haspopup="menu"
    aria-expanded={open}
    data-tooltip={open ? null : "Run shell command"}
  >
    <i class="bi bi-terminal"></i>
  </button>

  {#if open}
    <div class="menu-popover" role="menu" aria-label="Run shell command">
      <div class="text-[11px] font-semibold text-neutral-400 px-2 pt-1 pb-1.5">
        Run shell command
      </div>
      {#each commands as command (command.id)}
        <button class="menu-item" role="menuitem" onclick={() => run(command)} title={command.command}>
          <span class="text-[13px] text-neutral-100">{shellCommandLabel(command)}</span>
          <span class="menu-command">{command.command}</span>
        </button>
      {/each}
      <div class="my-1 h-px bg-neutral-700"></div>
      <button class="menu-item" role="menuitem" onclick={() => { open = false; onOpenLogs(); }}>
        <span class="text-[12px] text-neutral-400"><i class="bi bi-journal-text mr-1.5"></i>Open logs folder</span>
      </button>
    </div>
  {/if}
</div>

<style>
  @reference "../app.css";

  .menu-btn {
    @apply flex items-center justify-center w-8 h-8
      border-none rounded-md bg-transparent text-neutral-300
      cursor-pointer transition-[background,color] duration-150 text-base;
  }

  .menu-btn:hover {
    @apply bg-neutral-700 text-white;
  }

  .menu-btn.active {
    @apply bg-blue-600 text-white;
  }

  .menu-popover {
    @apply absolute top-[calc(100%+6px)] right-0 z-100 w-72
      p-1.5 rounded-lg bg-neutral-800 border border-neutral-600
      shadow-[0_8px_24px_rgba(0,0,0,0.5)];
  }

  .menu-item {
    @apply flex flex-col items-start w-full gap-0.5 px-2 py-1.5 rounded-md
      border-none bg-transparent text-left cursor-pointer;
  }

  .menu-item:hover,
  .menu-item:focus-visible {
    @apply bg-neutral-700 outline-none;
  }

  .menu-command {
    @apply w-full text-[11px] text-neutral-500 font-mono truncate;
  }

  [data-tooltip] {
    @apply relative;
  }

  /* 右端のボタンなので、中央揃えだとツールチップがウインドウの外へはみ出す */
  [data-tooltip]::after {
    content: attr(data-tooltip);
    @apply absolute top-[calc(100%+6px)] right-0
      px-2 py-1 bg-black text-neutral-200 text-[11px] leading-tight
      rounded whitespace-nowrap pointer-events-none opacity-0
      transition-opacity duration-100 z-100;
  }

  [data-tooltip]:hover::after {
    @apply opacity-100;
  }
</style>
