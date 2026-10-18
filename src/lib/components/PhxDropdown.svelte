<script module lang="ts">
  export interface DdOption {
    value: string
    label: string
  }
</script>

<script lang="ts">
  import { tick } from 'svelte'

  let {
    value = $bindable(),
    options,
    label = '',
    disabled = false,
    bare = false,
    onchange
  }: {
    /** 当前选中值 */
    value: string
    options: DdOption[]
    label?: string
    disabled?: boolean
    /** 无框触发态：透明无边距按钮，字号字体继承父级（02 稿图 hero 档案 ID 行） */
    bare?: boolean
    /** 选中项变化回调（受控用法：配合 value 单向绑定） */
    onchange?: (v: string) => void
  } = $props()

  let open = $state(false)
  let rootEl: HTMLDivElement | undefined
  let btnEl: HTMLButtonElement | undefined
  let menuEl = $state<HTMLDivElement | undefined>(undefined)
  let focusIndex = $state(-1)

  const current = $derived(options.find((o) => o.value === value))

  function toggle(): void {
    if (!disabled) open = !open
  }

  function close(): void {
    open = false
    btnEl?.focus()
  }

  function choose(v: string): void {
    value = v
    open = false
    focusIndex = -1
    onchange?.(v)
  }

  function focusItem(i: number): void {
    const els = menuEl?.querySelectorAll<HTMLElement>('.dd-item')
    if (!els || els.length === 0) return
    focusIndex = (i + els.length) % els.length
    els[focusIndex].focus()
  }

  function onBtnKeydown(e: KeyboardEvent): void {
    if (disabled) return
    if (e.key === 'Escape' && open) {
      e.preventDefault()
      close()
    } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      if (!open) open = true
      void tick().then(() => focusItem(e.key === 'ArrowDown' ? 0 : options.length - 1))
    }
  }

  function onMenuKeydown(e: KeyboardEvent): void {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      focusItem(focusIndex + 1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      focusItem(focusIndex - 1)
    } else if (e.key === 'Escape') {
      e.preventDefault()
      close()
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      const opt = options[focusIndex]
      if (opt) choose(opt.value)
    }
  }

  function onDocClick(e: MouseEvent): void {
    if (open && rootEl && !rootEl.contains(e.target as Node)) open = false
  }
</script>

<svelte:window onclick={onDocClick} />

<div bind:this={rootEl} class="dd" class:bare>
  <button
    type="button"
    bind:this={btnEl}
    class="dd-btn"
    {disabled}
    aria-label={label || undefined}
    aria-haspopup="listbox"
    aria-expanded={open}
    onclick={toggle}
    onkeydown={onBtnKeydown}
  >
    <span class="dd-val">{current?.label ?? value}</span>
    <svg class="arr" viewBox="0 0 8 8">
      <path d="M1 2.5 4 5.5 7 2.5" stroke="currentColor" fill="none" stroke-linecap="round" />
    </svg>
  </button>
  {#if open}
    <div bind:this={menuEl} class="dd-menu" role="listbox" tabindex="-1" onkeydown={onMenuKeydown}>
      {#each options as o (o.value)}
        <!-- 键盘操作（上下/Enter/空格）由菜单容器 onMenuKeydown 统一处理，事件冒泡至此生效 -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="dd-item"
          class:on={o.value === value}
          role="option"
          aria-selected={o.value === value}
          tabindex="-1"
          onclick={() => choose(o.value)}
        >
          {o.label}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  /* 15 屏稿图原值：触发钮 + 浮层菜单 */
  .dd {
    position: relative;
    flex: none;
  }
  .dd-btn {
    height: 30px;
    min-width: 190px;
    padding: 0 10px 0 12px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    background: var(--phx-fill-tertiary);
    border: none;
    border-radius: var(--phx-r-btn);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
    font-size: 12.5px;
    color: var(--phx-text);
    font-family: inherit;
  }
  .dd-btn:hover:not(:disabled) {
    background: var(--phx-fill-quaternary);
  }
  .dd-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .dd-btn .arr {
    width: 8px;
    height: 8px;
    color: var(--phx-text-3);
    flex: none;
  }
  .dd-menu {
    position: absolute;
    right: 0;
    top: calc(100% + 6px);
    min-width: 100%;
    z-index: var(--phx-z-overlay);
    background: var(--phx-surface-2);
    border-radius: 10px;
    padding: 4px;
    box-shadow:
      0 12px 32px rgba(0, 0, 0, 0.5),
      0 0 0 0.5px var(--phx-sep-strong);
  }
  .dd-item {
    padding: 8px 12px;
    border-radius: 6px;
    font-size: 12.5px;
    cursor: pointer;
    white-space: nowrap;
  }
  .dd-item:hover {
    background: var(--phx-fill-tertiary);
  }
  .dd-item.on {
    color: var(--phx-accent);
    background: var(--phx-accent-dim);
    font-weight: 600;
  }
  /* bare 无框触发态：透明无边距，字号字体继承父级（02 稿图 hero 档案 ID 行） */
  .dd.bare .dd-btn {
    background: transparent;
    box-shadow: none;
    min-width: 0;
    height: auto;
    padding: 0;
    font: inherit;
    color: var(--phx-text);
  }
  .dd.bare .dd-btn:hover:not(:disabled) {
    background: transparent;
  }
  .dd.bare .dd-menu {
    left: 0;
    right: auto;
  }
</style>
