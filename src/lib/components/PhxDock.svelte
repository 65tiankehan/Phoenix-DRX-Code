<script lang="ts">
  import { page } from '$app/state'
  import { resolve } from '$app/paths'
  import { t } from '$lib/i18n/i18n.svelte'
  import { playSound } from '$lib/sound'

  /** 角色域子页：dock「角色」项按分区高亮（技能 / 健康 / 检视等角色域屏） */
  const CHARACTER_SECTION = ['/character', '/skills', '/health']

  /** 仓库域子页：dock「仓库」项按分区高亮（Scav 装备与 PMC 装备同款界面，也归仓库域） */
  const STASH_SECTION = ['/stash', '/inspect', '/scav']

  const charOn = $derived(CHARACTER_SECTION.includes(page.url.pathname))
  const stashOn = $derived(STASH_SECTION.includes(page.url.pathname))

  /** 当前激活 dock 项序号（与模板中 a 的书写顺序一一对应）；-1 = 未命中任何项 */
  const onIdx = $derived.by(() => {
    const p = page.url.pathname
    if (p === '/home') return 0
    if (charOn) return 1
    if (p === '/quests') return 2
    if (p === '/traders') return 3
    if (p === '/hideout') return 4
    if (stashOn) return 5
    if (p === '/presets') return 6
    if (p === '/backup') return 7
    if (p === '/settings') return 8
    if (p === '/about') return 9
    return -1
  })

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

  <a
    href={resolve('/home')}
    class:on={page.url.pathname === '/home'}
    onclick={() => go('/home')}
    bind:this={links[0]}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4 11.5 12 4l8 7.5" /><path d="M6.5 10.5V20h11v-9.5" /></svg>
    <span class="lbl">{t('nav.home')}</span>
  </a>

  <a
    href={resolve('/character')}
    class:on={charOn}
    onclick={() => go('/character')}
    bind:this={links[1]}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="3.6" /><path d="M5 20c1.4-3.6 4-5 7-5s5.6 1.4 7 5" /></svg>
    <span class="lbl">{t('nav.character')}</span>
  </a>

  <a
    href={resolve('/quests')}
    class:on={page.url.pathname === '/quests'}
    onclick={() => go('/quests')}
    bind:this={links[2]}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4 6l1.4 1.4L8 4.8" /><path d="M4 12l1.4 1.4L8 10.8" /><path d="M4 18l1.4 1.4L8 16.8" /><path d="M11.5 6H20" /><path d="M11.5 12H20" /><path d="M11.5 18H20" /></svg>
    <span class="lbl">{t('nav.quests')}</span>
  </a>

  <a
    href={resolve('/traders')}
    class:on={page.url.pathname === '/traders'}
    onclick={() => go('/traders')}
    bind:this={links[3]}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M6 8h12l-1 12H7L6 8Z" /><path d="M9 8V6.5a3 3 0 0 1 6 0V8" /></svg>
    <span class="lbl">{t('nav.traders')}</span>
  </a>

  <a
    href={resolve('/hideout')}
    class:on={page.url.pathname === '/hideout'}
    onclick={() => go('/hideout')}
    bind:this={links[4]}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3.5 11 12 3.5 20.5 11" /><path d="M5.5 9.5V20h13V9.5" /><path d="M9.5 20v-5.5h5V20" /></svg>
    <span class="lbl">{t('nav.hideout')}</span>
  </a>

  <a
    href={resolve('/stash')}
    class:on={stashOn}
    onclick={() => go('/stash')}
    bind:this={links[5]}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="7.5" width="17" height="12.5" rx="1" /><path d="M3.5 11.5h17" /><path d="M9 7.5V4h6v3.5" /></svg>
    <span class="lbl">{t('nav.stash')}</span>
  </a>

  <a
    href={resolve('/presets')}
    class:on={page.url.pathname === '/presets'}
    onclick={() => go('/presets')}
    bind:this={links[6]}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3.6l2.5 5.2 5.7.8-4.1 4 1 5.7-5.1-2.7-5.1 2.7 1-5.7-4.1-4 5.7-.8z" /></svg>
    <span class="lbl">{t('nav.presets')}</span>
  </a>

  <a
    href={resolve('/backup')}
    class:on={page.url.pathname === '/backup'}
    onclick={() => go('/backup')}
    bind:this={links[7]}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 3v5h5" /><path d="M3.05 13A9 9 0 1 0 6 5.3L3 8" /><path d="M12 7v5l4 2" /></svg>
    <span class="lbl">{t('nav.backup')}</span>
  </a>

  <a
    href={resolve('/settings')}
    class:on={page.url.pathname === '/settings'}
    onclick={() => go('/settings')}
    bind:this={links[8]}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" /><circle cx="12" cy="12" r="3" /></svg>
    <span class="lbl">{t('nav.settings')}</span>
  </a>

  <a
    href={resolve('/about')}
    class:on={page.url.pathname === '/about'}
    onclick={() => go('/about')}
    bind:this={links[9]}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10" /><path d="M12 16v-4" /><path d="M12 8h.01" /></svg>
    <span class="lbl">{t('nav.about')}</span>
  </a>
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
    padding: 6px;
    background: var(--phx-glass);
    border-radius: 14px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45), 0 0 0 0.5px var(--phx-sep-strong);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
  }

  .dock-logo {
    width: 34px;
    height: 34px;
    border-radius: 9px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--phx-font-brand);
    font-style: italic;
    font-weight: 700;
    font-size: 19px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }

  .dock-sep {
    width: 0.5px;
    height: 26px;
    background: rgba(255, 255, 255, 0.16);
    margin: 0 4px;
  }

  .dock a {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    min-width: 56px;
    height: 46px;
    border-radius: 10px;
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
    width: 22px;
    height: 22px;
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
    top: 6px;
    bottom: 6px;
    left: 0;
    border-radius: 10px;
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
