<script lang="ts">
  import { fly } from 'svelte/transition'
  import { flip } from 'svelte/animate'
  import { cubicOut } from 'svelte/easing'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import { dismissToast, toastItems } from '$lib/toast.svelte'

  // 进出场时长/缓动对齐 token：--phx-dur-fast: 150ms + --phx-ease-out
  const dur = 150
  const easing = cubicOut

  /** 点动作：开外链并即清该条 toast */
  function openAction(id: number, url: string): void {
    void openUrl(url).catch(() => {})
    dismissToast(id)
  }
</script>

<div class="toast-region" aria-live="polite">
  {#each toastItems as item (item.id)}
    <div
      class="toast {item.kind}"
      transition:fly={{ y: -6, duration: dur, easing }}
      animate:flip={{ duration: dur }}
    >
      <span class="dot"></span>{item.text}
      {#if item.action}
        {@const act = item.action}
        <button class="act" type="button" onclick={() => openAction(item.id, act.url)}>
          {act.label}
        </button>
      {/if}
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
    /* region 整体穿透，但卡片内动作按钮要可点 */
    pointer-events: auto;
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
  .toast.info .dot {
    background: var(--phx-accent);
  }
  .act {
    flex: none;
    border: 0;
    background: none;
    padding: 0;
    font: inherit;
    font-size: 12.5px;
    color: var(--phx-accent);
    cursor: pointer;
  }
  .act:hover {
    text-decoration: underline;
  }
</style>
