<script lang="ts">
  import { fly } from 'svelte/transition'
  import { flip } from 'svelte/animate'
  import { cubicOut } from 'svelte/easing'
  import { toastItems } from '$lib/toast.svelte'

  // 进出场时长/缓动对齐 token：--phx-dur-fast: 150ms + --phx-ease-out
  const dur = 150
  const easing = cubicOut
</script>

<div class="toast-region" aria-live="polite">
  {#each toastItems as item (item.id)}
    <div
      class="toast {item.kind}"
      transition:fly={{ y: -6, duration: dur, easing }}
      animate:flip={{ duration: dur }}
    >
      <span class="dot"></span>{item.text}
    </div>
  {/each}
</div>

<style>
  /* 右上角可堆叠轻提示（视觉基线 §一），视觉沿用稿图玻璃卡片 */
  .toast-region {
    position: fixed;
    top: 48px;
    right: 14px;
    z-index: var(--phx-z-toast);
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 8px;
    pointer-events: none;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--phx-glass);
    backdrop-filter: blur(20px);
    border: 0.5px solid var(--phx-sep-strong);
    border-radius: 10px;
    padding: 8px 16px;
    font-size: 12.5px;
    color: var(--phx-text);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }
  .dot {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }
  .toast.success .dot {
    background: var(--phx-green);
  }
  .toast.warning .dot {
    background: var(--phx-warn);
  }
  .toast.error .dot {
    background: var(--phx-danger);
  }
</style>
