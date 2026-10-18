<script lang="ts">
  /** 骨架屏：内容加载缓冲占位（感官层基线 §二 通用交互动效） */
  let { lines = 4 }: { lines?: number } = $props()

  const widths = $derived(
    Array.from({ length: lines }, (_, i) => `${72 - (i % 3) * 14}%`)
  )
</script>

<div class="skel" aria-hidden="true">
  {#each widths as w, i (i)}
    <div class="bar" style:width={w}></div>
  {/each}
</div>

<style>
  .skel {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .bar {
    height: 14px;
    border-radius: 7px;
    background: var(--phx-fill-tertiary);
    animation: skel-breathe 1.2s ease-in-out infinite;
  }

  /* 呼吸只动 opacity；reduced-motion 双通道含 iteration-count:1，降级后静态显示不闪烁 */
  @keyframes skel-breathe {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.4;
    }
  }
</style>
