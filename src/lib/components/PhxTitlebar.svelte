<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { t } from '$lib/i18n/i18n.svelte'
  import PhxTooltip from './PhxTooltip.svelte'
  import { announcements, toggleAnnouncements } from '$lib/announcements.svelte'
  import { toggleCmdk } from '$lib/cmdk.svelte'

  const appWindow = getCurrentWindow()
  let maximized = $state(false)

  /** 铃铛未读点：有任一未读公告时亮起（关闭面板即全量标记已读，点随之熄灭） */
  const hasUnread = $derived(announcements.list.some((a) => !announcements.readIds.has(a.id)))

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
  <span class="tb-label" data-tauri-drag-region>Phoenix</span>
  <div class="capbtns">
    <!-- 20 稿图场景 3：通知按钮左侧搜索按钮，悬停提示「全局搜索 · Ctrl+K」 -->
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
    height: 40px;
    z-index: var(--phx-z-header);
    display: flex;
    align-items: center;
    padding: 0 0 0 18px;
  }

  .tb-label {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--phx-text-2);
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
    background: rgba(255, 255, 255, 0.06);
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
