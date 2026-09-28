<script lang="ts">
  // Third-Party Licenses ウインドウ (src-tauri/src/licenses.rs が開く)。
  // 本文は Rust が埋め込んだ THIRD-PARTY-NOTICES.txt を third_party_notices で受け取る
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

  let notices = $state("");
  let error = $state("");

  onMount(() => {
    invoke<string>("third_party_notices")
      .then((text) => {
        notices = text;
      })
      .catch((e) => {
        console.error("Failed to load the third-party notices:", e);
        error = String(e);
      });

    const onKeydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        void getCurrentWebviewWindow().close();
      }
    };
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });
</script>

<svelte:head>
  <title>Third-Party Licenses</title>
</svelte:head>

<div class="h-screen flex flex-col bg-[#1a1a1a] text-white p-5 font-[-apple-system,BlinkMacSystemFont,'Segoe_UI',Roboto,sans-serif]">
  <p class="flex-none mb-3 text-sm text-gray-300">
    FlashCap bundles the open source libraries listed below, under the licenses shown.
  </p>
  {#if error}
    <p class="text-sm text-red-400">Could not load the license list: {error}</p>
  {:else}
    <pre
      class="flex-1 min-h-0 overflow-auto p-3 rounded-md border border-[#2d2d2d] bg-[#0f0f0f] text-[#d0d0d0] text-[11px] leading-[1.45] whitespace-pre-wrap select-text font-[Menlo,Consolas,monospace]">{notices}</pre>
  {/if}
</div>
