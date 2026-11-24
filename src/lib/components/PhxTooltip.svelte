<script lang="ts">
  import type { Snippet } from 'svelte'

  let {
    label,
    placement = 'top',
    children
  }: {
    /** 提示文字 */
    label: string
    /** 出现位置 */
    placement?: 'top' | 'bottom'
    children: Snippet
  } = $props()

  let show = $state(false)
  let timer: number | undefined

  function enter(): void {
    window.clearTimeout(timer)
    timer = window.setTimeout(() => {
      show = true
    }, 300)
  }

  function leave(): void {
    window.clearTimeout(timer)
    timer = undefined
    show = false
  }
</script>

<span
  class="tipwrap"
  role="presentation"
  onmouseenter={enter}
  onmouseleave={leave}
  onfocusin={enter}
  onfocusout={leave}
>
  {@render children()}
  {#if show}
    <span class="tipbubble {placement}" role="tooltip">{label}</span>
  {/if}
</span>

<style>
  .tipwrap {
    position: relative;
    display: inline-flex;
  }
  /* 暗底 + 渐变描边（视觉基线 §一 悬停提示行），延迟 300ms 出现 */
  .tipbubble {
    position: absolute;
    left: 50%;
    transform: translateX(-50%);
    padding: 5px 10px;
    border-radius: 8px;
    font-size: 12px;
    line-height: 1.4;
    color: var(--phx-text);
    white-space: nowrap;
    pointer-events: none;
    z-index: var(--phx-z-overlay);
    border: 1px solid transparent;
    background:
      linear-gradient(var(--phx-surface-2), var(--phx-surface-2)) padding-box,
      linear-gradient(135deg, rgba(127, 127, 127, 0.30), rgba(127, 127, 127, 0.10)) border-box;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
    animation: tip-in var(--phx-dur-fast) var(--phx-ease-out);
  }
  .tipbubble.top {
    bottom: calc(100% + 8px);
  }
  .tipbubble.bottom {
    top: calc(100% + 8px);
  }
  @keyframes tip-in {
    from {
      opacity: 0;
      transform: translateX(-50%) translateY(2px);
    }
    to {
      opacity: 1;
      transform: translateX(-50%) translateY(0);
    }
  }
</style>
