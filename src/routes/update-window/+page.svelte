<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { getCurrentWindow } from '@tauri-apps/api/window'
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

  /** 速度（B/s，指数滑动平均）：不再直接显示，用于推算剩余时间 */
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
  /** 目标版本号（dl 分支里 status 不被 TS 收窄，单独取） */
  const dlVersion = $derived(status.phase === 'downloading' ? status.version : '')

  /** 字节 → MB 显示（1024 进位） */
  function mb(bytes: number): string {
    return (bytes / 1048576).toFixed(1)
  }

  /** 剩余时间（秒 → 约 N 秒；速度未知或 total 未知时不显示） */
  const etaText = $derived.by(() => {
    if (!dl || !dl.total || speed <= 0) return null
    const remain = Math.max(0, (dl.total - dl.received) / speed)
    return t('update.eta', { seconds: Math.ceil(remain) })
  })

  /** Release changelog 纯文本逐行 → 要点行（空行剔除，无富文本解析）；available 主卡与 downloaded 回顾卡共用 */
  const noteLines = $derived(
    (status.phase === 'available' || status.phase === 'downloaded') && status.notes
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
  let carTimer: ReturnType<typeof setInterval> | null = null

  function carGo(i: number, byUser = false): void {
    if (i === carIdx) return
    carIdx = i
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

  /* 窗口尺寸恒定（420×396，Rust inner_size 定死，稿图 27）：三态同骨架同尺寸，
   * 超长 notes 由 notes 卡内部滚动兜底——动态 setSize 通路整个移除，
   * 曾经的 capability 漏配静默拒调问题（§二.45）随之失去存在场景。 */

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
    <!-- 状态一：发现新版本（稿图 27：箭头徽章 + 版本对进副题 + notes 占满内容槽） -->
    <div class="up-body">
      <div class="up-head">
        <div class="badge badge-available">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v13"/><path d="M6 12l6 6 6-6"/></svg>
        </div>
        <div>
          <div class="up-h-title">{t('update.foundTitle')}</div>
          <div class="up-h-sub">
            <span class="mono">{currentVer}</span>
            <svg class="ver-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M13 6l6 6-6 6"/></svg>
            <span class="mono new">{status.version}</span>
            <span class="sub-tail">，{t('update.subFoundTail')}</span>
          </div>
        </div>
      </div>
      <div class="up-slot">
        {#if noteLines.length > 0}
          <div class="notes-card">
            <div class="nc-h">{t('update.notesTitle')}</div>
            {#each noteLines as line (line)}
              <div class="nc-li"><span class="nc-dot"></span><span>{line}</span></div>
            {/each}
          </div>
        {/if}
      </div>
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
    <!-- 状态二：下载中（后台下载 = 关窗，Rust 下载任务独立于窗口继续；稿图 27：进度环徽章 + 进度主卡，轮播降条幅） -->
    <div class="up-body">
      <div class="up-head">
        <div class="badge badge-ring">
          <svg viewBox="0 0 44 44">
            <circle cx="22" cy="22" r="17" fill="none" stroke="var(--phx-sep-strong)" stroke-width="3.5"/>
            <!-- 弧长 = 周长 2π×17 ≈ 106.8 × 进度 -->
            <circle class="ring-fill" cx="22" cy="22" r="17" fill="none" stroke="var(--phx-accent)" stroke-width="3.5" stroke-linecap="round" transform="rotate(-90 22 22)" style="stroke-dasharray: {(dl.pct / 100) * 106.8} 106.8"/>
          </svg>
          <span class="ring-pct">{dl.pct > 0 ? Math.floor(dl.pct) : '—'}</span>
        </div>
        <div>
          <div class="up-h-title">{t('update.dlTitle')}</div>
          <div class="up-h-sub">
            <span class="mono">{currentVer}</span>
            <svg class="ver-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14M13 6l6 6-6 6"/></svg>
            <span class="mono new">{dlVersion}</span>
            <span class="sub-tail"> · {t('update.subDlTail')}</span>
          </div>
        </div>
      </div>
      <div class="up-slot">
        <!-- 引导条幅：下载等待期轮播功能亮点（稿图 27 由插画卡降为条幅，稿图 21 文案沿用） -->
        <div class="car-strip" aria-live="polite">
          <div class="car-ico {CAR[carIdx].cls}">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
              {#each CAR[carIdx].icon as d (d)}<path d={d}/>{/each}
            </svg>
          </div>
          <div class="car-copy">
            <div class="car-t">{t(CAR[carIdx].t)}</div>
            <div class="car-s">{t(CAR[carIdx].s)}</div>
          </div>
          <div class="car-dots">
            {#each CAR as c, i (i)}
              <button class="car-dot" type="button" class:on={i === carIdx} aria-label={`${t('update.car.dot', { n: i + 1 })} · ${t(c.t)}`} onclick={() => carGo(i, true)}></button>
            {/each}
          </div>
        </div>
        <!-- 进度主卡：大百分比 + 进度条 + MB 量词；速度不显示，换用户真正关心的剩余时间 -->
        <div class="prog-card">
          <div class="prog-meta-top">
            <span class="prog-pct">{dl.pct > 0 ? `${Math.floor(dl.pct)}%` : '—'}</span>
            {#if etaText}<span class="prog-eta">{etaText}</span>{/if}
          </div>
          <div class="prog-track"><div class="prog-fill" style="width: {dl.pct}%"></div></div>
          <div class="prog-sub">
            {dl.total ? `${mb(dl.received)} MB / ${mb(dl.total)} MB` : mb(dl.received) + ' MB'}
          </div>
        </div>
      </div>
      <div class="up-actions">
        <button class="btn btn-ghost" type="button" style="flex:1" onclick={closeWin}>
          {t('update.dlBg')}
        </button>
      </div>
    </div>
  {:else if status.phase === 'downloaded'}
    <!-- 状态三：下载完成，待重启（稿图 27：提示卡 + 「本次更新内容」回顾卡填满内容槽，三态等高） -->
    <div class="up-body">
      <div class="up-head">
        <div class="badge badge-ready">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M4.5 12.5l5 5 10-11"/></svg>
        </div>
        <div>
          <div class="up-h-title">{t('update.readyTitle')}</div>
          <div class="up-h-sub">{t('update.readySub', { version: status.version })}</div>
        </div>
      </div>
      <div class="up-slot">
        <div class="done-note">
          <b>{t('update.readyWarnLabel')}</b>{t('update.readyWarn')}
        </div>
        {#if noteLines.length > 0}
          <div class="notes-card review">
            <div class="nc-h">{t('update.readyNotesTitle')}</div>
            {#each noteLines as line (line)}
              <div class="nc-li"><span class="nc-dot"></span><span>{line}</span></div>
            {/each}
          </div>
        {/if}
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
    padding: 4px 20px 20px;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
  /* 内容槽（稿图 27）：弹性主区，notes/进度卡占满，动作区贴底 */
  .up-slot {
    margin-top: 14px;
    flex: 1;
    min-height: 0;
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
  /* 状态徽章（稿图 27）：available 箭头 / downloading 进度环 / downloaded 绿勾，同形同位 */
  .badge {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
  }
  .badge svg {
    width: 22px;
    height: 22px;
  }
  .badge-available {
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .badge-ready {
    background: var(--phx-green-dim);
    color: var(--phx-green);
  }
  .badge-ring svg {
    position: absolute;
    inset: 0;
    width: 44px;
    height: 44px;
  }
  .ring-fill {
    transition: stroke-dasharray 0.2s linear;
  }
  .ring-pct {
    font-family: var(--phx-font-mono);
    font-size: 9.5px;
    font-weight: 700;
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
    max-width: 340px;
  }
  .up-h-sub .mono {
    font-family: var(--phx-font-mono);
    font-size: 11.5px;
    font-weight: 600;
    color: var(--phx-text-2);
  }
  .up-h-sub .mono.new {
    color: var(--phx-accent);
  }
  .ver-arrow {
    width: 12px;
    height: 12px;
    color: var(--phx-text-3);
    vertical-align: -2px;
  }
  .notes-card {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow-y: auto;
    padding: 12px 14px;
    flex: 1;
    min-height: 0;
  }
  .notes-card.review {
    margin-top: 10px;
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
  /* ===== 引导条幅（稿图 27：由 21 稿插画卡降为条幅配角，文案沿用） ===== */
  .car-strip {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    padding: 10px 14px;
    display: flex;
    align-items: center;
    gap: 12px;
    flex: none;
  }
  .car-ico {
    width: 36px;
    height: 36px;
    border-radius: 10px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .car-ico svg {
    width: 20px;
    height: 20px;
  }
  .car-a0 {
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .car-a1 {
    background: var(--phx-orange-dim);
    color: var(--phx-orange);
  }
  .car-a2 {
    background: var(--phx-green-dim);
    color: var(--phx-green);
  }
  .car-a3 {
    background: var(--phx-warn-bg);
    color: var(--phx-warn);
  }
  .car-copy {
    min-width: 0;
    flex: 1;
  }
  .car-t {
    font-size: 13px;
    font-weight: 700;
    letter-spacing: -0.1px;
  }
  .car-s {
    font-size: 11.5px;
    color: var(--phx-text-2);
    margin-top: 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .car-dots {
    display: flex;
    gap: 5px;
    flex: none;
  }
  .car-dot {
    width: 5px;
    height: 5px;
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
    width: 14px;
    background: var(--phx-accent);
  }
  /* 进度主卡（稿图 27）：大百分比 + 进度条 + MB 量词；速度不显示，换剩余时间 */
  .prog-card {
    margin-top: 10px;
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    padding: 16px 16px 14px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    flex: 1;
    min-height: 0;
  }
  .prog-meta-top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .prog-pct {
    font-family: var(--phx-font-mono);
    font-size: 28px;
    font-weight: 700;
    letter-spacing: -0.5px;
  }
  .prog-eta {
    font-size: 11.5px;
    color: var(--phx-text-3);
  }
  .prog-track {
    height: 5px;
    border-radius: 3px;
    background: var(--phx-fill-tertiary);
    overflow: hidden;
    margin-top: 10px;
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
  .done-note {
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
