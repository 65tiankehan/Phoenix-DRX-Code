<script lang="ts">
  // 03 角色页：基本信息 / 成长 / 外观与装备 / 健康状态，按 docs/prototypes/03 稿图逐项落地
  // 26 稿新增填充：成长经验脚注 / 装备速览（实读仓库装备槽）/ Scav 卡账号 ID 与版本
  import { onMount } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { goto } from '$app/navigation'
  import { resolve } from '$app/paths'
  import { listen } from '@tauri-apps/api/event'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import {
    phxChar,
    phxProfile,
    phxStash,
    phxScav,
    type PhxCharHealth,
    type PhxCharOptions,
    type PhxCharSide,
    type PhxStashData,
    type PhxScavData,
    phxClothing,
    type PhxClothingData
  } from '$lib/ipc'
  import PhxDropdown from '$lib/components/PhxDropdown.svelte'
  import PhxSegmented from '$lib/components/PhxSegmented.svelte'
  import PhxSwitch from '$lib/components/PhxSwitch.svelte'
  import PhxDialog from '$lib/components/PhxDialog.svelte'

  type Mode = 'pmc' | 'scav'
  /** 页内分区：概览 / 形象服装（稿图 31；导航裁决 2026-10-24） */
  type View = 'overview' | 'clothing'

  /** 骨架镜像占位行序号（boot 层内部件用） */
  const skRows = (n: number) => [...Array(n).keys()]

  let currentId = $state('')
  let meta = $state<{
    side: string
    aid: string
    gameVersion: string
    prestigeMax: number
    prestigeRecords: number
    prestigePending: number | null
  } | null>(null)
  let pmc = $state<PhxCharSide | null>(null)
  let scav = $state<PhxCharSide | null>(null)
  let opts = $state<PhxCharOptions | null>(null)
  let mode = $state<Mode>('pmc')
  let saving = $state(false)
  /** 「准备转生」确认弹窗开关与进行中标记 */
  let prepOpen = $state(false)
  let prepBusy = $state(false)
  let booting = $state(true)
  // 26 稿装备速览：PMC 走仓库装备槽，Scav 走侦察兵装备槽
  let pmcStash = $state<PhxStashData | null>(null)
  let scavEquip = $state<PhxScavData | null>(null)
  /** 经验输入框显示文本（千分位） */
  let expText = $state('')

  const cur = $derived(mode === 'pmc' ? pmc : scav)

  /** 健康九格：稿图顺序（右腿在前） */
  const HEALTH_CELLS: { key: keyof PhxCharHealth }[] = [
    { key: 'head' },
    { key: 'chest' },
    { key: 'stomach' },
    { key: 'rightArm' },
    { key: 'leftArm' },
    { key: 'rightLeg' },
    { key: 'leftLeg' },
    { key: 'hydration' },
    { key: 'energy' }
  ]

  /** 经验换算（参考 CharacterInfo.cs 实证口径）：expStart(level) = prefix(level) */
  function expStart(level: number): number {
    const table = opts?.expTable ?? []
    let acc = 0
    for (let i = 0; i < level && i < table.length; i++) acc += table[i]
    return acc
  }

  /** expEnd(level) = prefix(level) + table[level]；满级取表尾 +100000（对齐参考实现） */
  function expEnd(level: number): number {
    const table = opts?.expTable ?? []
    if (level < table.length) return expStart(level) + table[level]
    return expStart(table.length) + 100000
  }

  /** 最高等级 = 表长 - 1（80 项经验表 → 最高 79 级） */
  const maxLevel = $derived(Math.max(1, (opts?.expTable.length ?? 2) - 1))

  /** 经验 → 等级（循环口径照搬参考实现；载入 0 级由 normalizeSide 归 1） */
  function levelFromExp(exp: number): number {
    const table = opts?.expTable ?? []
    let acc = 0
    let lvl = 0
    for (let i = 0; i < table.length; i++) {
      if (exp < acc) break
      acc += table[i]
      lvl = i
    }
    return lvl
  }

  /** 数值口径统一：非负整数 */
  function snap(n: number): number {
    return Math.max(0, Math.trunc(Number(n) || 0))
  }

  /**
   * 体征口径：保留小数、只挡负数。
   * EFT 的血量是真小数（如 34.649），按整数截断会让每次保存都把玩家血量改小，
   * 所以这里不做 trunc，显示与写回都按原值走。
   */
  function vitalNum(n: number): number {
    const v = Number(n)
    return Number.isFinite(v) ? Math.max(0, v) : 0
  }

  /** 载入侧归一：0 级显示为 1 级（与稿图语义一致） */
  function normalizeSide(s: PhxCharSide): PhxCharSide {
    s.level = Math.max(1, s.level)
    s.experience = Math.max(0, s.experience)
    const h = s.health
    for (const k of Object.keys(h) as (keyof typeof h)[]) {
      h[k].current = vitalNum(h[k].current)
      h[k].maximum = vitalNum(h[k].maximum)
    }
    return s
  }

  function syncExpText(s: PhxCharSide | null): void {
    expText = s ? s.experience.toLocaleString('en-US') : ''
  }

  /** 全量刷新：当前档案 → 全量角色数据 + 选项（服务器经验表 / 语音 / 头型 / 口袋） */
  async function refresh(): Promise<void> {
    booting = true
    const id = await phxProfile.current()
    currentId = id ?? ''
    if (!currentId) {
      meta = null
      pmc = null
      scav = null
      pmcStash = null
      scavEquip = null
      syncExpText(null)
      booting = false
      return
    }
    const [data, options, stashData, scavData] = await Promise.all([
      phxChar.load(currentId),
      phxChar.options(),
      // 装备速览数据（26 稿）：仓库装备槽 + Scav 装备槽，随页签取用
      phxStash.load(currentId),
      phxScav.load(currentId)
    ])
    opts = options
    pmcStash = stashData
    scavEquip = scavData
    if (!data) {
      meta = null
      pmc = null
      scav = null
      syncExpText(null)
      booting = false
      return
    }
    meta = {
      side: data.side,
      aid: data.aid,
      gameVersion: data.gameVersion,
      prestigeMax: data.prestigeMax ?? 0,
      prestigeRecords: data.prestigeRecords ?? 0,
      prestigePending: data.prestigePending ?? null
    }
    pmc = normalizeSide(data.pmc)
    scav = normalizeSide(data.scav)
    syncExpText(cur)
    booting = false
  }

  /** 经验输入：剥离非数字，同步等级（只升不降怪癖不复刻，直接按表换算） */
  function onExpInput(): void {
    if (!cur) return
    const digits = expText.replace(/[^\d]/g, '')
    const exp = snap(Number(digits))
    cur.experience = exp
    expText = exp.toLocaleString('en-US')
    cur.level = Math.max(1, levelFromExp(exp))
  }

  /** 等级步进：钳 1..maxLevel，等级变化时把经验推到新区间起点 */
  function onStep(delta: number): void {
    if (!cur) return
    const next = Math.min(maxLevel, Math.max(1, cur.level + delta))
    if (next === cur.level) return
    cur.level = next
    cur.experience = expStart(next)
    syncExpText(cur)
  }

  /** 转生等级上限：服务端模板读不到（0）时整行不出现，不猜一个上限出来 */
  const prestigeMax = $derived(meta?.prestigeMax ?? 0)

  /** 转生步进：钳 0..上限；Scav 没有转生，只有 PMC 可改 */
  function onPrestigeStep(delta: number): void {
    if (!cur || mode !== 'pmc' || prestigeMax <= 0) return
    cur.prestige = Math.min(prestigeMax, Math.max(0, cur.prestige + delta))
  }

  /** 经验滑条：映射当前等级区间 [expStart, expEnd) */
  function onSlide(e: PointerEvent): void {
    if (!cur) return
    const el = e.currentTarget as HTMLDivElement
    el.setPointerCapture(e.pointerId)
    const rect = el.getBoundingClientRect()
    const ratio = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width))
    const start = expStart(cur.level)
    const end = expEnd(cur.level)
    cur.experience = Math.floor(start + ratio * Math.max(1, end - start - 1))
    syncExpText(cur)
  }

  /** 滑条键盘支持：方向键微调，Home/End 到区间两端 */
  function onSlideKey(e: KeyboardEvent): void {
    if (!cur) return
    const start = expStart(cur.level)
    const end = expEnd(cur.level)
    const span = Math.max(1, end - start - 1)
    let ratio = slideRatio
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') ratio += 0.01
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') ratio -= 0.01
    else if (e.key === 'Home') ratio = 0
    else if (e.key === 'End') ratio = 1
    else return
    e.preventDefault()
    ratio = Math.min(1, Math.max(0, ratio))
    cur.experience = Math.floor(start + ratio * span)
    syncExpText(cur)
  }

  const slideRatio = $derived.by(() => {
    if (!cur) return 0
    const start = expStart(cur.level)
    const end = expEnd(cur.level)
    const span = Math.max(1, end - start - 1)
    return Math.min(1, Math.max(0, (cur.experience - start) / span))
  })

  /** 成长脚注（26 稿）：本级经验区间 + 距下一级差额；满级时整行换「已达最高等级」 */
  const expFoot = $derived.by(() => {
    if (!cur) return null
    return {
      start: expStart(cur.level),
      end: expEnd(cur.level),
      left: Math.max(0, expEnd(cur.level) - cur.experience),
      maxed: cur.level >= maxLevel
    }
  })

  /** 装备速览三格（26 稿）：主武器/护甲/头盔，随 PMC/Scav 页签实读装备槽；空槽显示「未装备」 */
  const EQ_SLOTS = ['FirstPrimaryWeapon', 'ArmorVest', 'Headwear'] as const
  const eqRows = $derived.by(() => {
    const list = mode === 'pmc' ? pmcStash?.equipment : scavEquip?.equipment
    if (!list) return []
    return EQ_SLOTS.map((slot) => ({
      slot,
      name: list.find((e) => e.slot === slot)?.item?.name ?? ''
    }))
  })

  const metaText = $derived.by(() => {
    if (!meta) return t('char.meta.none')
    const v = meta.gameVersion
    return v ? t('char.meta.loaded', { v }) : t('char.meta.loadedPlain')
  })

  function setVital(key: keyof PhxCharHealth, part: 'current' | 'maximum', raw: string): void {
    if (!cur) return
    cur.health[key][part] = vitalNum(Number(raw))
  }

  const voiceOpts = $derived((opts?.voices ?? []).map((o) => ({ value: o.id, label: o.name })))
  const headOpts = $derived((opts?.heads ?? []).map((o) => ({ value: o.id, label: o.name })))
  const pocketOpts = $derived((opts?.pockets ?? []).map((p) => ({ value: p.id, label: p.name })))

  function vitalPct(value: number, max: number): number {
    return max > 0 ? Math.min(100, (value / max) * 100) : 0
  }

  /** 保存修改：写回前主进程强制自动备份（领域铁律）；数值全部强转非负整数 */
  // —— 形象与服装（稿图 31）：owned 集合与 head/voice 本地编辑，保存走 clothing_save（含备份）——
  let view = $state<View>('overview')
  let cloth = $state<PhxClothingData | null>(null)
  let clothOwned = new SvelteSet<string>()
  let clothHead = $state('')
  let clothVoice = $state('')
  let clothLoaded = $state(false)

  const clothDirty = $derived.by(() => {
    if (!cloth) return false
    const sameOwned =
      clothOwned.size === cloth.ownedCount &&
      cloth.suits.every((s) => clothOwned.has(s.suiteId) === s.owned)
    return !sameOwned || clothHead !== cloth.currentHead || clothVoice !== cloth.currentVoice
  })
  const unlockAll = $derived(cloth ? cloth.suits.length > 0 && clothOwned.size === cloth.suits.length : false)

  async function refreshClothing(): Promise<void> {
    try {
      const d = await phxClothing.load()
      cloth = d
      // SvelteSet 原地变更（重赋值会触发 non_reactive_update 与 no-unnecessary-state-wrap 两规打架）
      clothOwned.clear()
      for (const s of d.suits.filter((x) => x.owned)) clothOwned.add(s.suiteId)
      clothHead = d.currentHead
      clothVoice = d.currentVoice
      clothLoaded = true
    } catch (e) {
      toast.error(String(e))
    }
  }
  function toggleSuit(id: string): void {
    if (clothOwned.has(id)) clothOwned.delete(id)
    else clothOwned.add(id)
  }
  function setAllOwned(on: boolean): void {
    clothOwned.clear()
    if (on && cloth) for (const s of cloth.suits) clothOwned.add(s.suiteId)
  }
  async function onClothingSave(): Promise<void> {
    if (!cloth || saving) return
    saving = true
    try {
      await phxClothing.save([...clothOwned], clothHead, clothVoice)
      toast.success(t('cloth.saved'))
      await refreshClothing()
    } catch (e) {
      toast.error(String(e))
    } finally {
      saving = false
    }
  }

  async function onSave(): Promise<void> {
    if (!pmc || !scav || saving) return
    saving = true
    try {
      await phxChar.save(currentId, { pmc: cleanSide(pmc), scav: cleanSide(scav) })
      toast.success(t('char.toast.saved'))
    } catch {
      toast.error(t('char.toast.failed'))
    } finally {
      saving = false
    }
  }

  /** 写回前的清洗：等级至少 1、经验取整、体征保留小数（不截断，否则每次保存都掉血） */
  function cleanSide(s: PhxCharSide): PhxCharSide {
    const h = { ...s.health }
    for (const k of Object.keys(h) as (keyof typeof h)[]) {
      h[k] = { current: vitalNum(h[k].current), maximum: vitalNum(h[k].maximum) }
    }
    return {
      ...s,
      level: Math.max(1, snap(s.level)),
      experience: snap(s.experience),
      health: h
    }
  }

  /**
   * 准备转生：先保存当前编辑（拿准当前转生等级），再写 `spt.pendingPrestige`。
   * 真正的清档 / 技能转移 / 奖励发放由服务器在玩家进游戏「重置角色」时执行 —— 编辑器只写标记。
   */
  async function onPreparePrestige(): Promise<void> {
    if (!pmc || !scav || prepBusy || !currentId) return
    prepBusy = true
    try {
      await phxChar.save(currentId, { pmc: cleanSide(pmc), scav: cleanSide(scav) })
      const res = await phxChar.prestigePrepare(currentId, pmc.prestige + 1)
      toast.success(t('char.prestige.prepared', { n: res.level }))
      await refresh()
    } catch (e) {
      toast.error(String(e))
    } finally {
      prepBusy = false
      prepOpen = false
    }
  }

  /** 取消已准备的转生 */
  async function onCancelPrestige(): Promise<void> {
    if (prepBusy || !currentId) return
    prepBusy = true
    try {
      await phxChar.prestigeCancel(currentId)
      toast.success(t('char.prestige.cancelled'))
      await refresh()
    } catch (e) {
      toast.error(String(e))
    } finally {
      prepBusy = false
    }
  }

  onMount(() => {
    void refresh()
    void refreshClothing()
    // 托盘「切换存档」后刷新角色页（Rust 侧 emit；本页卸载时退订）
    const off = listen('tray:profile-switched', () => void refresh())
    return () => {
      void off.then((unlisten) => unlisten())
    }
  })
