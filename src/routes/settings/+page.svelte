<script lang="ts">
  // 15-settings 设置页（稿图 docs/prototypes/15-settings.html）
  // 组1 含「检查更新」开关 + 手动检查入口（2026-10-19 更新链落地，推翻 10-18 的「砍开关」裁决）。
  // 保存语义（稿图 note7，对齐参考 NotifyPropertyChangedAndSave）：改动即时落盘 + 轻提示，页头无「保存」按钮。
  import { onMount, tick } from 'svelte'
  import { resolve } from '$app/paths'
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
  import PhxSwitch from '$lib/components/PhxSwitch.svelte'
  import PhxDropdown from '$lib/components/PhxDropdown.svelte'
  import PhxDialog from '$lib/components/PhxDialog.svelte'

  let loaded = $state(false)
  let resetOpen = $state(false)

  // —— 感官层偏好（localStorage 通道：phx.sound / phx.motion）——
  let soundOn = $state(isSoundEnabled())
  let motionOn = $state(isReducedMotion())
  $effect(() => setSoundEnabled(soundOn))
  $effect(() => setReducedMotion(motionOn))

  // —— 行为偏好（settings.json 通道，Rust 侧整对象写入；自动补全消费端待 13 屏接入）——
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
</script>

<main class="page">
  <header class="main-head">
    <div>
      <h1 class="large-title">{t('nav.settings')}</h1>
      <p class="head-meta">{t('settings.headMeta')}</p>
    </div>
  </header>

  {#if loaded}
    <div class="scroll">
      <div class="sgrid">
        <div class="scol">
          <!-- 组 1 · 通用 -->
          <section class="sgroup">
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
          <section class="sgroup">
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
          <section class="sgroup">
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
          <section class="sgroup">
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
        </div>
      </div>
    </div>
  {/if}
</main>

<PhxDialog
  bind:open={resetOpen}
  heading={t('settings.reset.title')}
  text={t('settings.reset.body')}
  confirmText={t('settings.reset.confirm')}
  danger
  onconfirm={doReset}
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
    padding: 0 24px 8px;
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
</style>
