<script lang="ts">
  // 07 健康页：按 docs/prototypes/07 稿图逐项落地。
  // 数据口径：PMC / Scav 双侧九项指标（七部位 + 水分 / 能量）；联动规则对齐参考
  // CharacterMetric.Current setter——当前 > 上限时上限自动抬高到当前值（上限可单独
  // 下调、不反向联动当前值，当前最低 0）；比例条为稿图提案增补，纯展示不参与保存；
  // 800ms 防抖双侧全量自动保存（稿图无保存按钮），Rust 侧先备份、联动写入侧兜底。
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxProfile, phxHealth, type PhxHealthSide, type PhxHealthVital } from '$lib/ipc'
  import PhxIcon from '$lib/components/PhxIcon.svelte'
  import PhxSkeleton from '$lib/components/PhxSkeleton.svelte'
  import { playSound } from '$lib/sound'

  const ICON = { chev: '<path d="M6 9.5 12 15.5 18 9.5"/>' }

  /** 顶层分段：PMC / Scav 两套独立数据（存档 characters.pmc / .scav 同构） */
  const SEGS = ['pmc', 'scav'] as const
  type Side = (typeof SEGS)[number]

  /** 分组定义：身体部位（4 列网格）与水分 / 能量（2 列网格），卡内字段键对照存档 */
  const GROUPS: Array<{
    key: string
    title: MessageKey
    cols: number
    rows: Array<{ k: keyof PhxHealthSide; en: string; name: MessageKey }>
  }> = [
    {
      key: 'body',
      title: 'health.g.body',
      cols: 4,
      rows: [
        { k: 'head', en: 'Head', name: 'char.health.head' },
        { k: 'chest', en: 'Chest', name: 'char.health.chest' },
        { k: 'stomach', en: 'Stomach', name: 'char.health.stomach' },
        { k: 'leftArm', en: 'LeftArm', name: 'char.health.leftArm' },
        { k: 'rightArm', en: 'RightArm', name: 'char.health.rightArm' },
        { k: 'leftLeg', en: 'LeftLeg', name: 'char.health.leftLeg' },
        { k: 'rightLeg', en: 'RightLeg', name: 'char.health.rightLeg' }
      ]
    },
    {
      key: 'vitals',
      title: 'health.g.vitals',
      cols: 2,
      rows: [
        { k: 'hydration', en: 'Hydration', name: 'char.health.hydration' },
        { k: 'energy', en: 'Energy', name: 'char.health.energy' }
      ]
    }
  ]

  let currentId = $state('')
  let pmc = $state<PhxHealthSide>(blank())
  let scav = $state<PhxHealthSide>(blank())
  let loaded = $state(false)
  /** 首次加载中：骨架屏缓冲（感官层基线 §二）；数据到位或确认无档案后关闭 */
  let booting = $state(true)
  let saving = $state(false)
  let seg = $state<Side>('pmc')
  let folded = $state<Record<string, boolean>>({})
  /** 自动保存防抖（非渲染态） */
  let saveTimer: ReturnType<typeof setTimeout> | undefined
  let dirty = false

  function blank(): PhxHealthSide {
    return {
      head: { current: 0, maximum: 0 },
      chest: { current: 0, maximum: 0 },
      stomach: { current: 0, maximum: 0 },
      leftArm: { current: 0, maximum: 0 },
      rightArm: { current: 0, maximum: 0 },
      leftLeg: { current: 0, maximum: 0 },
      rightLeg: { current: 0, maximum: 0 },
      hydration: { current: 0, maximum: 0 },
      energy: { current: 0, maximum: 0 }
    }
  }

  const sideOf = (): PhxHealthSide => (seg === 'pmc' ? pmc : scav)

  const metaText = $derived(loaded ? t('health.meta.loaded') : t('health.meta.none'))

  /** 比例条填充（上限防零；纯展示、不参与保存——稿图注 3） */
  const pctOf = (v: PhxHealthVital): number =>
    v.maximum > 0 ? Math.min(100, Math.round((v.current / v.maximum) * 100)) : 0

  function toggleFold(k: string): void {
    playSound('ui')
    folded[k] = !folded[k]
  }

  /** 指标写入：当前 > 上限时上限自动抬高到当前值（参考 CharacterMetric.Current setter） */
  function setMetric(v: PhxHealthVital, which: 'current' | 'maximum', raw: number): void {
    const val = Math.max(0, Math.floor(raw) || 0)
    if (which === 'current') {
      v.current = val
      if (val > v.maximum) v.maximum = val
    } else {
      v.maximum = val
    }
    scheduleSave()
  }

  function stepMetric(v: PhxHealthVital, which: 'current' | 'maximum', delta: number): void {
    playSound('ui')
    setMetric(v, which, (which === 'current' ? v.current : v.maximum) + delta)
  }

  /** 数字框键入（可直接键入的编辑口径——稿图注 2：生产端为数字输入框） */
  function inputMetric(v: PhxHealthVital, which: 'current' | 'maximum', e: Event): void {
    setMetric(v, which, Number((e.currentTarget as HTMLInputElement).value))
  }

  /** 全部回满（稿图注 4 提案增补）：当前页签全部指标当前值置为各自上限 */
  function restoreAll(): void {
    if (!loaded) return
    playSound('success')
    const side = sideOf()
    for (const g of GROUPS) {
      for (const row of g.rows) side[row.k].current = side[row.k].maximum
    }
    scheduleSave()
  }

  /** 800ms 防抖全量自动保存；切档（refresh）时丢弃未落盘的防抖 */
  function scheduleSave(): void {
    dirty = true
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(() => void doSave(), 800)
  }

  async function doSave(): Promise<void> {
    saveTimer = undefined
    if (!dirty || !currentId) return
    const id = currentId
    dirty = false
    saving = true
    try {
      await phxHealth.save(id, { pmc, scav })
      toast.success(t('health.toast.saved'))
      playSound('success')
    } catch {
      dirty = true
      toast.error(t('health.toast.failed'))
    } finally {
      saving = false
    }
  }

  /** 全量刷新：当前档案 → 健康页数据 */
  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    currentId = id ?? ''
    loaded = false
    booting = true
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = undefined
    dirty = false
    seg = 'pmc'
    folded = {}
    if (!currentId) {
      pmc = blank()
      scav = blank()
      booting = false
      return
    }
    const data = await phxHealth.load(currentId)
    if (!data) {
      pmc = blank()
      scav = blank()
      booting = false
      return
    }
    pmc = data.pmc
    scav = data.scav
    loaded = true
    booting = false
  }

  onMount(() => {
    void refresh()
    // 托盘「切换存档」后刷新健康页（Rust 侧 emit；本页卸载时退订并落盘未保存变更）
    const off = listen('tray:profile-switched', () => void refresh())
    return () => {
      void off.then((unlisten) => unlisten())
      if (saveTimer) clearTimeout(saveTimer)
      if (dirty && currentId) void doSave()
    }
  })
