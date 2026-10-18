<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { getVersion } from '@tauri-apps/api/app'
  import { phxUpdate, PHX_UPDATE_EVENT } from '$lib/ipc'
  import type { PhxUpdateStatus } from '$lib/ipc'
  import { t } from '$lib/i18n/i18n.svelte'

  /**
   * 更新薄窗（/update-window，独立 WebviewWindow）。
   * 三态：available（发现新版本）→ downloading（下载中）→ downloaded（待重启）。
   * 状态单一来源 = Rust 广播；挂载先取快照再 listen，避免错过事件。
   * 透明窗口铁律：实心 --phx-win-bg 底，禁玻璃拟态。
   */

  let status = $state<PhxUpdateStatus>({ phase: 'idle' })
  let currentVer = $state('')

  /* 下载速度：chunk 事件差分 + 指数滑动平均（B/s） */
  let speed = $state(0)
  let lastSample = { t: 0, r: 0 }

  function sampleSpeed(received: number): void {
    const now = performance.now()
    if (lastSample.t === 0) {
      lastSample = { t: now, r: received }
      return
    }
    const dt = (now - lastSample.t) / 1000
    if (dt < 0.2) return // 200ms 采样一次，够稳
    const inst = (received - lastSample.r) / dt
    speed = inst > 0 ? (speed === 0 ? inst : speed * 0.7 + inst * 0.3) : 0
    lastSample = { t: now, r: received }
  }

  function applyStatus(s: PhxUpdateStatus): void {
    if (s.phase !== 'downloading') {
      speed = 0
      lastSample = { t: 0, r: 0 }
    } else {
      sampleSpeed(s.received)
    }
    status = s
  }

  onMount(() => {
    void getVersion().then((v) => {
      currentVer = v
    })
    void phxUpdate.getStatus().then(applyStatus)
    const un = listen<PhxUpdateStatus>(PHX_UPDATE_EVENT, (e) => applyStatus(e.payload))
    return () => {
      void un.then((fn) => fn())
    }
  })

  /** 下载中派生量 */
  const dl = $derived(
    status.phase === 'downloading'
      ? {
          received: status.received,
          total: status.total,
          pct: status.total && status.total > 0 ? (status.received / status.total) * 100 : 0
        }
      : null
  )

  /** 字节 → MB 显示（1024 进位） */
  function mb(bytes: number): string {
    return (bytes / 1048576).toFixed(1)
  }

  /** 速度显示（B/s → MB/s） */
  const speedText = $derived(speed > 0 ? `${mb(speed)} MB/s` : '—')
  /** 剩余时间（秒 → 约 N 秒；速度未知或 total 未知时不显示） */
  const etaText = $derived.by(() => {
    if (!dl || !dl.total || speed <= 0) return null
    const remain = Math.max(0, (dl.total - dl.received) / speed)
    return t('update.eta', { seconds: Math.ceil(remain) })
  })

  /** Release changelog 纯文本逐行 → 要点行（空行剔除，无富文本解析） */
  const noteLines = $derived(
    status.phase === 'available'
      ? status.notes.split(/\r?\n/).map((s) => s.trim()).filter(Boolean)
      : []
  )

  function closeWin(): void {
    void getCurrentWindow().close()
  }
  function startDownload(): void {
    void phxUpdate.download().catch(() => {})
  }
  function startInstall(): void {
    void phxUpdate.install().catch(() => {})
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape') closeWin()
  }}
/>

