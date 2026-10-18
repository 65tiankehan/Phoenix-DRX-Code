<script lang="ts">
  import type { Snippet } from 'svelte'

  let {
    checked = $bindable(false),
    label = '',
    disabled = false,
    children
  }: {
    /** 勾选状态 */
    checked?: boolean
    /** 复选框旁的文字标签 */
    label?: string
    disabled?: boolean
    children?: Snippet
  } = $props()

  function toggle(): void {
    if (!disabled) checked = !checked
  }

  function onKeydown(e: KeyboardEvent): void {
    if (disabled) return
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      toggle()
    }
  }
</script>

<div
  class="ckwrap"
  class:disabled
  role="checkbox"
  aria-checked={checked}
  aria-label={label || undefined}
  aria-disabled={disabled || undefined}
  tabindex={disabled ? -1 : 0}
  onclick={toggle}
  onkeydown={onKeydown}
>
  <span class="ck" class:on={checked}></span>
  {#if label}
    <span class="cklab">{label}</span>
  {/if}
  {@render children?.()}
</div>

<style>
  /* 13/08 屏稿图原值：20×20 复选框 + 11.5px 标签 */
  .ckwrap {
    display: flex;
    align-items: center;
    gap: 7px;
    cursor: pointer;
  }
  .ck {
    flex: none;
    width: 20px;
    height: 20px;
    border-radius: 6px;
    border: 0.5px solid var(--phx-sep-strong);
    background: transparent;
    position: relative;
    transition:
      background var(--phx-dur-fast),
      border-color var(--phx-dur-fast);
  }
  .ck.on {
    background: var(--phx-accent);
    border-color: var(--phx-accent);
  }
  .ck.on::after {
    content: '';
    position: absolute;
    left: 6px;
    top: 2.5px;
    width: 5px;
    height: 9px;
    border-right: 2px solid #fff;
    border-bottom: 2px solid #fff;
    transform: rotate(45deg);
  }
  .cklab {
    flex: none;
    font-size: 11.5px;
    color: var(--phx-text-2);
  }
  /* 三态（视觉基线 §四） */
  .ckwrap:not(.disabled):hover .ck:not(.on) {
    background: var(--phx-fill-quaternary);
  }
  .ckwrap:not(.disabled):hover .ck.on {
    filter: brightness(1.1);
  }
  .ckwrap:not(.disabled):active .ck {
    filter: brightness(0.92);
  }
  .ckwrap.disabled {
    cursor: default;
  }
  .ckwrap.disabled .ck {
    opacity: 0.45;
  }
</style>
