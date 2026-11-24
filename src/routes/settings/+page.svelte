<script lang="ts">
  // 15-settings 设置页（稿图 docs/prototypes/15-settings.html）
  // 组1 含「检查更新」开关 + 手动检查入口（2026-10-19 更新链落地，推翻 10-18 的「砍开关」裁决）。
  // 保存语义（稿图 note7，对齐参考 NotifyPropertyChangedAndSave）：改动即时落盘 + 轻提示，页头无「保存」按钮。
  import { onMount, tick } from 'svelte'
  import { page } from '$app/state'
  import { resolve } from '$app/paths'
  import { t, i18n, setLocale, type Locale } from '$lib/i18n/i18n.svelte'
  import { theme, setThemeMode, type ThemeMode } from '$lib/theme.svelte'
  import { isSoundEnabled, setSoundEnabled } from '$lib/sound'
  import { isReducedMotion, setReducedMotion } from '$lib/motion'
  import {
    phxSettings,
    phxServer,
    phxProfile,
    phxUpdate,
    phxAi,
    type PhxAiConfigView,
    type PhxIssueHandling,
    type PhxProfileMeta,
    type PhxServerRow
  } from '$lib/ipc'
  import { toast } from '$lib/toast.svelte'
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
  let showSponsor = $state(true)
  let issueHandling = $state<PhxIssueHandling>('ask')

  // armed 门：onMount 回填触发的 $effect 静默跑过，用户交互后才真正保存+提示。
  // armed 刻意不用 $state —— 回填后置 true 不应反过来再触发本 effect。
  let armed = false
  $effect(() => {
    const c = checkUpdates
    const a = autoAddMasterings
    const s = autoAddScavSkills
    const sp = showSponsor
    const h = issueHandling
    const aiCfg = ai
    if (!armed || !aiCfg) return
    void phxSettings
      .set({
        checkUpdates: c,
        autoAddMasterings: a,
        autoAddScavSkills: s,
        showSponsor: sp,
        issueHandling: h,
        ai: {
          enabled: aiCfg.enabled,
          provider: aiCfg.provider,
          baseUrl: aiCfg.baseUrl,
          model: aiCfg.model,
          pageEntries: aiCfg.pageEntries
        }
      })
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
      const [settings, server, pid, metas, aiCfg] = await Promise.all([
        phxSettings.get(),
        phxServer.current(),
        phxProfile.current(),
        phxProfile.list(),
        phxAi.getConfig().catch(() => null)
      ])
      checkUpdates = settings.checkUpdates
      autoAddMasterings = settings.autoAddMasterings
      autoAddScavSkills = settings.autoAddScavSkills
      showSponsor = settings.showSponsor
      issueHandling = settings.issueHandling
      if (aiCfg) ai = aiCfg
      // 兜底：AI 视图加载失败也不能卡死整页保存链（settings.ai 自带同值字段）
      else ai = { ...settings.ai, hasKey: false, keyHint: null }
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

  const themeOptions = $derived([
    { value: 'dark', label: t('settings.theme.dark') },
    { value: 'light', label: t('settings.theme.light') },
    { value: 'system', label: t('settings.theme.system') }
  ])

  function onThemeChange(v: string): void {
    setThemeMode(v as ThemeMode)
    toast.success(t('settings.toast.saved'))
  }

  function onIssueChange(v: string): void {
    issueHandling = v as PhxIssueHandling
  }

  // 手动检查更新：notAvailable / error 用 toast 提示；available 时 Rust 侧已自动开薄窗
  let checking = $state(false)
  /* dev-only 更新窗预览入口（正式构建静态剔除） */
  const devBuild = import.meta.env.DEV

  // —— AI 助手（稿图 30）：三件套+开关走 settings.json（ai 字段），密钥走凭据库独立通道 ——
  let ai = $state<PhxAiConfigView | null>(null)
  let aiKeyEditing = $state(false)
  let aiKeyInput = $state('')
  let aiSavingKey = $state(false)
  let aiTesting = $state(false)
  let aiTest = $state<{ ok: boolean; msg: string } | null>(null)
  let ollama = $state<{ ok: boolean; models: string[] } | null>(null)

  const aiProviderOptions = $derived([
    { value: 'deepseek', label: 'DeepSeek' },
    { value: 'glm', label: 'GLM' },
    { value: 'siliconflow', label: 'SiliconFlow' },
    { value: 'ollama', label: 'Ollama' },
    { value: 'custom', label: t('settings.ai.provider.custom') }
  ])
  const AI_PRESETS: Record<string, { baseUrl: string; model: string }> = {
    deepseek: { baseUrl: 'https://api.deepseek.com/v1', model: 'deepseek-chat' },
    glm: { baseUrl: 'https://open.bigmodel.cn/api/paas/v4', model: 'glm-4.5-flash' },
    siliconflow: { baseUrl: 'https://api.siliconflow.cn/v1', model: 'Qwen/Qwen3-8B' },
    ollama: { baseUrl: 'http://127.0.0.1:11434/v1', model: 'qwen3:8b' }
  }
  function onAiProviderChange(v: string): void {
    if (!ai) return
    ai.provider = v
    const preset = AI_PRESETS[v]
    if (preset) {
      ai.baseUrl = preset.baseUrl
      ai.model = preset.model
    }
  }
  const aiProviderLabel = $derived(
    ai ? (aiProviderOptions.find((o) => o.value === ai?.provider)?.label ?? ai.provider) : ''
  )
  const aiStatus = $derived.by(() => {
    if (!ai) return { ok: false, label: t('settings.ai.status.loading') }
    if (!ai.enabled) return { ok: false, label: t('settings.ai.status.off') }
    if (ai.hasKey || ai.provider === 'ollama')
      return { ok: true, label: `${t('settings.ai.status.ready')} · ${aiProviderLabel}` }
    return { ok: false, label: t('settings.ai.status.unset') }
  })

  async function aiSaveKey(): Promise<void> {
    if (!aiKeyInput.trim()) return
    aiSavingKey = true
    try {
      await phxAi.setKey(aiKeyInput.trim())
      aiKeyInput = ''
      aiKeyEditing = false
      ai = await phxAi.getConfig()
      toast.success(t('settings.ai.key.saved'))
    } catch (e) {
      toast.error(String(e))
    } finally {
      aiSavingKey = false
    }
  }
  async function aiClearKeyFn(): Promise<void> {
    try {
      await phxAi.clearKey()
      ai = await phxAi.getConfig()
    } catch (e) {
      toast.error(String(e))
    }
  }
  async function aiTestRun(): Promise<void> {
    aiTesting = true
    aiTest = null
    try {
      aiTest = { ok: true, msg: await phxAi.testConnection() }
    } catch (e) {
      aiTest = { ok: false, msg: String(e) }
    } finally {
      aiTesting = false
    }
  }
  async function ollamaDetect(): Promise<void> {
    const r = await phxAi.detectOllama().catch(() => null)
    ollama = r ? { ok: true, models: r } : { ok: false, models: [] }
  }

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
                <div class="slabel">{t('settings.theme')}</div>
                <div class="sdesc">{t('settings.theme.desc')}</div>
              </div>
              <PhxDropdown value={theme.mode} options={themeOptions} onchange={onThemeChange} />
            </div>
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
                <div class="slabel">{t('settings.showSponsor')}</div>
                <div class="sdesc">{t('settings.showSponsor.desc')}</div>
              </div>
              <PhxSwitch bind:checked={showSponsor} label={t('settings.showSponsor')} />
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
            {#if devBuild}
              <!-- 仅开发构建渲染（vite 生产构建静态剔除）：更新薄窗三态视觉调试入口 -->
              <div class="srow">
                <div class="stext">
                  <div class="slabel">更新窗预览（仅开发）</div>
                  <div class="sdesc">广播假状态并打开更新薄窗，逐态核对高度自适应；正式包无此项。</div>
                </div>
                <div class="dev-mock">
                  <button class="mbtn ghost" type="button" onclick={() => void phxUpdate.devMock('available')}>发现新版本</button>
                  <button class="mbtn ghost" type="button" onclick={() => void phxUpdate.devMock('downloading')}>下载中</button>
                  <button class="mbtn ghost" type="button" onclick={() => void phxUpdate.devMock('downloaded')}>待重启</button>
                </div>
              </div>
            {/if}
          </section>

          <!-- 组 1.5 · AI 助手（稿图 30）：三件套+开关走 settings 通道，密钥走凭据库，Ollama 本地免鉴权 -->
          <section class="sgroup" data-group="ai">
            <div class="sgroup-h">
              {t('settings.ai.group')}
              <span class="aitag" class:ok={aiStatus.ok}>{aiStatus.label}</span>
            </div>
            <div class="sgroup-sub">{t('settings.ai.group.sub')}</div>
            {#if ai}
              <div class="srow">
                <div class="stext">
                  <div class="slabel">{t('settings.ai.enable')}</div>
                  <div class="sdesc">{t('settings.ai.enable.desc')}</div>
                </div>
                <PhxSwitch bind:checked={ai.enabled} label={t('settings.ai.enable')} />
              </div>
              <div class="srow">
                <div class="stext">
                  <div class="slabel">{t('settings.ai.provider')}</div>
                  <div class="sdesc">{t('settings.ai.provider.desc')}</div>
                </div>
                <PhxDropdown value={ai.provider} options={aiProviderOptions} onchange={onAiProviderChange} />
              </div>
              {#if ai.provider !== 'ollama'}
                <div class="srow">
                  <div class="stext">
                    <div class="slabel">{t('settings.ai.baseurl')}</div>
                  </div>
                  <input
                    class="tinput"
                    value={ai.baseUrl}
                    onchange={(e) => ai && (ai.baseUrl = e.currentTarget.value)}
                    spellcheck="false"
                  />
                </div>
                <div class="srow">
                  <div class="stext">
                    <div class="slabel">{t('settings.ai.key')}</div>
                    <div class="sdesc">{t('settings.ai.key.desc')}</div>
                  </div>
                  {#if aiKeyEditing}
                    <div class="ai-inline">
                      <input
                        class="tinput"
                        type="password"
                        bind:value={aiKeyInput}
                        placeholder={t('settings.ai.key.placeholder')}
                      />
                      <button class="mbtn" type="button" disabled={aiSavingKey} onclick={aiSaveKey}>
                        {t('settings.ai.key.save')}
                      </button>
                      <button class="mbtn ghost" type="button" onclick={() => { aiKeyEditing = false; aiKeyInput = '' }}>
                        {t('settings.ai.key.cancel')}
                      </button>
                    </div>
                  {:else if ai.hasKey}
                    <div class="ai-inline">
                      <span class="tinput tinput-ro">•••••••• {ai.keyHint ?? ''}</span>
                      <button class="mbtn ghost" type="button" onclick={() => (aiKeyEditing = true)}>
                        {t('settings.ai.key.change')}
                      </button>
                      <button class="mbtn ghost" type="button" onclick={aiClearKeyFn}>
                        {t('settings.ai.key.clear')}
                      </button>
                    </div>
                  {:else}
                    <button class="mbtn" type="button" onclick={() => (aiKeyEditing = true)}>
                      {t('settings.ai.key.setup')}
                    </button>
                  {/if}
                </div>
              {:else}
                <div class="srow">
                  <div class="stext">
                    <div class="slabel">{t('settings.ai.ollama')}</div>
                    <div class="sdesc">{t('settings.ai.ollama.desc')}</div>
                  </div>
                  {#if ollama?.ok}
                    <span class="aitag ok">{t('settings.ai.ollama.found', { n: ollama.models.length })}</span>
                  {:else if ollama}
                    <span class="aitag err">{t('settings.ai.ollama.missing')}</span>
                  {:else}
                    <button class="mbtn" type="button" onclick={ollamaDetect}>
                      {t('settings.ai.ollama.check')}
                    </button>
                  {/if}
                </div>
              {/if}
              <div class="srow">
                <div class="stext">
                  <div class="slabel">{t('settings.ai.model')}</div>
                </div>
                <input
                  class="tinput"
                  value={ai.model}
                  onchange={(e) => ai && (ai.model = e.currentTarget.value)}
                  spellcheck="false"
                />
              </div>
              <div class="srow">
                <div class="stext">
                  <div class="slabel">{t('settings.ai.test')}</div>
                  <div class="sdesc">{t('settings.ai.test.desc')}</div>
                  {#if aiTest}
                    <div class="aitest" class:ok={aiTest.ok}>{aiTest.msg}</div>
                  {/if}
                </div>
                <button class="mbtn" type="button" disabled={aiTesting} onclick={aiTestRun}>
                  {aiTesting ? t('settings.ai.testing') : t('settings.ai.test.btn')}
                </button>
              </div>
              <div class="srow">
                <div class="stext">
                  <div class="slabel">{t('settings.ai.entries')}</div>
                  <div class="sdesc">{t('settings.ai.entries.desc')}</div>
                </div>
                <PhxSwitch bind:checked={ai.pageEntries} label={t('settings.ai.entries')} />
              </div>
              <div class="priv">
                <b>{t('settings.ai.privacy.label')}</b>{t('settings.ai.privacy')}
              </div>
            {/if}
          </section>

        </div>

        <div class="scol">
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
  .dev-mock {
    display: flex;
    gap: 8px;
    flex: none;
  }
  /* ===== AI 助手组（稿图 30） ===== */
  .aitag {
    font-size: 10.5px;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 5px;
    background: var(--phx-fill-quaternary);
    color: var(--phx-text-3);
  }
  .aitag.ok {
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .aitag.err {
    background: var(--phx-danger-bg);
    color: var(--phx-danger);
  }
  .tinput {
    height: 30px;
    padding: 0 10px;
    border: none;
    border-radius: 7px;
    background: var(--phx-fill-tertiary);
    color: var(--phx-text-2);
    font-size: 12px;
    font-family: var(--phx-font-mono);
    min-width: 0;
    outline: none;
  }
  .tinput:focus {
    box-shadow: inset 0 0 0 1px var(--phx-accent);
    color: var(--phx-text);
  }
  .tinput-ro {
    display: inline-flex;
    align-items: center;
    width: 150px;
    letter-spacing: 2px;
  }
  .ai-inline {
    display: flex;
    gap: 8px;
    align-items: center;
    flex: none;
  }
  .aitest {
    margin-top: 6px;
    font-size: 11.5px;
    color: var(--phx-danger);
    line-height: 1.6;
    word-break: break-all;
  }
  .aitest.ok {
    color: var(--phx-green);
  }
  .priv {
    margin: 4px 18px 16px;
    padding: 12px 14px;
    border-radius: 10px;
    background: var(--phx-warn-bg);
    box-shadow: inset 0 0 0 0.5px var(--phx-warn-bd);
    font-size: 12px;
    color: var(--phx-text-2);
    line-height: 1.7;
  }
  .priv b {
    color: var(--phx-warn);
    font-weight: 600;
    margin-right: 4px;
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
</style>
