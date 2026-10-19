<script lang="ts">
  import { onMount } from 'svelte'
  import { emit, listen } from '@tauri-apps/api/event'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { phxServer, phxProfile, phxTray } from '$lib/ipc'
  import type { PhxProfileMeta, PhxServerRow } from '$lib/ipc'
  import { t } from '$lib/i18n/i18n.svelte'

  let server = $state<PhxServerRow | null>(null)
  let profiles = $state<PhxProfileMeta[]>([])
  let current = $state<string | null>(null)

  /** 面板内容高度（bind:clientHeight 测量），上报 Rust 换算窗高（.wrap 上下边距计入窗口口径） */
  let panelH = $state(0)
  const WRAP_PAD = 16 // .wrap 上下 padding 合计（8px × 2）

  $effect(() => {
    if (panelH > 0) void phxTray.reportHeight(panelH + WRAP_PAD)
  })

  /** 当前存档名（列表内匹配；匹配不到显示「未选择」） */
  const currentName = $derived(profiles.find((p) => p.id === current)?.nickname ?? null)

  async function reload(): Promise<void> {
    ;[server, profiles, current] = await Promise.all([
      phxServer.current(),
      phxProfile.list(),
      phxProfile.current()
    ])
  }

  async function useProfile(id: string): Promise<void> {
    if (id === current) {
      void phxTray.hide()
      return
    }
    await phxProfile.use(id)
    current = id
    // 广播切档事件（首页监听后自动刷新），再收起面板
    await emit('tray:profile-switched')
    void phxTray.hide()
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === 'Escape') void phxTray.hide()
  }

  onMount(() => {
    void reload()
    const open = listen('tray:open', () => void reload())
    const focus = getCurrentWindow().onFocusChanged(({ payload }) => {
      if (payload) void reload()
    })
    return () => {
      void open.then((fn) => fn())
      void focus.then((fn) => fn())
    }
  })
</script>

<svelte:window onkeydown={onKey} />

<div class="wrap">
  <div class="panel" bind:clientHeight={panelH}>
    <button class="item" onclick={() => void phxTray.showMain()}>{t('tray.show')}</button>

    <div class="sep"></div>

    <div class="row">
      <span class="label">{t('tray.server')}</span>
      <span class="value">{server?.name ?? t('tray.unselected')}</span>
    </div>
    <div class="row">
      <span class="label">{t('tray.profile')}</span>
      <span class="value">{currentName ?? t('tray.unselected')}</span>
    </div>

    <div class="sep"></div>

    <div class="head">{t('tray.switch')}</div>
    {#if profiles.length === 0}
      <div class="empty">{t('tray.empty')}</div>
    {:else}
      <div class="list">
        {#each profiles as p (p.id)}
          <button
            class="item"
            class:active={p.id === current}
            onclick={() => void useProfile(p.id)}
          >
            <span class="pname">{p.nickname}</span>
            <span class="pmeta">{t('tray.meta', { n: p.level, side: p.side })}</span>
            {#if p.id === current}
              <svg class="check" viewBox="0 0 16 16" aria-hidden="true">
                <path
                  d="M3.5 8.5l3 3 6-7"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                />
              </svg>
            {/if}
          </button>
        {/each}
      </div>
    {/if}

    <div class="sep"></div>

    <button class="item" disabled={!server} onclick={() => void phxTray.openProfilesDir()}>
      {t('tray.openDir')}
    </button>

    <div class="sep"></div>

    <button class="item" onclick={() => void phxTray.quit()}>{t('tray.quit')}</button>
  </div>
</div>

<style>
  .wrap {
    width: 100%;
    height: 100vh;
    padding: 8px; /* 阴影呼吸边距（Rust 侧窗口宽度 264 含此边距） */
    box-sizing: border-box;
  }

  .panel {
    display: flex;
    flex-direction: column;
    /* 高度由内容撑开（测量后上报 Rust 调窗高），禁止 min-height 撑满窗口——
       那会让首次测量值虚高且永远收不回去，底部出现死空间 */
    padding: 6px;
    /* 实心卡片底：透明窗口里 backdrop-filter 糊不到窗外桌面，
       玻璃拟态在此不可用（会透出桌面内容），见视觉基线「托盘菜单」行 */
    background: var(--phx-surface-2);
    border: 1px solid var(--phx-sep);
    border-radius: var(--phx-r-card);
    box-shadow: var(--phx-shadow-win);
    color: var(--phx-text);
    user-select: none;
    -webkit-user-select: none;
  }

  .item {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 9px 10px;
    border: 0;
    border-radius: var(--phx-r-btn);
    background: transparent;
    color: var(--phx-text);
    font-family: inherit;
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    transition:
      background var(--phx-dur-fast) var(--phx-ease-out),
      color var(--phx-dur-fast) var(--phx-ease-out),
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }

  .item:hover {
    background: var(--phx-fill-tertiary);
  }

  .item:active {
    background: var(--phx-fill-quaternary);
  }

  .item:disabled {
    color: var(--phx-text-3);
    cursor: default;
  }

  .item:disabled:hover {
    background: transparent;
  }

  .item.active {
    background: var(--phx-accent-dim);
  }

  .sep {
    flex: none;
    height: 1px;
    margin: 4px 6px;
    background: var(--phx-sep);
  }

  .row {
    display: flex;
    flex: none;
    align-items: baseline;
    gap: 8px;
    padding: 4px 10px;
  }

  .label {
    flex: none;
    font-size: 12px;
    color: var(--phx-text-3);
  }

  .value {
    overflow: hidden;
    font-size: 12px;
    color: var(--phx-text-2);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .head {
    flex: none;
    padding: 6px 10px 2px;
    font-size: 11px;
    color: var(--phx-text-3);
  }

  .empty {
    flex: none;
    padding: 6px 10px 8px;
    font-size: 12px;
    color: var(--phx-text-3);
  }

  .list {
    display: flex;
    flex: none;
    flex-direction: column;
    max-height: 316px;
    overflow-y: auto;
  }

  .pname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pmeta {
    flex: none;
    font-size: 11px;
    color: var(--phx-text-3);
  }

  .check {
    flex: none;
    width: 14px;
    height: 14px;
    color: var(--phx-accent);
  }
</style>
