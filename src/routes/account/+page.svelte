<script lang="ts">
  // 24-account 账号页（稿图 docs/prototypes/24-account.html）
  // 2026-10-22 从设置页第 5 组「GitHub 账号与云同步」整组迁出，四态不变：
  // A 未登录 / C 授权中 / B 已登录 / D 同步失败，同一时刻只显示一种。
  import { onMount } from 'svelte'
  import { SvelteDate } from 'svelte/reactivity'
  import { getVersion } from '@tauri-apps/api/app'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import { t } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { sync, confirmSync, startLogin, cancelLogin, logout } from '$lib/sync.svelte'
  import PhxDialog from '$lib/components/PhxDialog.svelte'

  // 真实外链：开发者主页 / 源码仓库 / 发布页 / 问题反馈
  const HOME_URL = 'https://github.com/65tiankehan'
  const REPO_URL = 'https://github.com/65tiankehan/Phoenix-DRX'
  const RELEASES_URL = 'https://github.com/65tiankehan/Phoenix-DRX-releases/releases'
  const ISSUES_URL = 'https://github.com/65tiankehan/Phoenix-DRX-releases/issues'

  // 技术底座：真实栈，专有名词不翻译（同 about 页 MIT / GitHub Issues 先例）
  const STACK = ['Tauri 2', 'Svelte 5', 'Rust', 'TypeScript', 'Vite 8', 'SPT 4.1.x']

  let version = $state('')
  let avatarFailed = $state(false)
  let logoutOpen = $state(false)

  onMount(() => {
    void getVersion().then((v) => (version = v))
  })

  /** 配对码倒计时 mm:ss */
  const countdownText = $derived.by(() => {
    if (!sync.flow) return ''
    const m = Math.floor(sync.flow.remainingSec / 60)
    const s = sync.flow.remainingSec % 60
    return `${m}:${String(s).padStart(2, '0')}`
  })

  /** 最近一次成功同步的本地化时间（头卡统计与 D 态身份行共用） */
  const lastSyncText = $derived.by(() => {
    if (sync.lastSyncMs === null) return t('sync.neverSynced')
    const d = new Date(sync.lastSyncMs)
    const hm = `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
    const startToday = new SvelteDate().setHours(0, 0, 0, 0)
    if (sync.lastSyncMs >= startToday) return t('sync.syncedToday', { time: hm })
    if (sync.lastSyncMs >= startToday - 86400000) return t('sync.syncedYesterday', { time: hm })
    return t('sync.syncedDate', { month: d.getMonth() + 1, day: d.getDate(), time: hm })
  })

  function copyUserCode(): void {
    if (!sync.flow) return
    void navigator.clipboard
      .writeText(sync.flow.userCode)
      .then(() => toast.success(t('sync.codeCopied')))
      .catch(() => {})
  }

  function openVerifyPage(): void {
    if (sync.flow) void openUrl(sync.flow.verificationUri).catch(() => {})
  }

  /** 外链统一走系统浏览器：WebView2 会拦新窗口，href 只保留右键复制语义 */
  function openExternal(e: MouseEvent, url: string): void {
    e.preventDefault()
    void openUrl(url).catch(() => {})
  }
</script>

<main class="page">
  <header class="main-head">
    <div>
      <h1 class="large-title">{t('nav.account')}</h1>
      <p class="head-meta">{t('account.headMeta')}</p>
    </div>
  </header>

  <div class="scroll">
    {#if sync.sessionLoaded}
      <div class="pane">
        {#if sync.configured}
          <!-- 头卡 · 已登录：真头像 / 昵称 / 状态徽章 / 三项真实统计 / 主操作 -->
          <div class="hero">
            <div class="pfp" class:err={sync.lastError} aria-hidden="true">
              {#if sync.avatarUrl && !avatarFailed}
                <img
                  class="pfp-img"
                  src={sync.avatarUrl}
                  alt=""
                  onerror={() => (avatarFailed = true)}
                />
              {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><circle cx="12" cy="8" r="3.6"/><path d="M5 20c1.4-3.6 4-5 7-5s5.6 1.4 7 5"/></svg>
              {/if}
              <span class="dot"></span>
            </div>
            <div class="hero-id">
              <div class="hero-name">
                {sync.login}
                <span class="badge" class:ok={!sync.lastError} class:err={sync.lastError}>
                  {sync.lastError ? t('sync.badgeErr') : t('sync.badgeOk')}
                </span>
              </div>
              <div class="hero-sub">{t('account.heroSubConnected')}</div>
            </div>
            <div class="stats">
              <div class="stat">
                <div class="k">{t('account.stat.lastSync')}</div>
                <div class="v">{lastSyncText}</div>
              </div>
              <div class="stat">
                <div class="k">{t('account.stat.scope')}</div>
                <div class="v">{t('account.stat.scopeValue')}</div>
              </div>
              <div class="stat">
                <div class="k">{t('account.stat.version')}</div>
                <div class="v mono">{version || '—'}</div>
              </div>
            </div>
            <div class="hact">
              <button class="btn" type="button" disabled={sync.syncing} onclick={confirmSync}>
                {t('sync.btnSyncNow')}
              </button>
              <button class="btn danger" type="button" onclick={() => (logoutOpen = true)}>
                {t('sync.btnLogout')}
              </button>
            </div>
          </div>
        {:else}
          <!-- 头卡 · 未登录：无人形绿点、无统计，主操作换成登录 -->
          <div class="hero">
            <div class="pfp" aria-hidden="true">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><circle cx="12" cy="8" r="3.6"/><path d="M5 20c1.4-3.6 4-5 7-5s5.6 1.4 7 5"/></svg>
            </div>
            <div class="hero-id">
              <div class="hero-name">{t('account.notLoggedIn')}</div>
              <div class="hero-sub">{t('account.heroSubLoggedOut')}</div>
            </div>
            {#if !sync.flow}
              <div class="hact">
                <button class="btn" type="button" onclick={startLogin}>{t('sync.btnLogin')}</button>
              </div>
            {/if}
          </div>
        {/if}

        <div class="g2">
          <!-- 左列：同步设置（原设置页第 5 组） -->
          <div class="scol">
            {#if !sync.configured && sync.flow}
              <!-- 状态 C · 授权中 -->
              <section class="sgroup">
                <div class="sgroup-h">{t('sync.title')}</div>
                <div class="sgroup-sub">{t('sync.subC')}</div>
                <div class="srow">
                  <div class="stext">
                    <div class="slabel">{t('sync.authLabel')}</div>
                    <div class="sdesc">
                      <span class="stepn">1.</span> {t('sync.authStep1')}&ensp;<span class="stepn">2.</span> {t('sync.authStep2')}&ensp;<span class="stepn">3.</span> {t('sync.authStep3')}
                    </div>
                  </div>
                </div>
                <div class="code-wrap">
                  <div class="code-box">{sync.flow.userCode}</div>
                  <button class="code-url" type="button" onclick={openVerifyPage}>
                    github.com/login/device
                  </button>
                </div>
                <div class="srow">
                  <div class="stext">
                    <div class="slabel">{t('sync.codeValidLabel')}</div>
                    <div class="sdesc">
                      {t('sync.codeValidPrefix')}<span class="cd">{countdownText}</span>{t('sync.codeValidSuffix')}
                    </div>
                  </div>
                  <div class="act">
                    <button class="mbtn" type="button" onclick={copyUserCode}>
                      {t('sync.btnCopyCode')}
                    </button>
                    <button class="mbtn ghost" type="button" onclick={openVerifyPage}>
                      {t('sync.btnOpenPage')}
                    </button>
                  </div>
                </div>
                <div class="srow">
                  <div class="stext">
                    <div class="slabel">{t('sync.cancelLabel')}</div>
                    <div class="sdesc">{t('sync.cancelDesc')}</div>
                  </div>
                  <button class="mbtn danger" type="button" onclick={cancelLogin}>
                    {t('sync.btnCancel')}
                  </button>
                </div>
              </section>
            {:else}
              {#if sync.configured && sync.lastError}
                <!-- 状态 D · 同步失败（登录本身正常，只报这一次没传上去） -->
                <section class="sgroup">
                  <div class="sgroup-h">{t('sync.title')}</div>
                  <div class="sgroup-sub">{t('sync.subD')}</div>
                  <div class="srow">
                    <div class="id">
                      <div class="avatar err" aria-hidden="true">
                        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="8" r="3.6"/><path d="M5 20c1.4-3.6 4-5 7-5s5.6 1.4 7 5"/></svg>
                        <span class="dot"></span>
                      </div>
                      <div>
                        <div class="slabel">{sync.login}</div>
                        <div class="sdesc">{t('sync.lastSuccessSync', { text: lastSyncText })}</div>
                      </div>
                    </div>
                    <span class="badge err">{t('sync.badgeErr')}</span>
                  </div>
                  <div class="srow">
                    <div class="stext">
                      <div class="slabel">{t('sync.explainLabel')}</div>
                      <div class="sdesc">{t('sync.explainDesc')}</div>
                    </div>
                  </div>
                  <div class="srow">
                    <div class="stext">
                      <div class="slabel">{t('sync.retryLabel')}</div>
                      <div class="sdesc">{t('sync.retryDesc')}</div>
                    </div>
                    <button class="mbtn" type="button" disabled={sync.syncing} onclick={confirmSync}>
                      {t('sync.btnRetry')}
                    </button>
                  </div>
                  <div class="srow">
                    <div class="stext">
                      <div class="slabel">{t('sync.logoutLabel')}</div>
                      <div class="sdesc">{t('sync.logoutDesc')}</div>
                    </div>
                    <button class="mbtn danger" type="button" onclick={() => (logoutOpen = true)}>
                      {t('sync.btnLogout')}
                    </button>
                  </div>
                </section>
              {/if}

              <!-- 云同步范围：登录后是「自动同步」，未登录先讲清楚能同步什么 -->
              <section class="sgroup">
                <div class="sgroup-h">{t('account.sync.title')}</div>
                <div class="sgroup-sub">
                  {sync.configured ? t('account.sync.sub') : t('sync.subA')}
                </div>
                {#if sync.configured}
                  <div class="srow">
                    <div class="stext">
                      <div class="slabel">{t('sync.scopeLabel')}</div>
                      <div class="sdesc">{t('sync.scopeDesc')}</div>
                    </div>
                  </div>
                  <div class="srow">
                    <div class="stext">
                      <div class="slabel">{t('account.sync.autoLabel')}</div>
                      <div class="sdesc">{t('account.sync.autoDesc')}</div>
                    </div>
                  </div>
                {:else}
                  <div class="srow">
                    <div class="stext">
                      <div class="slabel">{t('sync.cloudLabel')}</div>
                      <div class="sdesc">{t('sync.cloudDesc')}</div>
                    </div>
                  </div>
                  <div class="srow">
                    <div class="stext">
                      <div class="slabel">{t('sync.scopeLabel')}</div>
                      <div class="sdesc">{t('sync.scopeDesc')}</div>
                    </div>
                  </div>
                {/if}
              </section>
            {/if}

            <!-- 实话卡：同步范围之外，一条都不传 -->
            <section class="sgroup">
              <div class="sgroup-h">{t('account.privacy.title')}</div>
              <div class="sgroup-sub">{t('account.privacy.sub')}</div>
              <div class="srow">
                <div class="stext">
                  <div class="slabel">{t('account.privacy.saveLabel')}</div>
                  <div class="sdesc">{t('account.privacy.saveDesc')}</div>
                </div>
              </div>
              <div class="srow">
                <div class="stext">
                  <div class="slabel">{t('account.privacy.tokenLabel')}</div>
                  <div class="sdesc">{t('account.privacy.tokenDesc')}</div>
                </div>
              </div>
            </section>
          </div>

          <!-- 右列：项目入口 / 技术底座 -->
          <div class="scol">
            <section class="sgroup">
              <div class="sgroup-h">{t('account.links.title')}</div>
              <div class="sgroup-sub">{t('account.links.sub')}</div>
              <a
                class="lnkrow"
                href={HOME_URL}
                target="_blank"
                rel="noopener external"
                onclick={(e) => openExternal(e, HOME_URL)}
              >
                <span class="ico">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M9 19c-4.5 1.5-4.5-2.5-6-3m12 6v-3.9c0-1.1.1-1.5-.5-2.1 2.8-.3 5.5-1.4 5.5-6a4.7 4.7 0 0 0-1.3-3.2 4.4 4.4 0 0 0-.1-3.3s-1.1-.3-3.5 1.3a12 12 0 0 0-6.2 0C6.5 1.2 5.4 1.5 5.4 1.5a4.4 4.4 0 0 0-.1 3.3 4.7 4.7 0 0 0-1.3 3.2c0 4.6 2.7 5.7 5.5 6-.6.6-.6 1.2-.5 2.1V22"/></svg>
                </span>
                <span class="ltext">
                  <span class="t">{t('account.links.home')}</span>
                  <span class="d">{t('account.links.homeDesc')}</span>
                </span>
                <span class="ar" aria-hidden="true">↗</span>
              </a>
              <a
                class="lnkrow"
                href={REPO_URL}
                target="_blank"
                rel="noopener external"
                onclick={(e) => openExternal(e, REPO_URL)}
              >
                <span class="ico">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4h6v6H4z"/><path d="M14 14h6v6h-6z"/><path d="M10 7h4v10"/></svg>
                </span>
                <span class="ltext">
                  <span class="t">{t('account.links.repo')}</span>
                  <span class="d">{t('account.links.repoDesc')}</span>
                </span>
                <span class="ar" aria-hidden="true">↗</span>
              </a>
              <a
                class="lnkrow"
                href={RELEASES_URL}
                target="_blank"
                rel="noopener external"
                onclick={(e) => openExternal(e, RELEASES_URL)}
              >
                <span class="ico">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v12"/><path d="m7 10 5 5 5-5"/><path d="M4 20h16"/></svg>
                </span>
                <span class="ltext">
                  <span class="t">{t('account.links.releases')}</span>
                  <span class="d">{t('account.links.releasesDesc')}</span>
                </span>
                <span class="ar" aria-hidden="true">↗</span>
              </a>
              <a
                class="lnkrow"
                href={ISSUES_URL}
                target="_blank"
                rel="noopener external"
                onclick={(e) => openExternal(e, ISSUES_URL)}
              >
                <span class="ico">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a8 8 0 0 1-8 8H7l-4 3V12a8 8 0 0 1 8-8h2a8 8 0 0 1 8 8z"/><path d="M12 8v3"/><path d="M12 14h.01"/></svg>
                </span>
                <span class="ltext">
                  <span class="t">{t('account.links.issues')}</span>
                  <span class="d">{t('account.links.issuesDesc')}</span>
                </span>
                <span class="ar" aria-hidden="true">↗</span>
              </a>
            </section>

            <!-- 技术底座：真实栈，单行收口 -->
            <section class="sgroup">
              <div class="srow">
                <div class="stext">
                  <div class="slabel">{t('account.stack.label')}</div>
                  <div class="sdesc">{t('account.stack.desc')}</div>
                </div>
                <div class="chips inline">
                  {#each STACK as item, i (item)}
                    <span class="chip" class:acc={i === 0}>{item}</span>
                  {/each}
                </div>
              </div>
            </section>
          </div>
        </div>
      </div>
    {/if}
  </div>
</main>

<PhxDialog
  bind:open={logoutOpen}
  heading={t('sync.logoutConfirmTitle')}
  text={t('sync.logoutConfirmBody')}
  confirmText={t('sync.logoutConfirmBtn')}
  danger
  onconfirm={logout}
/>

<style>
  /* 容器三段式（对齐 health / about 等生产页惯例） */
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .main-head {
    padding: 24px 24px 0;
    flex: none;
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
  }
  .large-title {
    font-size: 34px;
    font-weight: 700;
    letter-spacing: -0.4px;
    line-height: 1.15;
  }
  .head-meta {
    font-size: 13px;
    color: var(--phx-text-2);
    margin-top: 6px;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    margin-top: 12px;
    padding: 0 24px;
    scrollbar-width: none;
  }
  .scroll::-webkit-scrollbar {
    display: none;
  }
  .pane {
    min-height: 100%;
  }
  /* ===== 身份头卡（横幅底 + 大头像 + 统计 + 主操作） ===== */
  .hero {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 20px;
    border-radius: var(--phx-r-card);
    background:
      radial-gradient(120% 160% at 0% 0%, var(--phx-accent-dim), transparent 58%),
      var(--phx-surface);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep);
  }
  .pfp {
    width: 64px;
    height: 64px;
    border-radius: 50%;
    flex: none;
    position: relative;
    background: var(--phx-surface-2);
    box-shadow: inset 0 0 0 1px var(--phx-sep-strong);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--phx-accent);
    overflow: hidden;
  }
  .pfp svg {
    width: 30px;
    height: 30px;
  }
  .pfp-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .pfp .dot {
    position: absolute;
    right: 1px;
    bottom: 1px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--phx-green);
    box-shadow: 0 0 0 2.5px var(--phx-surface);
  }
  .pfp.err .dot {
    background: var(--phx-danger);
  }
  .hero-id {
    min-width: 0;
  }
  .hero-name {
    font-size: 21px;
    font-weight: 700;
    letter-spacing: -0.2px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .hero-sub {
    font-size: 12px;
    color: var(--phx-text-2);
    margin-top: 4px;
  }
  .stats {
    display: flex;
    gap: 26px;
    margin-left: 10px;
    padding-left: 26px;
    border-left: 0.5px solid var(--phx-sep);
  }
  .stat .k {
    font-size: 10.5px;
    color: var(--phx-text-3);
    letter-spacing: 0.04em;
  }
  .stat .v {
    font-size: 13px;
    font-weight: 600;
    margin-top: 4px;
  }
  .stat .v.mono {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12.5px;
  }
  .hact {
    display: flex;
    gap: 8px;
    margin-left: auto;
    flex: none;
  }
  /* ===== 按钮 ===== */
  .btn {
    height: 30px;
    padding: 0 14px;
    border: none;
    border-radius: var(--phx-r-btn);
    cursor: pointer;
    font-size: 12.5px;
    font-weight: 600;
    font-family: inherit;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    text-decoration: none;
    background: var(--phx-accent);
    color: #fff;
  }
  .btn:hover {
    filter: brightness(1.1);
  }
  .btn.danger {
    background: transparent;
    color: var(--phx-text-3);
  }
  .btn.danger:hover {
    background: var(--phx-danger-bg);
    color: var(--phx-danger);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
    pointer-events: none;
  }
  .mbtn {
    height: 26px;
    padding: 0 12px;
    border: none;
    border-radius: var(--phx-r-btn);
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    display: inline-flex;
    align-items: center;
    text-decoration: none;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .mbtn:hover {
    filter: brightness(1.15);
  }
  .mbtn.ghost {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
  }
  .mbtn.danger {
    background: transparent;
    color: var(--phx-text-3);
  }
  .mbtn.danger:hover {
    background: var(--phx-danger-bg);
    color: var(--phx-danger);
  }
  .mbtn:disabled {
    opacity: 0.5;
    cursor: default;
    pointer-events: none;
  }
  .act {
    display: flex;
    gap: 8px;
    flex: none;
  }
  /* ===== 双列卡片 ===== */
  .g2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0 16px;
    align-items: start;
    margin-top: 16px;
    padding-bottom: 24px;
  }
  .scol {
    min-width: 0;
  }
  .sgroup {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow: hidden;
  }
  .sgroup + .sgroup {
    margin-top: 16px;
  }
  .sgroup-h {
    padding: 14px 18px 2px;
    font-size: 14.5px;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .sgroup-sub {
    padding: 0 18px 12px;
    font-size: 11.5px;
    color: var(--phx-text-3);
  }
  .srow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10.5px 18px;
  }
  .srow + .srow,
  .sgroup-sub + .srow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .stext {
    min-width: 0;
  }
  .slabel {
    font-size: 13px;
    font-weight: 600;
  }
  .sdesc {
    font-size: 12px;
    color: var(--phx-text-2);
    margin-top: 3px;
    line-height: 1.5;
  }
  /* ===== 徽章 ===== */
  .badge {
    height: 22px;
    padding: 0 10px;
    border-radius: 999px;
    flex: none;
    display: inline-flex;
    align-items: center;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .badge.ok {
    background: var(--phx-green-dim);
    color: var(--phx-green);
  }
  .badge.err {
    background: var(--phx-danger-bg);
    color: var(--phx-danger);
  }
  /* ===== 项目入口行（真外链，右箭头） ===== */
  .lnkrow {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 11px 18px;
    text-decoration: none;
    transition: background 0.12s;
  }
  .lnkrow + .lnkrow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .lnkrow:hover {
    background: var(--phx-fill-tertiary);
  }
  .lnkrow .ico {
    width: 26px;
    height: 26px;
    border-radius: 7px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--phx-fill-tertiary);
    color: var(--phx-text-2);
  }
  .lnkrow .ico svg {
    width: 14px;
    height: 14px;
  }
  .lnkrow .ltext {
    min-width: 0;
  }
  /* 稿图里两层是裸 span，生产须显式换行才能形成标题/说明两行 */
  .lnkrow .t,
  .lnkrow .d {
    display: block;
  }
  .lnkrow .t {
    font-size: 13px;
    font-weight: 600;
    color: var(--phx-text);
  }
  .lnkrow .d {
    font-size: 11.5px;
    color: var(--phx-text-3);
    margin-top: 2px;
  }
  .lnkrow .ar {
    margin-left: auto;
    color: var(--phx-text-3);
    font-size: 12px;
    flex: none;
  }
  /* ===== 技术底座 chips ===== */
  .chips.inline {
    padding: 0;
    display: grid;
    grid-template-columns: repeat(3, auto);
    gap: 6px;
    justify-content: end;
  }
  .chip {
    height: 22px;
    padding: 0 9px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--phx-text-2);
    background: var(--phx-fill-tertiary);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
  }
  .chip.acc {
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    box-shadow: none;
  }
  /* ===== 授权中：配对码 ===== */
  .code-wrap {
    padding: 13px 18px 14px;
    border-top: 0.5px solid var(--phx-sep);
  }
  .code-wrap + .srow {
    border-top: none;
  }
  .code-box {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 24px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-align: center;
    padding: 12px 10px;
    background: var(--phx-surface-2);
    border-radius: var(--phx-r-btn);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
    user-select: all;
  }
  .code-url {
    display: block;
    margin-top: 9px;
    width: 100%;
    border: none;
    cursor: pointer;
    text-align: center;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
    color: var(--phx-accent);
    background: none;
    text-decoration: none;
  }
  .code-url:hover {
    text-decoration: underline;
  }
  .stepn {
    color: var(--phx-accent);
    font-weight: 700;
  }
  .cd {
    font-family: ui-monospace, Consolas, monospace;
    color: var(--phx-text);
    font-weight: 600;
  }
</style>

