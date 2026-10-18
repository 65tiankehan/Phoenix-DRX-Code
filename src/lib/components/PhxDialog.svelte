<script lang="ts">
  import { tick } from 'svelte'
  import { playSound } from '$lib/sound'
  import zhCN from '$lib/i18n/zh-CN.json'

  let {
    open = $bindable(false),
    heading,
    text = '',
    confirmText = zhCN['dlg.confirm'],
    cancelText = zhCN['dlg.cancel'],
    danger = false,
    onconfirm,
    oncancel
  }: {
    /** 是否显示弹层 */
    open?: boolean
    /** 弹层标题 */
    heading: string
    /** 弹层正文 */
    text?: string
    confirmText?: string
    cancelText?: string
    /** 确认键是否为危险操作（红色） */
    danger?: boolean
    onconfirm?: () => void
    oncancel?: () => void
  } = $props()

  let okBtn: HTMLButtonElement | undefined = $state()

  function confirm(): void {
    onconfirm?.()
    open = false
  }

  function cancel(): void {
    oncancel?.()
    open = false
  }

  function onKeydown(e: KeyboardEvent): void {
    if (!open) return
    if (e.key === 'Escape') {
      e.preventDefault()
      cancel()
    }
  }

  // 打开时播放弹出音并聚焦确认键（等 DOM 挂到 body 后再取）
  $effect(() => {
    if (open) {
      playSound('pop')
      void tick().then(() => okBtn?.focus())
    }
  })

  /** 挂到 body 下，避免被父级 overflow/z-index 裁剪（对齐旧 Teleport 语义） */
  function portal(node: HTMLElement): void {
    document.body.appendChild(node)
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <!-- 遮罩点击关闭是弹层标准交互；Esc 已由全局 keydown 覆盖 -->
  <div
    use:portal
    class="phx-mask"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) cancel()
    }}
  >
    <div class="dlg" role="alertdialog" aria-modal="true" aria-label={heading}>
      <div class="dlg-title">{heading}</div>
      {#if text}
        <div class="dlg-text">{text}</div>
      {/if}
      <div class="dlg-btns">
        <button type="button" class="dbtn" onclick={cancel}>{cancelText}</button>
        <button bind:this={okBtn} type="button" class="dbtn ok" class:danger onclick={confirm}>
          {confirmText}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  /* 15 屏稿图原值：遮罩 + 360px 暗色卡片 */
  .phx-mask {
    position: fixed;
    inset: 0;
    z-index: var(--phx-z-modal);
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .dlg {
    width: 360px;
    background: var(--phx-surface-2);
    border-radius: var(--phx-r-card);
    box-shadow:
      0 16px 48px rgba(0, 0, 0, 0.5),
      0 0 0 0.5px var(--phx-sep-strong);
    padding: 20px;
  }
  .dlg-title {
    font-size: 15px;
    font-weight: 700;
  }
  .dlg-text {
    font-size: 12.5px;
    color: var(--phx-text-2);
    margin-top: 8px;
    line-height: 1.6;
    white-space: pre-line;
  }
  .dlg-btns {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 18px;
  }
  .dbtn {
    height: 30px;
    padding: 0 16px;
    border: none;
    border-radius: var(--phx-r-btn);
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
  }
  .dbtn:hover {
    background: var(--phx-fill-quaternary);
  }
  .dbtn.ok {
    background: var(--phx-accent);
    color: #fff;
    box-shadow: none;
  }
  .dbtn.ok.danger {
    background: var(--phx-danger);
  }
  .dbtn.ok:hover {
    filter: brightness(1.15);
  }
</style>