</script>

<div class="page">
  <header class="main-head">
    <div>
      <div class="large-title">{t('health.title')}</div>
      <div class="head-meta">{metaText}</div>
    </div>
    <div class="head-actions">
      <button class="btn primary" type="button" onclick={restoreAll} disabled={!loaded || saving}>
        {t('health.act.restore')}
      </button>
    </div>
  </header>

  {#if booting}
    <div class="boot">
      <PhxSkeleton lines={6} />
    </div>
  {:else if loaded}
    <div class="toolbar">
      <div class="seg">
        {#each SEGS as s (s)}
          <button type="button" class:on={seg === s} onclick={() => { playSound('ui'); seg = s }}>{s.toUpperCase()}</button>
        {/each}
      </div>
    </div>

    <div class="scroll phx-hide-scroll">
      {#each GROUPS as g (g.key)}
        <section class="qgroup" class:folded={folded[g.key]}>
          <button class="ghead" type="button" onclick={() => toggleFold(g.key)}>
            <span class="gtitle">{t(g.title)}</span>
            <span class="gcount">{g.rows.length}</span>
            <span class="gchev"><PhxIcon markup={ICON.chev} /></span>
          </button>
          <div class="qcard">
            <div class="mgrid" class:g2={g.cols === 2} style={`--cols:${g.cols}`}>
              {#each g.rows as row (row.k)}
                {@const v = sideOf()[row.k]}
                <div class="mcard">
                  <div class="mhead">
                    <span class="mname">{t(row.name)}</span>
                    <span class="mkey">{row.en}</span>
                  </div>
                  <div class="mbar"><i style={`width:${pctOf(v)}%`}></i></div>
                  <div class="mrow">
                    <span class="mlabel">{t('health.row.cur')}</span>
                    <div class="stepper">
                      <button type="button" aria-label={t('health.aria.dec', { name: t(row.name) })} onclick={() => stepMetric(v, 'current', -1)}>−</button>
                      <input
                        class="val"
                        type="number"
                        min="0"
                        value={v.current}
                        aria-label={t('health.aria.cur', { name: t(row.name) })}
                        oninput={(e) => inputMetric(v, 'current', e)}
                      />
                      <button type="button" aria-label={t('health.aria.inc', { name: t(row.name) })} onclick={() => stepMetric(v, 'current', 1)}>+</button>
                    </div>
                  </div>
                  <div class="mrow" class:dim={v.current >= v.maximum}>
                    <span class="mlabel">{t('health.row.max')}</span>
                    <div class="stepper">
                      <button type="button" aria-label={t('health.aria.decMax', { name: t(row.name) })} onclick={() => stepMetric(v, 'maximum', -1)}>−</button>
                      <input
                        class="val"
                        type="number"
                        min="0"
                        value={v.maximum}
                        aria-label={t('health.aria.max', { name: t(row.name) })}
                        oninput={(e) => inputMetric(v, 'maximum', e)}
                      />
                      <button type="button" aria-label={t('health.aria.incMax', { name: t(row.name) })} onclick={() => stepMetric(v, 'maximum', 1)}>+</button>
                    </div>
                  </div>
                </div>
              {/each}
            </div>
          </div>
        </section>
      {/each}
    </div>
  {:else}
    <div class="empty">{t('health.empty.noprofile')}</div>
  {/if}
</div>

<style>
  /* ---- 公共（与 quests / skills 页同源） ---- */
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
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 24px 8px;
  }
  .empty {
    margin-top: 18px;
    padding: 60px 0;
    text-align: center;
    font-size: 13px;
    color: var(--phx-text-3);
  }

  /* ---- 加载骨架屏缓冲 ---- */
  .boot {
    margin: 16px 24px 0;
  }

  /* ---- 工具行：阵营分段 ---- */
  .toolbar {
    margin: 12px 24px 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    border-radius: 9px;
    background: var(--phx-fill-tertiary);
  }
  .seg button {
    border: none;
    background: transparent;
    color: var(--phx-text-2);
    font-family: inherit;
    font-size: 12px;
    font-weight: 500;
    padding: 5px 14px;
    border-radius: 7px;
    cursor: pointer;
    transition:
      background 0.15s,
      color 0.15s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  /* 稿图原值 rgba(255,255,255,.16)：无对应 token，沿用 character 页 .seg.on 硬编码先例 */
  .seg button.on {
    background: rgba(255, 255, 255, 0.16);
    color: var(--phx-text);
    font-weight: 600;
  }

  /* ---- 分组（iOS grouped list：组头在卡外，卡内指标网格） ---- */
  .qgroup {
    margin-top: 16px;
  }
  .ghead {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 0 2px 8px;
    border: none;
    background: transparent;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    user-select: none;
  }
  .gtitle {
    font-size: 13px;
    font-weight: 600;
    color: var(--phx-text-2);
    letter-spacing: 0.02em;
  }
  .gcount {
    font-family: var(--phx-font-mono);
    font-size: 11px;
    color: var(--phx-text-3);
    background: var(--phx-fill-tertiary);
    border-radius: 8px;
    padding: 1px 7px;
  }
  .gchev {
    width: 12px;
    height: 12px;
    color: var(--phx-text-3);
    margin-left: auto;
    display: flex;
    transition: transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .gchev :global(svg) {
    width: 12px;
    height: 12px;
  }
  .qgroup.folded .gchev {
    transform: rotate(-90deg);
  }
  .qcard {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow: hidden;
  }
  .qgroup.folded .qcard {
    display: none;
  }

  /* ---- 指标卡：部位名 + 当前/上限比例条（提案）+ 当前/上限双步进 ---- */
  .mgrid {
    display: grid;
    grid-template-columns: repeat(var(--cols, 4), 1fr);
    gap: 10px;
    padding: 12px;
  }
  .mgrid.g2 {
    grid-template-columns: repeat(2, 1fr);
  }
  .mcard {
    background: var(--phx-surface-2);
    border-radius: 9px;
    padding: 12px 14px;
  }
  .mhead {
    display: flex;
    align-items: baseline;
    gap: 7px;
    min-width: 0;
  }
  .mname {
    font-size: 14px;
    font-weight: 600;
    color: var(--phx-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mkey {
    font-size: 11px;
    color: var(--phx-text-3);
    font-family: var(--phx-font-mono);
    white-space: nowrap;
  }
  .mbar {
    height: 4px;
    border-radius: 2px;
    background: var(--phx-fill-quaternary);
    margin: 11px 0 13px;
    overflow: hidden;
  }
  .mbar i {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: var(--phx-accent);
    transition: width var(--phx-dur-norm) var(--phx-ease-out);
  }
  .mrow {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .mrow + .mrow {
    margin-top: 7px;
  }
  .mlabel {
    font-size: 11px;
    color: var(--phx-text-2);
    width: 26px;
    flex: none;
  }
  .mrow.dim .mlabel,
  .mrow.dim .stepper .val {
    color: var(--phx-text-3);
  }
  .stepper {
    display: flex;
    align-items: center;
    height: 28px;
    flex: none;
    border-radius: 8px;
    background: var(--phx-fill-tertiary);
    overflow: hidden;
  }
  .stepper button {
    width: 28px;
    height: 100%;
    border: none;
    background: transparent;
    color: var(--phx-accent);
    font-size: 16px;
    font-weight: 600;
    cursor: pointer;
    padding: 0;
    transition:
      background 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .stepper button:hover {
    background: var(--phx-fill-quaternary);
  }
  /* 参考编辑器口径：可直接键入的数字框（稿图 ±1 步进器为示意，此处输入框嵌步进中段） */
  .stepper .val {
    width: 46px;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    text-align: center;
    font-family: var(--phx-font-mono);
    font-size: 12px;
    color: var(--phx-text);
    -moz-appearance: textfield;
    appearance: textfield;
  }
  .stepper .val::-webkit-outer-spin-button,
  .stepper .val::-webkit-inner-spin-button {
    -webkit-appearance: none;
    margin: 0;
  }
</style>
