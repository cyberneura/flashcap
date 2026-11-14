<script lang="ts">
  // 蛍光ペンの塗り (CYBERNEURA-DEV-1264)。表示だけを受け持ち、操作は ShapeOverlay が受ける。
  //
  // +page.svelte で <img> のすぐ上・MaskOverlay より下に置く。マスクは「隠す」ものなので
  // 蛍光ペンより上に来る。書き出し (renderComposite) もマスクの範囲には蛍光ペンを描かない。
  import type { Shape } from "./types";
  import { highlightPaint, type LumaMap } from "./highlight";

  interface Props {
    shapes: Shape[];
    /** 画像の明るさの地図。乗算で重ねるか半透明で重ねるかの判定に使う (highlight.ts) */
    lumaMap: LumaMap | null;
  }

  let { shapes, lumaMap }: Props = $props();

  let highlights = $derived(shapes.filter((s) => s.type === "highlight"));
</script>

<!--
  svg ではなく div に mix-blend-mode を当て、下の <img> と混ぜる。svg の中の要素に当てた
  mix-blend-mode が svg の外の要素と混ざるかは描画エンジン次第なので頼らない
  (HTML の兄弟要素どうしなら素直に混ざる)。
-->
{#each highlights as shape (shape.id)}
  {@const paint = highlightPaint(lumaMap, shape)}
  <div
    class="highlight-fill"
    style:left="{shape.x}px"
    style:top="{shape.y}px"
    style:width="{shape.width}px"
    style:height="{shape.height}px"
    style:background-color={shape.color}
    style:mix-blend-mode={paint.blend}
    style:opacity={paint.opacity}
  ></div>
{/each}

<style>
  @reference "../app.css";

  .highlight-fill {
    @apply absolute pointer-events-none;
  }
</style>
