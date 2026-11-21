<script lang="ts">
  // スポットライトの暗い部分 (CYBERNEURA-DEV-1322)。表示だけを受け持ち、操作は ShapeOverlay が受ける。
  //
  // 画像と同じ大きさの div を半透明の黒で塗り、clip-path で「暗くする範囲」だけを残す。
  // 範囲の計算 (スポットライトの和集合とマスクのくり抜き) は spotlight.ts の spotlightDarkRects で、
  // 書き出し (renderComposite) も同じ関数で同じ矩形を塗る。
  //
  // +page.svelte で HighlightLayer のすぐ上・MaskOverlay より下に置く。矢印・枠・文字・ステップは
  // 更に上なので暗くならない (外側に置いた注釈も見える)。
  import type { MaskRect, Shape } from "./types";
  import { spotlightClipPath, spotlightDarkRects } from "./spotlight";

  interface Props {
    shapes: Shape[];
    masks: MaskRect[];
    imageWidth: number;
    imageHeight: number;
    /** 外側に重ねる黒の不透明度 */
    darkness: number;
  }

  let { shapes, masks, imageWidth, imageHeight, darkness }: Props = $props();

  let hasSpotlight = $derived(shapes.some((s) => s.type === "spotlight"));
  let clipPath = $derived(
    hasSpotlight ? spotlightClipPath(spotlightDarkRects(imageWidth, imageHeight, shapes, masks)) : null,
  );
</script>

{#if clipPath}
  <div
    class="spotlight-dim"
    style:background-color="rgba(0, 0, 0, {darkness})"
    style:clip-path={clipPath}
  ></div>
{/if}

<style>
  @reference "../app.css";

  .spotlight-dim {
    @apply absolute top-0 left-0 w-full h-full pointer-events-none;
  }
</style>
