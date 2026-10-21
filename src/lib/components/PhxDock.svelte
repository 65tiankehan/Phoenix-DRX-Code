<script lang="ts">
  import { page } from '$app/state'
  import { resolve } from '$app/paths'
  import { t } from '$lib/i18n/i18n.svelte'
  import { playSound } from '$lib/sound'
  import { NAV, navIndex } from '$lib/nav'

  /** 当前激活项序号（与 NAV 顺序一一对应，角色域 / 仓库域子页已归并）；-1 = 未命中 */
  const onIdx = $derived(navIndex(page.url.pathname))

  let wrap = $state<HTMLElement>()
  let links: HTMLAnchorElement[] = []
  /** 滑块几何：null = 未完成首测（不渲染，插入即终态，路由首屏无从 0 滑入的跳变） */
  let ind = $state<{ left: number; width: number } | null>(null)

  function measure(): void {
    const el = onIdx >= 0 ? links[onIdx] : undefined
    if (!el) {
      ind = null
      return
    }
    ind = { left: el.offsetLeft, width: el.offsetWidth }
  }

  // 激活项变化（路由切换）时重测定位（measure 内同步读取自动追踪）
  $effect(() => {
    measure()
  })

  // dock 尺寸变化（窗口缩放等）时防止滑块漂移
  $effect(() => {
    if (!wrap) return
    const ro = new ResizeObserver(measure)
    ro.observe(wrap)
    return () => ro.disconnect()
  })

  /** dock 切页音效：仅路由真正变化时响（感官层基线 §一映射表 nav 语义） */
  function go(target: string): void {
    if (page.url.pathname !== target) playSound('nav')
  }

  /** 鼠标跟随光源（nav 层事件委托）：写入相对坐标供链接 ::after 光斑定位 */
  function spotlight(e: MouseEvent): void {
    const el = (e.target as HTMLElement).closest('a')
    if (!el) return
    const r = el.getBoundingClientRect()
    el.style.setProperty('--mx', `${e.clientX - r.left}px`)
    el.style.setProperty('--my', `${e.clientY - r.top}px`)
  }
</script>

<nav class="dock" bind:this={wrap} onmousemove={spotlight}>
  <div class="dock-logo">P</div>
  <div class="dock-sep"></div>

  {#if ind}
    <div
      class="ind"
      style:transform={`translateX(${ind.left}px)`}
      style:width={`${ind.width}px`}
    ></div>
  {/if}

  {#each NAV as item, i (item.path)}
    <a
      href={resolve(item.path)}
      class:on={navIndex(item.path) === onIdx}
      onclick={() => go(item.path)}
      bind:this={links[i]}
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        {#each item.icon as d (d)}<path d={d} />{/each}
      </svg>
      <span class="lbl">{t(item.key)}</span>
    </a>
  {/each}
</nav>

<style>
  .dock {
    position: absolute;
    left: 50%;
    bottom: 18px;
    transform: translateX(-50%);
    z-index: var(--phx-z-header);
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 5px;
    background: var(--phx-glass);
    border-radius: 13px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45), 0 0 0 0.5px var(--phx-sep-strong);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
  }

  .dock-logo {
    width: 30px;
    height: 30px;
    border-radius: 8px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--phx-font-brand);
    font-style: italic;
    font-weight: 700;
    font-size: 17px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }

  .dock-sep {
    width: 0.5px;
    height: 22px;
    background: rgba(255, 255, 255, 0.16);
    margin: 0 3px;
  }

  .dock a {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    min-width: 50px;
    height: 42px;
    border-radius: 9px;
    text-decoration: none;
    overflow: hidden;
    transition:
      background var(--phx-dur-fast) var(--phx-ease-out),
      filter var(--phx-dur-fast) var(--phx-ease-out);
  }

  .dock a:active {
    filter: brightness(0.92);
  }

  .dock a svg {
    width: 19px;
    height: 19px;
    color: var(--phx-text-2);
    transition: color var(--phx-dur-fast) var(--phx-ease-out);
  }

  .dock a .lbl {
    font-size: 10px;
    font-weight: 500;
    color: var(--phx-text-2);
    letter-spacing: 0.02em;
    transition:
      color var(--phx-dur-fast) var(--phx-ease-out),
      font-weight var(--phx-dur-fast) var(--phx-ease-out);
  }

  .dock a:hover {
    background: var(--phx-fill-quaternary);
  }

  /* hover 光源（跟随鼠标的光斑，与 PhxSegmented 同款规格）：--mx/--my 由 nav 委托 spotlight 写入 */
  .dock a::after {
    content: '';
    position: absolute;
    inset: 0;
    pointer-events: none;
    border-radius: inherit;
    background: radial-gradient(
      160px circle at var(--mx, 50%) var(--my, 50%),
      rgba(255, 255, 255, 0.08),
      transparent 60%
    );
    opacity: 0;
    transition: opacity var(--phx-dur-norm) var(--phx-ease-out);
  }
  .dock a:hover::after {
    opacity: 1;
  }

  /* 液体滑块：{#if} 插入即终态；仅路由切换时产生过渡（--phx-dur-norm + emphasized，感官层基线档位） */
  .ind {
    position: absolute;
    top: 5px;
    bottom: 5px;
    left: 0;
    border-radius: 9px;
    background: var(--phx-fill-secondary);
    pointer-events: none;
    transition:
      transform var(--phx-dur-norm) var(--phx-ease-emphasized),
      width var(--phx-dur-norm) var(--phx-ease-emphasized);
  }

  .dock a.on svg {
    color: var(--phx-accent);
  }

  .dock a.on .lbl {
    color: var(--phx-text);
    font-weight: 600;
  }
</style>