</script>

<div class="page">
  <div class="main-head">
    <div>
      <div class="large-title">{t('nav.character')}</div>
      <div class="head-meta">{metaText}</div>
    </div>
    <div class="head-actions">
      <PhxSegmented
        size="md"
        role="tablist"
        ariaLabel={t('char.view.aria')}
        bind:value={view}
        options={[
          { value: 'overview', label: t('char.view.overview') },
          { value: 'clothing', label: t('char.view.clothing') }
        ]}
      />
      {#if view === 'overview'}
        <PhxSegmented
          size="md"
          role="tablist"
          ariaLabel={t('char.mode.aria')}
          bind:value={mode}
          onchange={() => syncExpText(cur)}
          options={[
            { value: 'pmc', label: t('char.mode.pmc') },
            { value: 'scav', label: t('char.mode.scav') }
          ]}
        />
      {/if}
      {#if view === 'clothing'}
        <button
          type="button"
          class="btn primary"
          disabled={!cloth || saving || !clothDirty}
          onclick={() => void onClothingSave()}
        >
          {t('char.act.save')}
        </button>
      {:else}
        <button
          type="button"
          class="btn primary"
          disabled={!cur || saving}
          onclick={() => void onSave()}
        >
          {t('char.act.save')}
        </button>
      {/if}
    </div>
  </div>

  <div class="scroll">
    {#if view === 'clothing'}
      <!-- ===== 形象与服装（稿图 31；落地归属=角色页分区 tab，2026-10-24 导航裁决） ===== -->
      {#if cloth}
        <div class="cloth-wrap phx-fade-in">
          <div class="cloth-look">
            <div class="cloth-avatar">P</div>
            <div class="cloth-look-meta">
              <div class="cloth-look-name">
                {(cloth.heads.find((h) => h.id === clothHead)?.name ?? clothHead) || t('cloth.look.unset')}
              </div>
              <div class="cloth-look-sub">
                {t('cloth.look.sub', {
                  head: cloth.heads.find((h) => h.id === clothHead)?.name ?? clothHead,
                  voice: cloth.voices.find((v) => v.id === clothVoice)?.name ?? clothVoice
                })}
              </div>
            </div>
            <div class="cloth-look-dd">
              <span class="cloth-dd-l">{t('cloth.head')}</span>
              <PhxDropdown
                value={clothHead}
                options={cloth.heads.map((h) => ({ value: h.id, label: h.name }))}
                onchange={(v) => (clothHead = v)}
              />
              <span class="cloth-dd-l">{t('cloth.voice')}</span>
              <PhxDropdown
                value={clothVoice}
                options={cloth.voices.map((v) => ({ value: v.id, label: v.name }))}
                onchange={(v) => (clothVoice = v)}
              />
            </div>
          </div>

          <div class="cloth-sec-h">
            <span>{t('cloth.suits', { owned: clothOwned.size, total: cloth.suits.length })}</span>
            <span class="cloth-all">
              {t('cloth.unlockAll')}
              <PhxSwitch checked={unlockAll} label={t('cloth.unlockAll')} onchange={setAllOwned} />
            </span>
          </div>
          {#if cloth.suits.length === 0}
            <div class="cloth-empty">{t('cloth.empty')}</div>
          {:else}
            <div class="cloth-grid">
              {#each cloth.suits as suit (suit.suiteId)}
                <div class="cloth-card" class:owned={clothOwned.has(suit.suiteId)}>
                  <div class="cloth-thumb">
                    <svg viewBox="0 0 96 96" fill="none">
                      <path class="art-body" d="M30 24 Q36 14 48 14 Q60 14 66 24 L74 36 L64 40 L64 82 Q48 88 32 82 L32 40 L22 36 Z"/>
                      <path class="art-line" d="M48 20 L48 82"/>
                    </svg>
                    <span class="cloth-st" class:on={clothOwned.has(suit.suiteId)}>
                      {clothOwned.has(suit.suiteId) ? t('cloth.state.owned') : t('cloth.state.locked')}
                    </span>
                  </div>
                  <div class="cloth-card-body">
                    <div class="cloth-n">{suit.name}</div>
                    <button class="cloth-btn" type="button" onclick={() => toggleSuit(suit.suiteId)}>
                      {clothOwned.has(suit.suiteId) ? t('cloth.act.lock') : t('cloth.act.unlock')}
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {:else if clothLoaded}
        <div class="cloth-empty">{t('cloth.loadFail')}</div>
      {:else}
        <div class="cloth-empty">{t('cloth.loading')}</div>
      {/if}
    {:else}
    <!-- 骨架层：镜像 5fr/7fr 仪表台真实布局；数据就绪加 .gone 淡出，与新内容交叉淡化（感官层基线 §二） -->
    <div class="boot phx-boot" class:gone={!booting}>
      <div class="dash">
        <section class="panel">
          <div class="panel-head">
            <i class="phx-sk" style="width:58px"></i>
            <i class="phx-sk slim" style="width:96px"></i>
          </div>
          <div class="frow"><i class="phx-sk slim k"></i><i class="phx-sk fld grow"></i></div>
          <div class="frow"><i class="phx-sk slim k"></i><i class="phx-sk seg"></i></div>
          <div class="frow"><i class="phx-sk slim k"></i><i class="phx-sk grow"></i><i class="phx-sk tag"></i></div>
          <div class="frow"><i class="phx-sk slim k"></i><i class="phx-sk grow"></i><i class="phx-sk tag"></i></div>
        </section>
        <section class="panel">
          <div class="panel-head">
            <i class="phx-sk" style="width:58px"></i>
            <i class="phx-sk slim" style="width:110px"></i>
          </div>
          <div class="frow">
            <i class="phx-sk slim k"></i>
            <i class="phx-sk lvln"></i>
            <i class="phx-sk step"></i>
            <i class="phx-sk slim rng"></i>
          </div>
          <div class="frow">
            <i class="phx-sk slim k"></i>
            <i class="phx-sk expf"></i>
            <i class="phx-sk trk grow"></i>
          </div>
          <div class="exp-foot"><i class="phx-sk slim" style="width:150px"></i><i class="phx-sk slim" style="width:120px"></i></div>
          <i class="phx-sk slim hint-sk" style="width:188px"></i>
        </section>
        <section class="panel">
          <div class="panel-head">
            <i class="phx-sk" style="width:92px"></i>
            <i class="phx-sk slim" style="width:80px"></i>
          </div>
          {#each skRows(3) as r (r)}
            <div class="frow">
              <i class="phx-sk slim k"></i>
              <i class="phx-sk fsel" style={`width:${190 + r * 22}px`}></i>
            </div>
          {/each}
          <div class="sub-head"><i class="phx-sk slim" style="width:56px"></i><i class="phx-sk slim" style="width:64px"></i></div>
          {#each skRows(3) as r (r)}
            <div class="frow"><i class="phx-sk slim k"></i><i class="phx-sk grow"></i><i class="phx-sk tag"></i></div>
          {/each}
        </section>
        <section class="panel">
          <div class="panel-head">
            <i class="phx-sk" style="width:58px"></i>
            <i class="phx-sk slim" style="width:100px"></i>
          </div>
          <div class="hgrid">
            {#each skRows(9) as c (c)}
              <div class="hcell">
                <i class="phx-sk slim" style="width:44px"></i>
                <span class="hp"><i class="phx-sk"></i><span class="sep">/</span><i class="phx-sk"></i></span>
                <i class="phx-sk slot"></i>
              </div>
            {/each}
          </div>
        </section>
      </div>
    </div>

    {#if cur}
      <div class="dash phx-fade-in" class:scav={mode === 'scav'}>
        <!-- 基本信息 -->
        <section class="panel">
          <div class="panel-head">
            <span class="t">{t('char.panel.basic')}</span>
            <span class="n">{t('char.panel.basicNote')}</span>
          </div>
          <div class="frow">
            <span class="k">{t('char.f.nickname')}</span>
            <input class="field" type="text" value={cur.nickname} oninput={(e) => cur && (cur.nickname = e.currentTarget.value)} />
          </div>
          {#if mode === 'pmc'}
            <div class="frow">
              <span class="k">{t('char.f.side')}</span>
              <div class="mini">
                <PhxSegmented
                  size="md"
                  role="radiogroup"
                  ariaLabel={t('char.f.side')}
                  value={cur.side}
                  onchange={(v) => cur && (cur.side = v)}
                  options={[
                    { value: 'Bear', label: 'BEAR' },
                    { value: 'Usec', label: 'USEC' }
                  ]}
                />
              </div>
            </div>
            <div class="frow">
              <span class="k">{t('char.f.aid')}</span>
              <span class="read">{meta?.aid ?? '—'}</span>
              <span class="ro-tag">{t('char.ro')}</span>
            </div>
            <div class="frow">
              <span class="k">{t('char.f.version')}</span>
              <span class="read">{meta?.gameVersion ?? '—'}</span>
              <span class="ro-tag">{t('char.ro')}</span>
            </div>
          {:else}
            <!-- Scav 卡（26 稿）：账号 ID 与游戏版本补齐左右两栏高度 -->
            <div class="frow">
              <span class="k">{t('char.f.aid')}</span>
              <span class="read">{meta?.aid ?? '—'}</span>
              <span class="ro-tag">{t('char.ro')}</span>
            </div>
            <div class="frow">
              <span class="k">{t('char.f.version')}</span>
              <span class="read">{meta?.gameVersion ?? '—'}</span>
              <span class="ro-tag">{t('char.ro')}</span>
            </div>
          {/if}
        </section>

        <!-- 成长 -->
        <section class="panel">
          <div class="panel-head">
            <span class="t">{t('char.panel.growth')}</span>
            <span class="n">{t('char.panel.growthNote')}</span>
          </div>
          <div class="frow">
            <span class="k">{t('char.f.level')}</span>
            <span class="lvl">{cur.level}</span>
            <div class="stepper">
              <button type="button" aria-label="-1" onclick={() => onStep(-1)}>−</button>
              <button type="button" aria-label="+1" onclick={() => onStep(1)}>+</button>
            </div>
            <span class="range">1 – {maxLevel}</span>
          </div>
          {#if mode === 'pmc' && prestigeMax > 0}
            <!-- 转生等级：上限取服务端 templates/prestige.json 的档位数，读不到整行不出现 -->
            <div class="frow">
              <span class="k">{t('char.f.prestige')}</span>
              <span class="lvl">{cur.prestige}</span>
              <div class="stepper">
                <button type="button" aria-label="-1" onclick={() => onPrestigeStep(-1)}>−</button>
                <button type="button" aria-label="+1" onclick={() => onPrestigeStep(1)}>+</button>
              </div>
              <span class="range">0 – {prestigeMax}</span>
            </div>
            <div class="hint">
              {t('char.prestige.hint', { n: meta?.prestigeRecords ?? 0 })}
            </div>
            <!-- 真转生：只写存档标记，清档与转移由服务器在游戏内「重置角色」时执行 -->
            <div class="prep">
              {#if meta?.prestigePending}
                <span class="prep-badge">
                  {t('char.prestige.pendingBadge', { n: meta.prestigePending })}
                </span>
                <div class="hint strong">{t('char.prestige.pendingHint')}</div>
                <button type="button" class="prep-btn ghost" disabled={prepBusy} onclick={onCancelPrestige}>
                  {t('char.prestige.cancelBtn')}
                </button>
              {:else if cur.prestige < prestigeMax}
                <button
                  type="button"
                  class="prep-btn"
                  disabled={prepBusy}
                  onclick={() => (prepOpen = true)}
                >
                  {t('char.prestige.prepareBtn', { n: cur.prestige + 1 })}
                </button>
              {:else}
                <div class="hint strong">{t('char.prestige.atTop')}</div>
              {/if}
            </div>
          {/if}
          <div class="frow exp">
            <span class="k">{t('char.f.exp')}</span>
            <input
              class="field"
              type="text"
              inputmode="numeric"
              value={expText}
              oninput={onExpInput}
            />
            <div
              class="slider"
              role="slider"
              tabindex={0}
              aria-label={t('char.f.exp')}
              aria-valuemin={expStart(cur.level)}
              aria-valuemax={expEnd(cur.level)}
              aria-valuenow={cur.experience}
              onpointerdown={onSlide}
              onkeydown={onSlideKey}
            >
              <div class="track"></div>
              <div class="fill" style={`width:${slideRatio * 100}%`}></div>
              <div class="thumb" style={`left:${slideRatio * 100}%`}></div>
            </div>
          </div>
          {#if expFoot}
            <!-- 经验脚注（26 稿）：本级经验区间 + 距下一级差额；满级整行换文案 -->
            <div class="exp-foot">
              {#if expFoot.maxed}
                <span>{t('char.exp.maxed')}</span>
              {:else}
                <span>{t('char.exp.range', { range: `${expFoot.start.toLocaleString('en-US')} – ${expFoot.end.toLocaleString('en-US')}` })}</span>
                <span>{t('char.exp.left', { n: expFoot.left.toLocaleString('en-US') })}</span>
              {/if}
            </div>
          {/if}
          <div class="hint">{t('char.exp.hint')}</div>
        </section>

        <!-- 外观与装备 -->
        <section class="panel">
          <div class="panel-head">
            <span class="t">{t('char.panel.gear')}</span>
            <span class="n">{t('char.panel.gearNote')}</span>
          </div>
          <div class="frow">
            <span class="k">{t('char.f.voice')}</span>
            <span class="fsel">
              <PhxDropdown value={cur.voice} options={voiceOpts} onchange={(v) => (cur.voice = v)} />
            </span>
          </div>
          <div class="frow">
            <span class="k">{t('char.f.head')}</span>
            <span class="fsel">
              <PhxDropdown value={cur.head} options={headOpts} onchange={(v) => (cur.head = v)} />
            </span>
          </div>
          <div class="frow">
            <span class="k">{t('char.f.pocket')}</span>
            <span class="fsel">
              <PhxDropdown
                value={cur.pocketsTpl}
                options={pocketOpts}
                onchange={(v) => (cur.pocketsTpl = v)}
              />
            </span>
          </div>
          {#if eqRows.length > 0}
            <!-- 装备速览（26 稿）：随 PMC/Scav 页签实读装备槽，只读展示 -->
            <div class="sub-head">
              <span class="st">{t('char.eq.title')}</span>
              <button type="button" class="go" onclick={() => goto(resolve('/stash'))}>
                {t('char.eq.gostash')} ›
              </button>
            </div>
            {#each eqRows as eq (eq.slot)}
              <div class="frow eq">
                <span class="k">{t(`stash.slot.${eq.slot}` as MessageKey)}</span>
                <span class="read">{eq.name || t('char.eq.empty')}</span>
                <span class="ro-tag">{t('char.ro')}</span>
              </div>
            {/each}
          {/if}
        </section>

        <!-- 健康状态 -->
        <section class="panel">
          <div class="panel-head">
            <span class="t">{t('char.panel.health')}</span>
            <span class="n">{t('char.panel.healthNote')}</span>
          </div>
          <div class="hgrid">
            {#each HEALTH_CELLS as cell (cell.key)}
              {@const v = cur.health[cell.key]}
              <div class="hcell">
                <span class="hn">{t(`char.health.${cell.key}` as MessageKey)}</span>
                <span class="hp">
                  <input
                    type="number"
                    step="any"
                    min="0"
                    value={v.current}
                    oninput={(e) => setVital(cell.key, 'current', e.currentTarget.value)}
                  />
                  <span class="sep">/</span>
                  <input
                    type="number"
                    step="any"
                    min="0"
                    value={v.maximum}
                    oninput={(e) => setVital(cell.key, 'maximum', e.currentTarget.value)}
                  />
                </span>
                <div class="bar">
                  <div class="bfill" style={`width:${vitalPct(v.current, v.maximum)}%`}></div>
                </div>
              </div>
            {/each}
          </div>
        </section>
      </div>
    {:else if !booting}
      <div class="empty">{t('char.empty.none')}</div>
    {/if}
      {/if}
</div>
</div>

<PhxDialog
  bind:open={prepOpen}
  heading={t('char.prestige.dlgHead', { n: (cur?.prestige ?? 0) + 1 })}
  text={t('char.prestige.dlgText')}
  confirmText={t('char.prestige.dlgOk')}
  cancelText={t('dlg.cancel')}
  danger
  onconfirm={onPreparePrestige}
/>

<style>
  /* 03 稿图原值：页头 + 5fr/7fr 仪表台四面板 */
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .main-head {
    padding: 24px 24px 0;
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
    margin-top: 6px;
    font-size: 13px;
    color: var(--phx-text-2);
  }
  .head-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 2px;
  }
  .btn {
    height: 32px;
    padding: 0 15px;
    border: none;
    border-radius: var(--phx-r-btn);
    font-size: 13px;
    font-weight: 600;
    font-family: inherit;
    cursor: pointer;
  }
  .btn.primary {
    background: var(--phx-accent);
    color: #fff;
  }
  .btn.primary:not(:disabled):hover {
    filter: brightness(1.12);
  }
  .btn:disabled {
    opacity: 0.45;
    cursor: default;
  }
  /* 分段切换（模式 / 阵营共用） */
  /* BEAR/USEC 分段靠右（稿图排版；页面内包裹层，分段本体为 PhxSegmented 组件）；
     --seg-pad-x 稿图原值 22px 经 CSS 变量继承穿透进组件（组件默认 18px） */
  .mini {
    margin-left: auto;
    --seg-pad-x: 22px;
  }
  .scroll {
    position: relative; /* .phx-boot 骨架层的定位基准 */
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 24px 104px;
  }
  /* 骨架层内部：与 .scroll 同 padding 镜像真实几何，占位条走全局 .phx-sk 原语 */
  .boot {
    padding: 0 24px 104px;
  }
  .boot .fld {
    height: 32px;
    border-radius: 8px;
  }
  .boot .seg {
    width: 148px;
    height: 29px;
    border-radius: 8px;
    margin-left: auto;
  }
  .boot .tag {
    width: 34px;
    height: 18px;
    border-radius: 5px;
    margin-left: auto;
  }
  .boot .lvln {
    width: 24px;
    margin-left: auto;
  }
  .boot .step {
    width: 94px;
    height: 29px;
    border-radius: 8px;
    margin-left: 10px;
  }
  .boot .rng {
    width: 52px;
    margin-left: 10px;
  }
  .boot .expf {
    width: 110px;
    height: 32px;
    border-radius: 8px;
  }
  .boot .trk {
    height: 4px;
    border-radius: 2px;
    margin-left: 10px;
  }
  .boot .hint-sk {
    margin-top: 12px;
  }
  .boot .fsel {
    height: 32px;
    border-radius: 8px;
    margin-left: auto;
  }
  .boot .hcell .hp .phx-sk {
    flex: 1 1 0;
    min-width: 0;
    height: 16px;
    border-radius: 4px;
  }
  .boot .hcell .phx-sk.slot {
    height: 3px;
    border-radius: 2px;
  }
  .empty {
    margin-top: 18px;
    padding: 60px 0;
    text-align: center;
    font-size: 13px;
    color: var(--phx-text-3);
  }
  .dash {
    margin-top: 18px;
    display: grid;
    grid-template-columns: 5fr 7fr;
    gap: 14px;
  }
  .panel {
    background: var(--phx-surface-2);
    border-radius: var(--phx-r-card);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
    padding: 2px 14px 12px;
    min-width: 0;
  }
  .panel-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    padding: 10px 0 8px;
  }
  .panel-head .t {
    font-size: 13px;
    font-weight: 600;
    color: var(--phx-text-2);
  }
  .panel-head .n {
    font-size: 12px;
    color: var(--phx-text-3);
  }
  .frow {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
  }
  .frow + .frow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .k {
    width: 60px;
    flex: none;
    font-size: 13px;
    color: var(--phx-text-2);
  }
  .field {
    flex: 1;
    min-width: 0;
    height: 32px;
    padding: 0 10px;
    border: none;
    outline: none;
    border-radius: 8px;
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
    font-family: var(--phx-font-mono);
    font-size: 14px;
    transition: box-shadow 0.15s;
  }
  .field:focus {
    box-shadow: 0 0 0 3px var(--phx-accent-dim);
  }
  .read {
    flex: 1;
    min-width: 0;
    font-family: var(--phx-font-mono);
    font-size: 13px;
    color: var(--phx-text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ro-tag {
    flex: none;
    margin-left: auto;
    padding: 2px 6px;
    border: 0.5px solid var(--phx-sep-strong);
    border-radius: 5px;
    font-size: 11px;
    color: var(--phx-text-3);
  }
  /* 等级：数字 + 步进器 + 范围 */
  .lvl {
    margin-left: auto;
    font-family: var(--phx-font-mono);
    font-size: 14px;
  }
  .stepper {
    display: flex;
    width: 94px;
    height: 29px;
    margin-left: 10px;
    background: var(--phx-fill-tertiary);
    border-radius: 8px;
    overflow: hidden;
  }
  .stepper button {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--phx-text);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
  }
  .stepper button + button {
    border-left: 0.5px solid var(--phx-sep-strong);
  }
  .stepper button:active {
    background: var(--phx-fill-quaternary);
  }
  .range {
    flex: none;
    margin-left: 10px;
    font-family: var(--phx-font-mono);
    font-size: 11px;
    color: var(--phx-text-3);
  }
  /* 经验：定宽输入 + 滑条 */
  .exp .field {
    flex: none;
    width: 110px;
    text-align: center;
  }
  .slider {
    position: relative;
    flex: 1;
    height: 20px;
    display: flex;
    align-items: center;
    cursor: pointer;
    touch-action: none;
    outline: none;
  }
  .slider:focus-visible .track {
    box-shadow: 0 0 0 2px var(--phx-accent-dim);
  }
  .slider .track {
    width: 100%;
    height: 4px;
    border-radius: 2px;
    background: var(--phx-fill-quaternary);
  }
  .slider .fill {
    position: absolute;
    left: 0;
    top: 8px;
    height: 4px;
    border-radius: 2px;
    background: var(--phx-accent);
  }
  /* 稿图原值 #fff + rgba(0,0,0,.4) 阴影：无对应 token，同上按稿图直写 */
  .slider .thumb {
    position: absolute;
    top: 50%;
    width: 18px;
    height: 18px;
    margin: -9px 0 0 -9px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
    pointer-events: none;
  }
  .hint {
    padding: 8px 0 4px;
    font-size: 11px;
    color: var(--phx-text-3);
  }

  /* 「准备转生」：真转生的入口（只写存档标记，清档由服务器执行） */
  .prep {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: flex-start;
    padding: 10px 0 2px;
  }

  .hint.strong {
    padding: 0;
    color: var(--phx-warn);
  }

  .prep-badge {
    padding: 3px 9px;
    border-radius: 999px;
    font-size: 11px;
    color: var(--phx-warn);
    background: color-mix(in srgb, var(--phx-warn) 14%, transparent);
  }

  .prep-btn {
    padding: 7px 14px;
    border: 1px solid var(--phx-line);
    border-radius: var(--phx-radius-btn);
    font-size: 12px;
    color: var(--phx-text);
    background: var(--phx-surface-2);
    cursor: pointer;
  }

  .prep-btn:hover:not(:disabled) {
    border-color: var(--phx-accent);
  }

  .prep-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .prep-btn.ghost {
    color: var(--phx-text-3);
    background: transparent;
  }
  /* 经验脚注（26 稿）：本级区间 + 距下一级差额，实心 surface-2 底 */
  .exp-foot {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    margin-top: 10px;
    padding: 7px 10px;
    background: var(--phx-surface-2);
    border-radius: 8px;
    font-size: 12px;
    color: var(--phx-text-2);
  }
  .boot .exp-foot {
    margin-top: 10px;
    min-height: 28px;
  }
  /* 装备速览小节（26 稿）：分组标题 + 前往仓库 + 三行只读 */
  .sub-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
    padding: 11px 0 0;
    border-top: 0.5px solid var(--phx-sep);
    margin-top: 4px;
  }
  .sub-head .st {
    font-size: 11px;
    font-weight: 600;
    color: var(--phx-text-3);
  }
  .sub-head .go {
    border: none;
    background: transparent;
    padding: 0;
    font-size: 12px;
    font-family: inherit;
    color: var(--phx-accent);
    cursor: pointer;
  }
  .sub-head .go:hover {
    filter: brightness(1.15);
  }
  .frow.eq .read {
    font-family: inherit;
    font-size: 13px;
  }
  /* 外观与装备：下拉容器对齐稿图 fselect（右对齐 240 上限） */
  .fsel {
    flex: 1;
    max-width: 240px;
    margin-left: auto;
    display: flex;
  }
  .fsel :global(.dd) {
    flex: 1;
    min-width: 0;
  }
  .fsel :global(.dd-btn) {
    width: 100%;
    height: 32px;
    font-size: 14px;
  }
  /* 健康 3×3：部位 + 当前/最大 + 血条 */
  .hgrid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    padding-top: 4px;
  }
  .hcell {
    background: var(--phx-surface-2);
    border-radius: 9px;
    padding: 8px 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .hcell .hn {
    font-size: 12px;
    color: var(--phx-text-2);
  }
  .hcell .hp {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .hcell input {
    flex: 1 1 0;
    width: 0;
    min-width: 0;
    padding: 2px 0;
    border: none;
    outline: none;
    background: transparent;
    border-bottom: 1px solid transparent;
    color: var(--phx-text);
    font-family: var(--phx-font-mono);
    font-size: 14px;
    text-align: center;
    transition: border-color 0.15s;
    appearance: textfield;
  }
  .hcell input:focus {
    border-bottom-color: var(--phx-accent);
  }
  .hcell input::-webkit-outer-spin-button,
  .hcell input::-webkit-inner-spin-button {
    appearance: none;
    margin: 0;
  }
  .hcell .sep {
    font-size: 12px;
    color: var(--phx-text-3);
  }
  .hcell .bar {
    height: 3px;
    border-radius: 2px;
    background: var(--phx-fill-quaternary);
    overflow: hidden;
  }
  .hcell .bar .bfill {
    display: block;
    height: 100%;
    background: var(--phx-accent);
  }

  /* ===== 形象与服装（稿图 31） ===== */
  .cloth-wrap { max-width: 1080px; }
  .cloth-look {
    display: flex; align-items: center; gap: 16px; flex: none;
    background: var(--phx-surface); border-radius: var(--phx-r-card); padding: 18px;
  }
  .cloth-avatar {
    width: 96px; height: 96px; border-radius: 18px; flex: none;
    display: flex; align-items: center; justify-content: center;
    font-family: fantasy; font-style: italic; font-weight: 700; font-size: 44px;
    background: var(--phx-accent-dim); color: var(--phx-accent);
  }
  .cloth-look-meta { min-width: 0; flex: 1; }
  .cloth-look-name { font-size: 16px; font-weight: 700; }
  .cloth-look-sub { font-size: 12px; color: var(--phx-text-2); margin-top: 4px; line-height: 1.6; }
  .cloth-look-dd { display: flex; flex-direction: column; gap: 10px; flex: 1; }
  .cloth-look-dd .cloth-dd-l { font-size: 12px; font-weight: 600; color: var(--phx-text-2); }
  .cloth-sec-h {
    margin: 22px 0 10px; font-size: 15px; font-weight: 700;
    display: flex; align-items: center; justify-content: space-between;
  }
  .cloth-all { display: flex; align-items: center; gap: 10px; font-size: 12.5px; font-weight: 400; color: var(--phx-text-2); }
  .cloth-empty { margin-top: 24px; font-size: 13px; color: var(--phx-text-3); }
  .cloth-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; }
  .cloth-card {
    background: var(--phx-surface); border-radius: var(--phx-r-card); overflow: hidden;
    position: relative; box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
  }
  .cloth-card.owned .cloth-thumb .art-body { fill: rgba(0, 145, 255, 0.18); stroke: rgba(0, 145, 255, 0.62); }
  .cloth-card.owned .cloth-thumb .art-line { stroke: rgba(0, 145, 255, 0.45); }
  .cloth-card:not(.owned) .cloth-thumb .art-body { fill: rgba(255, 255, 255, 0.04); stroke: rgba(255, 255, 255, 0.18); }
  .cloth-card:not(.owned) .cloth-thumb .art-line { stroke: rgba(255, 255, 255, 0.15); }
  .cloth-thumb { height: 96px; display: flex; align-items: center; justify-content: center; position: relative; background: var(--phx-surface-2); }
  .cloth-thumb svg { width: 64px; height: 64px; display: block; }
  .cloth-thumb .art-body { stroke-width: 2; }
  .cloth-thumb .art-line { stroke-width: 1.6; fill: none; }
  .cloth-st {
    position: absolute; top: 8px; left: 8px; font-size: 10.5px; font-weight: 700;
    padding: 2px 8px; border-radius: 5px; background: rgba(0, 0, 0, 0.45); color: var(--phx-text-3);
  }
  .cloth-st.on { background: var(--phx-green-dim); color: var(--phx-green); }
  .cloth-card-body { padding: 10px 12px 12px; min-height: 46px; display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  .cloth-n { font-size: 12.5px; font-weight: 600; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .cloth-btn {
    height: 26px; padding: 0 10px; border: none; border-radius: 7px; flex: none; cursor: pointer;
    font-size: 11.5px; font-weight: 600; font-family: inherit;
    background: var(--phx-accent-dim); color: var(--phx-accent);
  }
</style>