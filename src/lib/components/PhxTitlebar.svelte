<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { page } from '$app/state'
  import { resolve } from '$app/paths'
  import { NAV, navIndex } from '$lib/nav'
  import { oftenVisited, markFromTab } from '$lib/recent.svelte'
  import { playSound } from '$lib/sound'
  import { t } from '$lib/i18n/i18n.svelte'
  import PhxTooltip from './PhxTooltip.svelte'
  import { announcements, toggleAnnouncements } from '$lib/announcements.svelte'
  import { toggleCmdk } from '$lib/cmdk.svelte'

  const appWindow = getCurrentWindow()
  let maximized = $state(false)

  /** 当前激活导航项序号（与 dock 同一归并口径；-1 = 非导航屏） */
  const onIdx = $derived(navIndex(page.url.pathname))

  /** 存档选择屏（01）：全窗独立屏、无 dock，顶栏只保留品牌与窗口键，不挂常去组 */
  const bare = $derived(page.route.id === '/server-select')

  /** 常去前三（频次排序，真实访问记录；记录为空时不渲染整组） */
  const often = $derived(
    oftenVisited()
      .map((r) => NAV.find((n) => n.path === r.path))
      .filter((n): n is (typeof NAV)[number] => n !== undefined)
  )

  /* 常去组液体滑块（与 dock 同款口径）：测激活 tab 的 offsetLeft/Width，路由切换时滑动过渡 */
  let grp = $state<HTMLElement>()
  let tabs: HTMLAnchorElement[] = []
  /** 激活 tab 下标；-1 = 当前页不在常去组 */
  const onTab = $derived(often.findIndex((n) => navIndex(n.path) === onIdx))
  /** 滑块几何：null = 未命中或未首测（不渲染，插入即终态，避免从 0 滑入的跳变） */
  let tind = $state<{ left: number; width: number } | null>(null)

  function measureTab(): void {
    const el = onTab >= 0 ? tabs[onTab] : undefined
    if (!el || !grp) {
      tind = null
      return
    }
    // 用相对 grp 的矩形计算，避免 tooltip 包裹层（span.tipwrap）改变 offsetLeft 参照
    const g = grp.getBoundingClientRect()
    const r = el.getBoundingClientRect()
    tind = { left: r.left - g.left, width: r.width }
  }

  // 激活项 / 常去序列变化（路由切换、项增删换序）时重测定位（onTab 读取自动追踪）
  $effect(() => {
    measureTab()
  })

  // 组尺寸变化（窗口缩放等）时防止滑块漂移
  $effect(() => {
    if (!grp) return
    const ro = new ResizeObserver(measureTab)
    ro.observe(grp)
    return () => ro.disconnect()
  })

  /** 顶栏 tab 切页音效：仅路由真正变化时响；同时打「顶栏点入」标记（打分权重更高） */
  function goNav(target: string): void {
    if (page.url.pathname !== target) {
      playSound('nav')
      markFromTab(target)
    }
  }

  /** 铃铛未读点：有任一未读公告/小贴士时亮起；推广永不点亮（关闭面板即全量标记已读，点随之熄灭） */
  const hasUnread = $derived(
    announcements.list.some((a) => a.kind !== 'sponsor' && !announcements.readIds.has(a.id))
  )

  $effect(() => {
    let unlisten: (() => void) | null = null
    let disposed = false

    const sync = (): void => {
      void appWindow.isMaximized().then((v) => {
        maximized = v
      })
    }
    sync()
    void appWindow.onResized(sync).then((fn) => {
      if (disposed) fn()
      else unlisten = fn
    })

    return () => {
      disposed = true
      unlisten?.()
    }
  })
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <span class="brand-dot" data-tauri-drag-region></span>
    <span class="tb-label" data-tauri-drag-region>Phoenix</span>
    {#if !bare}
      <span class="brand-sep" data-tauri-drag-region></span>
      <span class="tb-tag" data-tauri-drag-region>{t('app.subtitle')}</span>
    {/if}
  </div>

  <!-- 中间：常去 tab 组（频次排序，稿图 23；与 dock 共用 navIndex 归并口径）。
       存档选择屏与「少于 2 项」不渲染：前者是全窗独立屏（无 dock 语境），
       后者只有当前页一项时没有跳转价值，只会重复 dock。 -->
  {#if !bare && often.length > 1}
    <nav class="tnav" aria-label={t('nav.often')}>
      <div class="grp" bind:this={grp}>
        {#if tind}
          <div
            class="tind"
            style:transform={`translateX(${tind.left}px)`}
            style:width={`${tind.width}px`}
          ></div>
        {/if}
        <span class="grp-cap" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 3v5h5" />
            <path d="M3.05 13A9 9 0 1 0 6 5.3L3 8" />
            <path d="M12 7.5V12l3.5 2" />
          </svg>
          <span class="grp-txt">{t('nav.often')}</span>
        </span>
        <span class="grp-sep" aria-hidden="true"></span>
        {#each often as item, i (item.path)}
          <PhxTooltip label={t(item.key)} placement="bottom">
            <a
              class="ttab"
              class:on={navIndex(item.path) === onIdx}
              href={resolve(item.path)}
              onclick={() => goNav(item.path)}
              bind:this={tabs[i]}
            >
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                {#each item.icon as d (d)}<path d={d} />{/each}
              </svg>
              <span>{t(item.key)}</span>
            </a>
          </PhxTooltip>
        {/each}
      </div>
    </nav>
  {/if}

  <div class="capbtns">
    <!-- 20 稿图场景 3：通知按钮左侧搜索按钮，悬停提示「全局搜索 · Ctrl+K」 -->
    <PhxTooltip label={t('nav.assistant')} placement="bottom">
      <a
        class="searchbtn"
        href={resolve('/assistant')}
        aria-label={t('nav.assistant')}
      >
        <svg
          viewBox="0 0 15 15"
          fill="none"
          stroke="currentColor"
          stroke-width="1.3"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path d="M7.5 1.8 8.8 4.7 12 5.1 9.7 7.3 10.3 10.5 7.5 9 4.7 10.5 5.3 7.3 3 5.1 6.2 4.7Z" />
          <path d="M12.2 11.2 13.5 12.5" />
        </svg>
      </a>
    </PhxTooltip>
    <PhxTooltip label={`${t('cmdk.search')} · Ctrl+K`} placement="bottom">
      <button
        type="button"
        class="searchbtn"
        aria-label={t('cmdk.search')}
        onclick={() => toggleCmdk()}
      >
        <svg
          viewBox="0 0 15 15"
          fill="none"
          stroke="currentColor"
          stroke-width="1.3"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <circle cx="6.6" cy="6.6" r="4.4" />
          <path d="M13.2 13.2 10 10" />
        </svg>
      </button>
    </PhxTooltip>
    <PhxTooltip label={t('announce.bell')} placement="bottom">
      <button
        type="button"
        class="bell"
        aria-label={t('announce.bell')}
        onclick={() => toggleAnnouncements()}
      >
        <svg
          viewBox="0 0 14 14"
          fill="none"
          stroke="currentColor"
          stroke-width="1.2"
          stroke-linecap="round"
          stroke-linejoin="round"
        >
          <path
            d="M7 1.6a3.9 3.9 0 0 0-3.9 3.9c0 3-1.2 4.2-1.2 4.2h10.2s-1.2-1.2-1.2-4.2A3.9 3.9 0 0 0 7 1.6Z"
          />
          <path d="M5.9 11.9a1.2 1.2 0 0 0 2.2 0" />
        </svg>
        {#if hasUnread}
          <span class="bell-dot"></span>
        {/if}
      </button>
    </PhxTooltip>
    <PhxTooltip label={t('cap.minimize')} placement="bottom">
      <button type="button" aria-label={t('cap.minimize')} onclick={() => void appWindow.minimize()}>
        <svg viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" fill="none" /></svg>
      </button>
    </PhxTooltip>
    <PhxTooltip label={maximized ? t('cap.restore') : t('cap.maximize')} placement="bottom">
      <button
        type="button"
        aria-label={maximized ? t('cap.restore') : t('cap.maximize')}
        onclick={() => void appWindow.toggleMaximize()}
      >
        {#if maximized}
          <svg viewBox="0 0 10 10">
            <rect x=".5" y="2.5" width="7" height="7" stroke="currentColor" fill="none" />
            <path d="M2.5 2.5V.5h7v7" stroke="currentColor" fill="none" />
          </svg>
        {:else}
          <svg viewBox="0 0 10 10">
            <rect x=".5" y=".5" width="9" height="9" stroke="currentColor" fill="none" />
          </svg>
        {/if}
      </button>
    </PhxTooltip>
    <PhxTooltip label={t('cap.close')} placement="bottom">
      <button type="button" class="cap-close" aria-label={t('cap.close')} onclick={() => void appWindow.close()}>
        <svg viewBox="0 0 10 10">
          <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" fill="none" />
        </svg>
      </button>
    </PhxTooltip>
  </div>
</header>

<style>
  .titlebar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    /* 44px（2026-10-22 用户反馈「常去组贴顶太近」）：组高 32px 居中后上下各 6px */
    height: 44px;
    z-index: var(--phx-z-header);
    display: flex;
    align-items: center;
    padding: 0 0 0 18px;
  }

  /* B 字标款（稿图 22 用户选定）：小方点 + 大写拉字距字标 + 副标 */
  .brand {
    display: flex;
    align-items: center;
    min-width: 0;
  }

  .brand-dot {
    width: 4px;
    height: 4px;
    border-radius: 1px;
    background: var(--phx-accent);
    margin-right: 10px;
    flex: none;
  }

  .tb-label {
    font-size: 11.5px;
    font-weight: 700;
    letter-spacing: 0.22em;
    text-transform: uppercase;
    color: var(--phx-text);
  }

  .brand-sep {
    width: 1px;
    height: 11px;
    background: var(--phx-sep-strong);
    margin: 0 10px;
    flex: none;
  }

  .tb-tag {
    font-size: 11px;
    color: var(--phx-text-3);
    letter-spacing: 0.02em;
    white-space: nowrap;
  }

  /* 中间常去 tab 组（稿图 23）：容器不接管指针，组本体才可交互 */
  .tnav {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
  }

  .tnav .grp {
    position: relative;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border-radius: 10px;
    background: var(--phx-fill-tertiary);
    box-shadow: 0 0 0 0.5px var(--phx-sep);
    pointer-events: auto;
  }

  .grp-cap {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 2px 0 7px;
    color: var(--phx-text-3);
    flex: none;
  }

  .grp-cap svg {
    width: 12px;
    height: 12px;
  }

  .grp-txt {
    font-size: 10px;
    letter-spacing: 0.04em;
  }

  .grp-sep {
    width: 0.5px;
    height: 10px;
    background: var(--phx-sep-strong);
    margin: 0 4px 0 6px;
    flex: none;
  }

  .ttab {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 11px;
    border-radius: 8px;
    font-size: 11px;
    font-weight: 500;
    color: var(--phx-text-2);
    text-decoration: none;
    white-space: nowrap;
    transition:
      background var(--phx-dur-fast) var(--phx-ease-out),
      color var(--phx-dur-fast) var(--phx-ease-out);
  }

  .ttab svg {
    width: 14px;
    height: 14px;
    flex: none;
    transition: color var(--phx-dur-fast) var(--phx-ease-out);
  }

  .ttab:hover {
    background: var(--phx-fill-quaternary);
    color: var(--phx-text);
  }

  /* 选中胶囊底色由 .tind 滑块承担（随路由滑动），tab 本体只管文字与图标态 */
  .ttab.on {
    color: var(--phx-text);
    font-weight: 600;
  }

  .ttab.on svg {
    color: var(--phx-accent);
  }

  .tind {
    position: absolute;
    top: 2px;
    bottom: 2px;
    left: 0;
    border-radius: 8px;
    background: var(--phx-fill-secondary);
    pointer-events: none;
    transition:
      transform var(--phx-dur-norm) var(--phx-ease-emphasized),
      width var(--phx-dur-norm) var(--phx-ease-emphasized);
  }

  .capbtns {
    display: flex;
    align-self: stretch;
    margin-left: auto;
  }

  /* 包 tooltip 后按钮需继续撑满标题栏高（span 为 PhxTooltip 根元素 tipwrap） */
  .capbtns :global(span) {
    align-self: stretch;
  }

  .capbtns button {
    width: 46px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--phx-text-2);
    background: none;
    border: 0;
    font: inherit;
    transition:
      background 0.12s,
      color 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
    cursor: default;
  }

  .capbtns button svg {
    width: 10px;
    height: 10px;
    display: block;
  }

  .capbtns button:hover {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
  }

  .capbtns button.cap-close:hover {
    background: #c42b1c;
    color: #fff;
  }

  /* 20 稿图场景 3：搜索按钮（铃铛左侧，与铃铛同款窄钮） */
  .capbtns button.searchbtn {
    width: 34px;
  }

  .capbtns button.searchbtn svg {
    width: 14px;
    height: 14px;
  }

  /* 18 稿图原值：公告铃铛（窗口三键左侧，34px 窄钮 + 14px 铃铛线稿 + 未读蓝点） */
  .capbtns button.bell {
    position: relative;
    width: 34px;
  }

  .capbtns button.bell svg {
    width: 14px;
    height: 14px;
  }

  .capbtns .bell-dot {
    position: absolute;
    top: 9px;
    right: 7px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--phx-accent);
    box-shadow: 0 0 0 2px var(--phx-win-bg);
  }
</style>
