<script lang="ts">
  // 15-settings 设置页（稿图 docs/prototypes/15-settings.html）
  // 组1 含「检查更新」开关 + 手动检查入口（2026-10-19 更新链落地，推翻 10-18 的「砍开关」裁决）。
  // 保存语义（稿图 note7，对齐参考 NotifyPropertyChangedAndSave）：改动即时落盘 + 轻提示，页头无「保存」按钮。
  import { onMount, tick } from 'svelte'
  import { SvelteDate } from 'svelte/reactivity'
  import { page } from '$app/state'
  import { resolve } from '$app/paths'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import { t, i18n, setLocale, type Locale } from '$lib/i18n/i18n.svelte'
  import { isSoundEnabled, setSoundEnabled } from '$lib/sound'
  import { isReducedMotion, setReducedMotion } from '$lib/motion'
  import {
    phxSettings,
    phxServer,
    phxProfile,
    phxUpdate,
    type PhxIssueHandling,
    type PhxProfileMeta,
    type PhxServerRow
  } from '$lib/ipc'
  import { toast } from '$lib/toast.svelte'
  import { sync, confirmSync, startLogin, cancelLogin, logout } from '$lib/sync.svelte'
  import PhxSwitch from '$lib/components/PhxSwitch.svelte'
  import PhxDropdown from '$lib/components/PhxDropdown.svelte'
  import PhxDialog from '$lib/components/PhxDialog.svelte'

  let booting = $state(true)
  let loaded = $state(false)
  let resetOpen = $state(false)

  // —— 感官层偏好（localStorage 通道：phx.sound / phx.motion）——
  let soundOn = $state(isSoundEnabled())
  let motionOn = $state(isReducedMotion())
  $effect(() => setSoundEnabled(soundOn))
  $effect(() => setReducedMotion(motionOn))

  // —— 行为偏好（settings.json 通道，Rust 侧整对象写入；自动补全消费端已接入
  // 技能页读写链：load_skills 内存补视图，save_skills 写盘前补全落盘）——
  // 字段名对齐 Rust Settings 序列化（checkUpdates / autoAddMasterings / autoAddScavSkills），
  // 名字对不上会读到 undefined，Svelte 5 会因 bind 到 undefined 直接炸渲染。
  let checkUpdates = $state(true)
  let autoAddMasterings = $state(false)
  let autoAddScavSkills = $state(false)
  let issueHandling = $state<PhxIssueHandling>('ask')

  // armed 门：onMount 回填触发的 $effect 静默跑过，用户交互后才真正保存+提示。
  // armed 刻意不用 $state —— 回填后置 true 不应反过来再触发本 effect。
  let armed = false
  $effect(() => {
    const c = checkUpdates
    const a = autoAddMasterings
    const s = autoAddScavSkills
    const h = issueHandling
    if (!armed) return
    void phxSettings
      .set({ checkUpdates: c, autoAddMasterings: a, autoAddScavSkills: s, issueHandling: h })
      .then(() => toast.success(t('settings.toast.saved')))
  })

  // —— 服务器与账号（01 屏职责去重：此处只读展示 + 跳转）——
  let serverRow = $state<PhxServerRow | null>(null)
  let profileId = $state<string | null>(null)
  let profileMetas = $state<PhxProfileMeta[]>([])
  const serverText = $derived(serverRow ? serverRow.path : t('settings.noServer'))
  const accountText = $derived.by(() => {
    if (!profileId) return t('settings.noAccount')
    // 稿图形态：昵称 [存档文件名]；查不到昵称时回退存档 id
    const meta = profileMetas.find((p) => p.id === profileId)
    return `${meta ? meta.nickname : profileId} [${profileId}.json]`
  })

  onMount(async () => {
    try {
      const [settings, server, pid, metas] = await Promise.all([
        phxSettings.get(),
        phxServer.current(),
        phxProfile.current(),
        phxProfile.list()
      ])
      checkUpdates = settings.checkUpdates
      autoAddMasterings = settings.autoAddMasterings
      autoAddScavSkills = settings.autoAddScavSkills
      issueHandling = settings.issueHandling
      serverRow = server
      profileId = pid
      profileMetas = metas
    } finally {
      await tick() // 先让回填触发的保存 effect 以未武装状态跑完
      armed = true
      loaded = true
      booting = false
    }
  })

  // 语言下拉：稿图原文固定两态，不随语言翻译
  const langOptions = [
    { value: 'zh-CN', label: '简体中文' },
    { value: 'en-US', label: 'English' }
  ]
  const issueOptions = $derived([
    { value: 'ask', label: t('settings.issue.ask') },
    { value: 'fix', label: t('settings.issue.fix') },
    { value: 'ignore', label: t('settings.issue.ignore') }
  ])

  function onLangChange(v: string): void {
    setLocale(v as Locale)
    toast.success(t('settings.toast.saved'))
  }

  function onIssueChange(v: string): void {
    issueHandling = v as PhxIssueHandling
  }

  // 手动检查更新：notAvailable / error 用 toast 提示；available 时 Rust 侧已自动开薄窗
  let checking = $state(false)
  function checkNow(): void {
    if (checking) return
    checking = true
    void phxUpdate
      .check(true)
      .then((s) => {
        if (s.phase === 'notAvailable') toast.success(t('update.noUpdate'))
        else if (s.phase === 'error') toast.error(t('update.checkFail'))
      })
      .catch(() => toast.error(t('update.checkFail')))
      .finally(() => (checking = false))
  }

  // 命令面板分组级定位（20 稿图场景 2）：/settings?group=<anchor> 滚到对应设置组。
  // 组 5（sync）在 sessionLoaded 内，未登录时找不到元素静默跳过。
  $effect(() => {
    const g = page.url.searchParams.get('group')
    void loaded
    if (g && loaded) {
      void tick().then(() => {
        document.querySelector(`[data-group="${g}"]`)?.scrollIntoView({ block: 'start' })
      })
    }
  })

  function openDataDir(): void {
    void phxSettings.openDataDir().then(() => toast.success(t('settings.toast.opened')))
  }

  function doReset(): void {
    void phxSettings
      .reset()
      .then(() => toast.success(t('settings.toast.restarting')))
      .then(() => phxSettings.restart())
      .catch(() => toast.error(t('settings.toast.resetFailed')))
  }

  // —— GitHub 账号与云同步（19 稿图，四态互斥：A 未登录 / C 授权中 / B 已登录 / D 同步失败）——
  let logoutOpen = $state(false)

  /** 配对码倒计时 mm:ss */
  const countdownText = $derived.by(() => {
    if (!sync.flow) return ''
    const m = Math.floor(sync.flow.remainingSec / 60)
    const s = sync.flow.remainingSec % 60
    return `${m}:${String(s).padStart(2, '0')}`
  })

  /** 最近一次成功同步的本地化时间（B/D 态身份行 sdesc 用） */
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
</script>