<div class="upwin">
  <div class="up-titlebar" data-tauri-drag-region>
    <span class="up-tb-label" data-tauri-drag-region>{t('update.winTitle')}</span>
  </div>

  {#if status.phase === 'available'}
    <!-- 状态一：发现新版本 -->
    <div class="up-body">
      <div class="up-head">
        <div class="up-logo">P</div>
        <div>
          <div class="up-h-title">{t('update.foundTitle')}</div>
          <div class="up-h-sub">{t('update.foundSub', { current: currentVer })}</div>
        </div>
      </div>
      <div class="up-vers">
        <span class="ver-chip ver-old">{currentVer}</span>
        <svg class="ver-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M13 6l6 6-6 6"/></svg>
        <span class="ver-chip ver-new">{status.version}</span>
      </div>
      {#if noteLines.length > 0}
        <div class="notes-card">
          <div class="nc-h">{t('update.notesTitle')}</div>
          {#each noteLines as line (line)}
            <div class="nc-li"><span class="nc-dot"></span><span>{line}</span></div>
          {/each}
        </div>
      {/if}
      <div class="up-actions">
        <button class="btn btn-ghost" type="button" onclick={() => void phxUpdate.dismiss()}>
          {t('update.later')}
        </button>
        <button class="btn btn-primary" type="button" onclick={startDownload}>
          {t('update.downloadNow')}
        </button>
      </div>
    </div>
  {:else if dl}
    <!-- 状态二：下载中（后台下载 = 关窗，Rust 下载任务独立于窗口继续） -->
    <div class="up-body">
      <div class="up-head">
        <div class="up-logo">P</div>
        <div>
          <div class="up-h-title">{t('update.dlTitle')}</div>
          <div class="up-h-sub">{t('update.dlSub')}</div>
        </div>
      </div>
      <div class="prog">
        <div class="prog-meta">
          <span class="prog-pct">{dl.pct > 0 ? `${Math.floor(dl.pct)}%` : '—'}</span>
          <span class="prog-speed">{speedText}</span>
        </div>
        <div class="prog-track"><div class="prog-fill" style="width: {dl.pct}%"></div></div>
        <div class="prog-sub">
          {dl.total ? `${mb(dl.received)} MB / ${mb(dl.total)} MB` : mb(dl.received) + ' MB'}
          {#if etaText}· {etaText}{/if}
        </div>
      </div>
      <div class="up-actions">
        <button class="btn btn-ghost" type="button" style="flex:1" onclick={closeWin}>
          {t('update.dlBg')}
        </button>
      </div>
    </div>
  {:else if status.phase === 'downloaded'}
    <!-- 状态三：下载完成，待重启 -->
    <div class="up-body">
      <div class="up-head">
        <div class="done-ring">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4.5 12.5l5 5 10-11"/></svg>
        </div>
        <div>
          <div class="up-h-title">{t('update.readyTitle')}</div>
          <div class="up-h-sub">{t('update.readySub', { version: status.version })}</div>
        </div>
      </div>
      <div class="done-note">
        <b>{t('update.readyWarnLabel')}</b>{t('update.readyWarn')}
      </div>
      <div class="up-actions">
        <button class="btn btn-ghost" type="button" onclick={closeWin}>
          {t('update.restartLater')}
        </button>
        <button class="btn btn-primary" type="button" onclick={startInstall}>
          {t('update.restartNow')}
        </button>
      </div>
    </div>
  {:else if status.phase === 'checking'}
    <!-- 兜底：窗口先于检查结果出现 -->
    <div class="up-body">
      <div class="up-head">
        <div class="up-logo">P</div>
        <div>
          <div class="up-h-title">{t('update.winTitle')}</div>
          <div class="up-h-sub">{t('update.checking')}</div>
        </div>
      </div>
    </div>
  {:else}
    <!-- idle / notAvailable / error：极简兜底（正常链路不会停在 These 态） -->
    <div class="up-body">
      <div class="up-head">
        <div class="up-logo">P</div>
        <div>
          <div class="up-h-title">{t('update.winTitle')}</div>
          <div class="up-h-sub">
            {status.phase === 'error' ? status.message : t('update.noUpdate')}
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  /* 稿图 17-update.html 类名全套；透明窗口实心底（章程铁律，禁玻璃拟态） */
  .upwin {
    width: 100vw;
    height: 100vh;
    background: var(--phx-win-bg);
    border-radius: var(--phx-r-win);
    box-shadow: var(--phx-shadow-win);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }
  .up-titlebar {
    height: 38px;
    display: flex;
    align-items: center;
    padding: 0 8px 0 16px;
    flex: none;
  }
  .up-tb-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--phx-text-3);
  }
  .up-body {
    padding: 6px 20px 20px;
    display: flex;
    flex-direction: column;
  }
  .up-head {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .up-logo {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: fantasy;
    font-style: italic;
    font-weight: 700;
    font-size: 23px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .up-h-title {
    font-size: 17px;
    font-weight: 700;
    letter-spacing: -0.2px;
  }
  .up-h-sub {
    font-size: 12px;
    color: var(--phx-text-2);
    margin-top: 3px;
    max-width: 300px;
  }
  .up-vers {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 16px;
  }
  .ver-chip {
    height: 26px;
    padding: 0 10px;
    border-radius: 7px;
    flex: none;
    display: inline-flex;
    align-items: center;
    font-family: var(--phx-font-mono);
    font-size: 12.5px;
    font-weight: 600;
  }
  .ver-old {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text-2);
  }
  .ver-new {
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .ver-arrow {
    width: 14px;
    height: 14px;
    color: var(--phx-text-3);
    flex: none;
  }
  .notes-card {
    margin-top: 14px;
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    max-height: 150px;
    overflow-y: auto;
    padding: 12px 14px;
  }
  .notes-card::-webkit-scrollbar {
    width: 4px;
  }
  .notes-card::-webkit-scrollbar-thumb {
    background: var(--phx-sep-strong);
    border-radius: 2px;
  }
  .nc-h {
    font-size: 11.5px;
    font-weight: 700;
    color: var(--phx-text-3);
    letter-spacing: 0.06em;
    margin-bottom: 8px;
  }
  .nc-li {
    display: flex;
    gap: 8px;
    font-size: 12.5px;
    color: var(--phx-text-2);
    line-height: 1.7;
  }
  .nc-li + .nc-li {
    margin-top: 5px;
  }
  .nc-dot {
    width: 4px;
    height: 4px;
    border-radius: 2px;
    background: var(--phx-accent);
    margin-top: 8px;
    flex: none;
  }
  .up-actions {
    display: flex;
    gap: 10px;
    margin-top: 18px;
  }
  .btn {
    height: 34px;
    padding: 0 16px;
    border: none;
    border-radius: var(--phx-r-btn);
    font-size: 13px;
    font-weight: 600;
    font-family: inherit;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    transition: filter 0.12s, background 0.12s;
  }
  .btn-primary {
    background: var(--phx-accent);
    color: #fff;
    flex: 1;
  }
  .btn-primary:hover {
    filter: brightness(1.12);
  }
  .btn-ghost {
    background: var(--phx-fill-quaternary);
    color: var(--phx-text-2);
  }
  .btn-ghost:hover {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
  }
  .prog {
    margin-top: 18px;
  }
  .prog-meta {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 8px;
  }
  .prog-pct {
    font-family: var(--phx-font-mono);
    font-size: 15px;
    font-weight: 700;
  }
  .prog-speed {
    font-size: 11.5px;
    color: var(--phx-text-3);
    font-family: var(--phx-font-mono);
  }
  .prog-track {
    height: 5px;
    border-radius: 3px;
    background: var(--phx-fill-tertiary);
    overflow: hidden;
  }
  .prog-fill {
    height: 100%;
    border-radius: 3px;
    background: var(--phx-accent);
    transition: width 0.2s linear;
  }
  .prog-sub {
    margin-top: 8px;
    font-size: 11.5px;
    color: var(--phx-text-3);
  }
  .done-ring {
    width: 46px;
    height: 46px;
    border-radius: 50%;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--phx-green-dim);
  }
  .done-ring svg {
    width: 22px;
    height: 22px;
    color: var(--phx-green);
  }
  .done-note {
    margin-top: 14px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--phx-warn-bg);
    box-shadow: inset 0 0 0 0.5px var(--phx-warn-bd);
    font-size: 12px;
    color: var(--phx-text-2);
    line-height: 1.6;
  }
  .done-note b {
    color: var(--phx-warn);
    font-weight: 600;
    margin-right: 4px;
  }
</style>
