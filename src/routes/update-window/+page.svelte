<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { LogicalSize } from '@tauri-apps/api/dpi'
  import { getVersion } from '@tauri-apps/api/app'
  import { phxUpdate, PHX_UPDATE_EVENT } from '$lib/ipc'
  import type { PhxUpdateStatus } from '$lib/ipc'
  import { t } from '$lib/i18n/i18n.svelte'
  import { playSound } from '$lib/sound'
  import { isReducedMotion } from '$lib/motion'

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

  /* ===== 引导轮播（仅下载中态展示；稿图 21-update-carousel.html） =====
   * 全部 slide 常驻堆叠（绝对定位），on/leaving 双类做淡入+轻位移过渡；
   * transition 由 app.css reduced-motion 双通道压平，isReducedMotion 只管自动轮播计时器。
   * 插图为内联 SVG path 集（token 色板），不引外部图片。 */
  type MsgKey = Parameters<typeof t>[0]
  interface CarSlide {
    t: MsgKey
    s: MsgKey
    cls: string
    icon: string[]
  }
  const CAR: CarSlide[] = [
    {
      t: 'update.car.t0',
      s: 'update.car.s0',
      cls: 'car-a0',
      icon: ['M21 8l-9-5-9 5v8l9 5 9-5V8z', 'M3.3 8.4L12 13l8.7-4.6', 'M12 13v9']
    },
    {
      t: 'update.car.t1',
      s: 'update.car.s1',
      cls: 'car-a1',
      icon: ['M12 4a4 4 0 1 0 0 8 4 4 0 1 0 0-8z', 'M4 21c0-4 3.6-6.5 8-6.5s8 2.5 8 6.5']
    },
    {
      t: 'update.car.t2',
      s: 'update.car.s2',
      cls: 'car-a2',
      icon: ['M9 11.5l2 2 4.5-5', 'M7 4h10a3 3 0 0 1 3 3v10a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3V7a3 3 0 0 1 3-3z']
    },
    {
      t: 'update.car.t3',
      s: 'update.car.s3',
      cls: 'car-a3',
      icon: ['M12 3l8 3v5c0 5-3.4 8.4-8 10-4.6-1.6-8-5-8-10V6l8-3z', 'M12 8v4l2.5 2.5']
    }
  ]
  const CAR_INTERVAL = 4000

  let carIdx = $state(0)
  let carPrev = $state(-1)
  let carTimer: ReturnType<typeof setInterval> | null = null
  let carLeaveTimer: ReturnType<typeof setTimeout> | null = null

  function carGo(i: number, byUser = false): void {
    if (i === carIdx) return
    carPrev = carIdx
    carIdx = i
    if (carLeaveTimer) clearTimeout(carLeaveTimer)
    carLeaveTimer = setTimeout(() => {
      carPrev = -1
    }, 420)
    if (byUser) {
      playSound('ui')
      carStartTimer() // 手动切换后重置计时
    }
  }
  function carStopTimer(): void {
    if (carTimer) {
      clearInterval(carTimer)
      carTimer = null
    }
  }
  function carStartTimer(): void {
    carStopTimer()
    if (!isReducedMotion()) {
      carTimer = setInterval(() => carGo((carIdx + 1) % CAR.length), CAR_INTERVAL)
    }
  }

  /* 布尔派生而非 dl 本体：下载进度高频更新不应反复重启计时器 */
  const isDl = $derived(dl !== null)
  $effect(() => {
    if (!isDl) return
    carStartTimer()
    return () => carStopTimer()
  })

  /* 窗高自适应：Rust 只给初值，状态变化后按内容实测重设（宽 420 不变）。
   * 否则固定 380 高会在内容短的状态下（如单行公告）窗底留一大块空白。 */
  const WIN_W = 420
  const TITLEBAR_H = 38
  let appliedH = 0

  $effect(() => {
    void status // 依赖：任一状态变化（含下载进度）都重测一次
    const h = document.querySelector<HTMLElement>('.up-body')?.offsetHeight ?? 0
    const total = TITLEBAR_H + h
    if (!h || Math.abs(total - appliedH) < 1) return
    appliedH = total
    void getCurrentWindow().setSize(new LogicalSize(WIN_W, total))
  })

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
      <!-- 引导轮播：下载等待期轮播功能亮点（稿图 21-update-carousel.html） -->
      <div class="carousel" aria-live="polite">
        <div class="car-viewport">
          {#each CAR as c, i (i)}
            <div class="car-slide" class:on={i === carIdx} class:leaving={i === carPrev}>
              <div class="car-art {c.cls}">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                  {#each c.icon as d (d)}<path d={d}/>{/each}
                </svg>
              </div>
              <div class="car-copy">
                <div class="car-t">{t(c.t)}</div>
                <div class="car-s">{t(c.s)}</div>
              </div>
            </div>
          {/each}
        </div>
        <div class="car-dots">
          {#each CAR as c, i (i)}
            <button class="car-dot" type="button" class:on={i === carIdx} aria-label={`${t('update.car.dot', { n: i + 1 })} · ${t(c.t)}`} onclick={() => carGo(i, true)}></button>
          {/each}
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
  /* ===== 引导轮播（稿图 21-update-carousel.html 定稿值） ===== */
  .carousel {
    margin-top: 16px;
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    padding: 16px 16px 12px;
    overflow: hidden;
    position: relative;
  }
  .car-viewport {
    position: relative;
    height: 118px;
  }
  .car-slide {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    gap: 16px;
    opacity: 0;
    transform: translateX(14px);
    pointer-events: none;
  }
  .car-slide.on {
    opacity: 1;
    transform: translateX(0);
    transition:
      opacity var(--phx-dur-slow) var(--phx-ease-out),
      transform var(--phx-dur-slow) var(--phx-ease-out);
    pointer-events: auto;
  }
  .car-slide.leaving {
    opacity: 0;
    transform: translateX(-14px);
    transition:
      opacity var(--phx-dur-slow) var(--phx-ease-out),
      transform var(--phx-dur-slow) var(--phx-ease-out);
  }
  .car-art {
    width: 92px;
    height: 92px;
    flex: none;
    border-radius: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
  }
  .car-art svg {
    width: 46px;
    height: 46px;
    position: relative;
    z-index: 1;
  }
  .car-art::after {
    content: '';
    position: absolute;
    inset: -9px;
    border-radius: 20px;
    border: 1px dashed var(--phx-sep-strong);
  }
  .car-art::before {
    content: '';
    position: absolute;
    right: -15px;
    top: -6px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--phx-accent);
    opacity: 0.55;
  }
  .car-a0 {
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .car-a1 {
    background: var(--phx-orange-dim);
    color: var(--phx-orange);
  }
  .car-a1::before {
    background: var(--phx-orange);
  }
  .car-a2 {
    background: var(--phx-green-dim);
    color: var(--phx-green);
  }
  .car-a2::before {
    background: var(--phx-green);
  }
  .car-a3 {
    background: var(--phx-warn-bg);
    color: var(--phx-warn);
  }
  .car-a3::before {
    background: var(--phx-warn);
  }
  .car-copy {
    min-width: 0;
  }
  .car-t {
    font-size: 15.5px;
    font-weight: 700;
    letter-spacing: -0.2px;
  }
  .car-s {
    font-size: 12px;
    color: var(--phx-text-2);
    line-height: 1.7;
    margin-top: 5px;
  }
  .car-dots {
    display: flex;
    justify-content: center;
    gap: 7px;
    margin-top: 12px;
  }
  .car-dot {
    width: 6px;
    height: 6px;
    border-radius: 3px;
    background: var(--phx-fill-quaternary);
    border: none;
    padding: 0;
    cursor: pointer;
    transition:
      background var(--phx-dur-fast) var(--phx-ease-out),
      width var(--phx-dur-norm) var(--phx-ease-emphasized);
  }
  .car-dot:hover {
    background: var(--phx-sep-strong);
  }
  .car-dot.on {
    width: 16px;
    background: var(--phx-accent);
  }
  .prog {
    margin-top: 16px; /* 21 稿定稿值：加轮播卡后进度区上间距 18→16 */
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