<main class="page">
  <header class="main-head">
    <div>
      <h1 class="large-title">{t('nav.settings')}</h1>
      <p class="head-meta">{t('settings.headMeta')}</p>
    </div>
  </header>

  <div class="body">
    <div class="boot phx-boot" class:gone={!booting}>
      <div class="sgrid">
        <div class="scol">
          <section class="sgroup">
            <div class="sgroup-h"><i class="phx-sk gh-sk"></i></div>
            <div class="sgroup-sub"><i class="phx-sk gs-sk"></i></div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk dd-sk"></i>
            </div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk sw-sk"></i>
            </div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk sw-sk"></i>
            </div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk sw-sk"></i>
            </div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk mbtn-sk"></i>
            </div>
          </section>
          <section class="sgroup">
            <div class="sgroup-h"><i class="phx-sk gh-sk"></i></div>
            <div class="sgroup-sub"><i class="phx-sk gs-sk"></i></div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sp-sk"></i></div>
              <i class="phx-sk mbtn-sk"></i>
            </div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sp-sk"></i></div>
              <i class="phx-sk mbtn-sk"></i>
            </div>
          </section>
        </div>
        <div class="scol">
          <section class="sgroup">
            <div class="sgroup-h"><i class="phx-sk gh-sk"></i></div>
            <div class="sgroup-sub"><i class="phx-sk gs-sk"></i></div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk sw-sk"></i>
            </div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk sw-sk"></i>
            </div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk dd-sk"></i>
            </div>
          </section>
          <section class="sgroup">
            <div class="sgroup-h"><i class="phx-sk gh-sk"></i></div>
            <div class="sgroup-sub"><i class="phx-sk gs-sk"></i></div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk mbtn-sk"></i>
            </div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk mbtn-sk"></i>
            </div>
          </section>
          <!-- 组 5 骨架（GitHub 账号，身份行形态） -->
          <section class="sgroup">
            <div class="sgroup-h"><i class="phx-sk gh-sk"></i></div>
            <div class="sgroup-sub"><i class="phx-sk gs-sk"></i></div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk badge-sk"></i>
            </div>
            <div class="srow">
              <div class="stext"><i class="phx-sk sl-sk"></i><i class="phx-sk sd-sk"></i></div>
              <i class="phx-sk mbtn-sk"></i>
            </div>
          </section>
        </div>
      </div>
    </div>
    {#if loaded}
      <div class="scroll phx-fade-in">
        <div class="sgrid">
        <div class="scol">
          <!-- 组 1 · 通用 -->
          <section class="sgroup" data-group="general">
            <div class="sgroup-h">{t('settings.group.general')}</div>
            <div class="sgroup-sub">{t('settings.group.general.sub')}</div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.language')}</div>
                <div class="sdesc">{t('settings.language.desc')}</div>
              </div>
              <PhxDropdown value={i18n.locale} options={langOptions} onchange={onLangChange} />
            </div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.sound')}</div>
                <div class="sdesc">{t('settings.sound.desc')}</div>
              </div>
              <PhxSwitch bind:checked={soundOn} label={t('settings.sound')} />
            </div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.reducedMotion')}</div>
                <div class="sdesc">{t('settings.reducedMotion.desc')}</div>
              </div>
              <PhxSwitch bind:checked={motionOn} label={t('settings.reducedMotion')} />
            </div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.checkUpdates')}</div>
                <div class="sdesc">{t('settings.checkUpdates.desc')}</div>
              </div>
              <PhxSwitch bind:checked={checkUpdates} label={t('settings.checkUpdates')} />
            </div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.checkNow')}</div>
                <div class="sdesc">{t('settings.checkNow.desc')}</div>
              </div>
              <button class="mbtn" type="button" disabled={checking} onclick={checkNow}>
                {checking ? t('update.checking') : t('settings.checkNow.btn')}
              </button>
            </div>
          </section>

          <!-- 组 2 · 服务器与账号 -->
          <section class="sgroup" data-group="server">
            <div class="sgroup-h">{t('settings.group.server')}</div>
            <div class="sgroup-sub">{t('settings.group.server.sub')}</div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.serverDir')}</div>
                <div class="spath">{serverText}</div>
              </div>
              <a class="mbtn ghost" href={resolve('/server-select')}>{t('settings.changeServer')}</a>
            </div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.account')}</div>
                <div class="spath">{accountText}</div>
              </div>
              <a class="mbtn ghost" href={resolve('/server-select')}>{t('settings.changeServer')}</a>
            </div>
          </section>
        </div>

        <div class="scol">
          <!-- 组 3 · 存档编辑行为 -->
          <section class="sgroup" data-group="behavior">
            <div class="sgroup-h">{t('settings.group.behavior')}</div>
            <div class="sgroup-sub">{t('settings.group.behavior.sub')}</div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.autoMasterings')}</div>
                <div class="sdesc">{t('settings.autoMasterings.desc')}</div>
              </div>
              <PhxSwitch bind:checked={autoAddMasterings} label={t('settings.autoMasterings')} />
            </div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.autoScav')}</div>
                <div class="sdesc">{t('settings.autoScav.desc')}</div>
              </div>
              <PhxSwitch bind:checked={autoAddScavSkills} label={t('settings.autoScav')} />
            </div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.issueHandling')}</div>
                <div class="sdesc">{t('settings.issueHandling.desc')}</div>
              </div>
              <PhxDropdown value={issueHandling} options={issueOptions} onchange={onIssueChange} />
            </div>
          </section>

          <!-- 组 4 · 维护 -->
          <section class="sgroup" data-group="maintenance">
            <div class="sgroup-h">{t('settings.group.maintenance')}</div>
            <div class="sgroup-sub">{t('settings.group.maintenance.sub')}</div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.openDataDir')}</div>
                <div class="sdesc">{t('settings.openDataDir.desc')}</div>
              </div>
              <button class="mbtn ghost" type="button" onclick={openDataDir}>
                {t('settings.btnOpen')}
              </button>
            </div>
            <div class="srow">
              <div class="stext">
                <div class="slabel">{t('settings.resetSettings')}</div>
                <div class="sdesc">{t('settings.resetSettings.desc')}</div>
              </div>
              <button class="mbtn danger" type="button" onclick={() => (resetOpen = true)}>
                {t('settings.btnReset')}
              </button>
            </div>
          </section>

          <!-- 组 5 · GitHub 账号（19 稿图：四态同一时刻只显示一个，A 未登录 / C 授权中 / B 已登录 / D 同步失败） -->
          {#if sync.sessionLoaded}
            <section class="sgroup" data-group="sync">
              <div class="sgroup-h">{t('sync.title')}</div>
              {#if !sync.configured && sync.flow}
                <!-- 状态 C · 授权中 -->
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
              {:else if !sync.configured}
                <!-- 状态 A · 未登录 -->
                <div class="sgroup-sub">{t('sync.subA')}</div>
                <div class="srow">
                  <div class="stext">
                    <div class="slabel">{t('sync.cloudLabel')}</div>
                    <div class="sdesc">{t('sync.cloudDesc')}</div>
                  </div>
                  <button class="mbtn" type="button" onclick={startLogin}>
                    {t('sync.btnLogin')}
                  </button>
                </div>
                <div class="srow">
                  <div class="stext">
                    <div class="slabel">{t('sync.scopeLabel')}</div>
                    <div class="sdesc">{t('sync.scopeDesc')}</div>
                  </div>
                </div>
              {:else if sync.lastError}
                <!-- 状态 D · 同步失败（登录状态正常，给安心话术） -->
                <div class="sgroup-sub">{t('sync.subD')}</div>
                <div class="srow">
                  <div class="id">
                    <div class="avatar" aria-hidden="true">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="8" r="3.6"/><path d="M5 20c1.4-3.6 4-5 7-5s5.6 1.4 7 5"/></svg>
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
              {:else}
                <!-- 状态 B · 已登录（badge 随最近一次同步结果切换，失败见状态 D） -->
                <div class="sgroup-sub">{t('sync.subB')}</div>
                <div class="srow">
                  <div class="id">
                    <div class="avatar" aria-hidden="true">
                      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="8" r="3.6"/><path d="M5 20c1.4-3.6 4-5 7-5s5.6 1.4 7 5"/></svg>
                    </div>
                    <div>
                      <div class="slabel">{sync.login}</div>
                      <div class="sdesc">{t('sync.recentSync', { text: lastSyncText })}</div>
                    </div>
                  </div>
                  <span class="badge ok">{t('sync.badgeOk')}</span>
                </div>
                <div class="srow">
                  <div class="stext">
                    <div class="slabel">{t('sync.scopeLabel')}</div>
                    <div class="sdesc">{t('sync.scopeDescAuto')}</div>
                  </div>
                </div>
                <div class="srow">
                  <div class="stext">
                    <div class="slabel">{t('sync.manualLabel')}</div>
                    <div class="sdesc">{t('sync.manualDesc')}</div>
                  </div>
                  <button class="mbtn" type="button" disabled={sync.syncing} onclick={confirmSync}>
                    {t('sync.btnSyncNow')}
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
              {/if}
            </section>
          {/if}
        </div>
      </div>
    </div>
  {/if}
  </div>
</main>

<PhxDialog
  bind:open={resetOpen}
  heading={t('settings.reset.title')}
  text={t('settings.reset.body')}
  confirmText={t('settings.reset.confirm')}
  danger
  onconfirm={doReset}
/>

<PhxDialog
  bind:open={logoutOpen}
  heading={t('sync.logoutConfirmTitle')}
  text={t('sync.logoutConfirmBody')}
  confirmText={t('sync.logoutConfirmBtn')}
  danger
  onconfirm={logout}
/>

<style>
  /* 容器三段式（对齐 health 等生产页惯例） */
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .main-head {
    padding: 24px 24px 0;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 24px 104px;
  }
  /* 加载态包装层：boot 覆盖层 + 内容同层交叉淡化 */
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .boot {
    padding: 0 24px 104px;
  }
  .boot .gh-sk {
    width: 96px;
    height: 15px;
    border-radius: 6px;
  }
  .boot .gs-sk {
    width: 140px;
    height: 12px;
    border-radius: 6px;
  }
  .boot .sl-sk {
    width: 45%;
    height: 13px;
    border-radius: 6px;
  }
  .boot .sd-sk,
  .boot .sp-sk {
    width: 70%;
    height: 12px;
    border-radius: 6px;
    margin-top: 3px;
  }
  .boot .sp-sk {
    width: 55%;
  }
  .boot .dd-sk {
    width: 190px;
    height: 30px;
    border-radius: 8px;
    flex: none;
  }
  .boot .sw-sk {
    width: 40px;
    height: 24px;
    border-radius: 12px;
    flex: none;
  }
  .boot .mbtn-sk {
    width: 64px;
    height: 26px;
    border-radius: 8px;
    flex: none;
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
  /* 双列设置网格 */
  .sgrid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0 16px;
    align-items: start;
  }
  .scol {
    min-width: 0;
  }
  .sgroup {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow: hidden;
    margin-top: 16px;
  }
  .sgroup-h {
    padding: 14px 18px 2px;
    font-size: 14.5px;
    font-weight: 700;
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
    padding: 13px 18px;
  }
  .srow + .srow,
  .sgroup-sub + .srow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .stext {
    flex: 1;
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
  .spath {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
    color: var(--phx-text-2);
    margin-top: 3px;
    overflow-wrap: anywhere;
  }
  /* 小按钮 */
  .mbtn {
    height: 26px;
    padding: 0 12px;
    border: none;
    border-radius: var(--phx-r-btn);
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    display: inline-flex;
    align-items: center;
    text-decoration: none;
    font-family: inherit;
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
  /* ===== 19 · GitHub 账号分区（token 均为视觉基线在册值） ===== */
  .act {
    display: flex;
    gap: 8px;
    flex: none;
  }
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
  .boot .badge-sk {
    width: 56px;
    height: 22px;
    border-radius: 999px;
    flex: none;
  }
  .id {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .avatar {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .avatar svg {
    width: 15px;
    height: 15px;
  }
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
    margin-inline: auto;
    border: none;
    background: none;
    padding: 0;
    cursor: pointer;
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12px;
    color: var(--phx-accent);
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
