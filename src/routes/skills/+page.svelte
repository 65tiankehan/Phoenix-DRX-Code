<script lang="ts">
  // 06 技能 / 武器熟练页：按 docs/prototypes/06 稿图逐项落地。
  // 数据口径：PMC / Scav 双侧 Common（普通技能，上限 5100）+ Mastering（武器熟练，
  // 上限 = globals Level2+Level3，随武器各行不同）；行内等级 Lv n = 经验 ÷ 100 纯展示。
  // bot 开头的技能（BotReload / BotSound）为机器人内部技能：「全部拉满」跳过；
  // 800ms 防抖全量自动保存（稿图无保存按钮），Rust 侧先备份、Progress 夹取 0..cap。
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxProfile, phxSkills, type PhxSkillRow, type PhxSkillsSaveRow } from '$lib/ipc'
  import PhxIcon from '$lib/components/PhxIcon.svelte'
  import PhxSkeleton from '$lib/components/PhxSkeleton.svelte'
  import { playSound } from '$lib/sound'

  const ICON = {
    chev: '<path d="M6 9.5 12 15.5 18 9.5"/>',
    search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-3.8-3.8"/>'
  }

  /** 顶层分段：PMC / Scav 两套独立数据（存档 characters.pmc / .scav 同构） */
  const SEGS = ['pmc', 'scav'] as const
  type Side = (typeof SEGS)[number]

  /** 分组定义（组序固定：普通技能 → 武器熟练；capNote 稿图组头注） */
  const GROUPS: Array<{ key: 'Common' | 'Mastering'; title: MessageKey; capNote: MessageKey }> = [
    { key: 'Common', title: 'skills.g.common', capNote: 'skills.cap.common' },
    { key: 'Mastering', title: 'skills.g.mastering', capNote: 'skills.cap.mastering' }
  ]

  let currentId = $state('')
  let pmc = $state<{ common: PhxSkillRow[]; mastering: PhxSkillRow[] }>({
    common: [],
    mastering: []
  })
  let scav = $state<{ common: PhxSkillRow[]; mastering: PhxSkillRow[] }>({
    common: [],
    mastering: []
  })
  let loaded = $state(false)
  /** 首次加载中：骨架屏缓冲（感官层基线 §二）；数据到位或确认无档案后关闭 */
  let booting = $state(true)
  let saving = $state(false)
  let search = $state('')
  let seg = $state<Side>('pmc')
  let folded = $state<Record<string, boolean>>({})
  /** 一键设置全部：自绘浮层（fixed 定位贴近按钮下方） */
  let panelOpen = $state(false)
  let panelX = $state(0)
  let panelY = $state(0)
  let panelVal = $state(1000)
  /** 自动保存防抖（非渲染态） */
  let saveTimer: ReturnType<typeof setTimeout> | undefined
  let dirty = false

  const sideOf = (): { common: PhxSkillRow[]; mastering: PhxSkillRow[] } =>
    seg === 'pmc' ? pmc : scav

  const metaText = $derived(loaded ? t('skills.meta.loaded') : t('skills.meta.none'))

  /** 过滤后的分组（搜索按名称 / Id 实时过滤，叠加当前阵营分段；空组仍显示空态卡） */
  const visGroups = $derived.by(() => {
    const q = search.trim().toLowerCase()
    const side = sideOf()
    return GROUPS.map((g) => {
      const rows = side[g.key === 'Common' ? 'common' : 'mastering']
      return {
        key: g.key,
        title: t(g.title),
        capNote: t(g.capNote),
        total: rows.length,
        rows: q ? rows.filter((r) => `${r.name} ${r.id}`.toLowerCase().includes(q)) : rows
      }
    })
  })

  /** 经验值口径：步长 100，夹取 0..cap */
  function clampVal(v: number, cap: number): number {
    return Math.max(0, Math.min(cap, Math.round(v / 100) * 100))
  }

  /** 滑杆填充百分比（cap 防零） */
  const pctOf = (r: PhxSkillRow): number => (r.progress / Math.max(1, r.cap)) * 100

  function toggleFold(k: string): void {
    playSound('ui')
    folded[k] = !folded[k]
  }

  function stepRow(row: PhxSkillRow, delta: number): void {
    playSound('ui')
    row.progress = clampVal(row.progress + delta, row.cap)
    scheduleSave()
  }

  function sliderRow(row: PhxSkillRow, e: Event): void {
    row.progress = clampVal(Number((e.currentTarget as HTMLInputElement).value), row.cap)
    scheduleSave()
  }

  /** 全部拉满：当前阵营页签所有行置为各自上限，跳过 bot 开头的技能（稿图注 4） */
  function maxAll(): void {
    if (!loaded) return
    playSound('success')
    const side = sideOf()
    for (const r of side.common) if (!r.bot) r.progress = r.cap
    for (const r of side.mastering) if (!r.bot) r.progress = r.cap
    scheduleSave()
  }

  /** 一键设置全部浮层：贴按钮下方（fixed，宽 300 + 边距 16） */
  function openPanel(e: MouseEvent): void {
    e.stopPropagation()
    playSound('pop')
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    panelX = Math.min(r.left, window.innerWidth - 316)
    panelY = r.bottom + 8
    panelVal = 1000
    panelOpen = true
  }

  function panelStep(delta: number): void {
    playSound('ui')
    panelVal = clampVal(panelVal + delta, 5100)
  }

  /** 应用统一值到当前页签全部行（含 bot 行；超出各自 cap 的按 cap 夹取） */
  function applyPanel(): void {
    playSound('success')
    const side = sideOf()
    for (const r of side.common) r.progress = clampVal(panelVal, r.cap)
    for (const r of side.mastering) r.progress = clampVal(panelVal, r.cap)
    panelOpen = false
    scheduleSave()
  }

  /** 800ms 防抖全量自动保存；切档（refresh）时丢弃未落盘的防抖 */
  function scheduleSave(): void {
    dirty = true
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(() => void doSave(), 800)
  }

  /** 保存载荷：双侧全量行（side + group + id + progress），Rust 侧定点替换 */
  function allRows(): PhxSkillsSaveRow[] {
    const out: PhxSkillsSaveRow[] = []
    for (const [side, d] of [
      ['pmc', pmc],
      ['scav', scav]
    ] as const) {
      for (const r of d.common) out.push({ side, group: 'Common', id: r.id, progress: r.progress })
      for (const r of d.mastering)
        out.push({ side, group: 'Mastering', id: r.id, progress: r.progress })
    }
    return out
  }

  async function doSave(): Promise<void> {
    saveTimer = undefined
    if (!dirty || !currentId) return
    const id = currentId
    dirty = false
    saving = true
    try {
      await phxSkills.save(id, { rows: allRows() })
      toast.success(t('skills.toast.saved'))
      playSound('success')
    } catch {
      dirty = true
      toast.error(t('skills.toast.failed'))
    } finally {
      saving = false
    }
  }

  /** 全量刷新：当前档案 → 技能页数据 */
  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    currentId = id ?? ''
    loaded = false
    booting = true
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = undefined
    dirty = false
    panelOpen = false
    search = ''
    seg = 'pmc'
    folded = {}
    if (!currentId) {
      pmc = { common: [], mastering: [] }
      scav = { common: [], mastering: [] }
      booting = false
      return
    }
    const data = await phxSkills.load(currentId)
    if (!data) {
      pmc = { common: [], mastering: [] }
      scav = { common: [], mastering: [] }
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
    // 托盘「切换存档」后刷新技能页（Rust 侧 emit；本页卸载时退订并落盘未保存变更）
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
      <div class="large-title">{t('skills.title')}</div>
      <div class="head-meta">{metaText}</div>
    </div>
    <div class="head-actions">
      <button class="btn primary" type="button" onclick={maxAll} disabled={!loaded || saving}>
        {t('skills.act.max')}
      </button>
    </div>
  </header>

  {#if booting}
    <div class="boot">
      <PhxSkeleton lines={6} />
    </div>
  {:else if loaded}
    <div class="toolbar">
      <div class="search-wrap">
        <span class="search-ico"><PhxIcon markup={ICON.search} /></span>
        <input
          class="search"
          type="text"
          placeholder={t('skills.search.ph')}
          aria-label={t('skills.search.ph')}
          bind:value={search}
        />
      </div>
      <div class="seg">
        {#each SEGS as s (s)}
          <button type="button" class:on={seg === s} onclick={() => { playSound('ui'); seg = s }}>{s.toUpperCase()}</button>
        {/each}
      </div>
      <button class="btn ghost" type="button" style="margin-left:auto" onclick={openPanel}>
        {t('skills.act.all')}
      </button>
    </div>

    <div class="scroll phx-hide-scroll">
      {#each visGroups as g (g.key)}
        <section class="qgroup" class:folded={folded[g.key]}>
          <button class="ghead" type="button" onclick={() => toggleFold(g.key)}>
            <span class="gtitle">{g.title}</span>
            <span class="gcount">{g.total}</span>
            <span class="gcap">{g.capNote}</span>
            <span class="gchev"><PhxIcon markup={ICON.chev} /></span>
          </button>
          <div class="qcard">
            {#if g.rows.length}
              {#each g.rows as row (row.id)}
                <div class="srow">
                  <div class="sinfo">
                    <div class="sname">{row.name}</div>
                    <div class="smeta">
                      {#if row.bot}
                        {t('skills.meta.bot')}
                      {:else}
                        {row.id} · <span class="lv">Lv {Math.floor(row.progress / 100)}</span>
                      {/if}
                    </div>
                  </div>
                  <div class="stepper">
                    <button type="button" aria-label="-100" onclick={() => stepRow(row, -100)}>−</button>
                    <span class="val">{row.progress}</span>
                    <button type="button" aria-label="+100" onclick={() => stepRow(row, 100)}>+</button>
                  </div>
                  <input
                    type="range"
                    class="srange"
                    min="0"
                    max={row.cap}
                    step="100"
                    value={row.progress}
                    aria-label={row.name}
                    style={`background: linear-gradient(to right, var(--phx-accent) ${pctOf(row)}%, var(--phx-fill-quaternary) ${pctOf(row)}%)`}
                    oninput={(e) => sliderRow(row, e)}
                  />
                </div>
              {/each}
            {:else}
              <div class="gempty">
                <span class="gempty-ico"><PhxIcon markup={ICON.chev} /></span>
                <div class="t">{t('skills.empty.group')}</div>
              </div>
            {/if}
          </div>
        </section>
      {/each}
      {#if !visGroups.some((g) => g.rows.length)}
        <div class="empty">{search.trim() ? t('skills.empty.none') : t('skills.empty.group')}</div>
      {/if}
    </div>
  {:else}
    <div class="empty">{t('skills.empty.noprofile')}</div>
  {/if}
</div>

{#if panelOpen}
  <div class="menu-mask" role="presentation" onclick={() => (panelOpen = false)}></div>
  <div class="panel show" style={`left:${panelX}px; top:${panelY}px`} role="dialog" aria-label={t('skills.act.all')}>
    <h4>{t('skills.panel.title')}</h4>
    <p class="p-desc">{t('skills.panel.desc')}</p>
    <div class="p-valrow">
      <div class="stepper">
        <button type="button" aria-label="-100" onclick={() => panelStep(-100)}>−</button>
        <span class="val">{panelVal}</span>
        <button type="button" aria-label="+100" onclick={() => panelStep(100)}>+</button>
      </div>
      <input
        type="range"
        class="srange"
        min="0"
        max="5100"
        step="100"
        bind:value={panelVal}
        aria-label={t('skills.panel.value')}
        style={`background: linear-gradient(to right, var(--phx-accent) ${(panelVal / 5100) * 100}%, var(--phx-fill-quaternary) ${(panelVal / 5100) * 100}%)`}
      />
    </div>
    <div class="p-actions">
      <button class="btn ghost" type="button" onclick={() => (panelOpen = false)}>
        {t('skills.panel.cancel')}
      </button>
      <button class="btn primary" type="button" onclick={applyPanel}>{t('skills.panel.apply')}</button>
    </div>
  </div>
{/if}

<style>
  /* ---- 公共（与 quests 页同源） ---- */
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
  .btn.ghost {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
  }
  .btn.ghost:not(:disabled):hover {
    background: var(--phx-fill-quaternary);
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

  /* ---- 工具行：搜索 + 阵营分段 + 一键设置全部 ---- */
  .toolbar {
    margin: 12px 24px 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .search-wrap {
    position: relative;
    flex: none;
  }
  .search-ico {
    position: absolute;
    left: 9px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    color: var(--phx-text-3);
    pointer-events: none;
  }
  .search-ico :global(svg) {
    width: 14px;
    height: 14px;
  }
  .search {
    width: 240px;
    height: 32px;
    padding: 0 10px 0 30px;
    border: none;
    outline: none;
    border-radius: 9px;
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
    font-family: inherit;
    font-size: 13px;
  }
  .search::placeholder {
    color: var(--phx-text-3);
  }
  .search:focus {
    box-shadow: 0 0 0 3px var(--phx-accent-dim);
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

  /* ---- 分组（iOS grouped list：组头在卡外，行为卡内列表） ---- */
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
  .gcap {
    font-size: 11px;
    color: var(--phx-text-3);
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

  /* ---- 技能行：名称 + 经验值步进（±100）+ 滑杆，双向联动 ---- */
  .srow {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px 14px;
    min-height: 44px;
  }
  .srow + .srow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .sinfo {
    flex: 1;
    min-width: 0;
  }
  .sname {
    font-size: 14px;
    font-weight: 600;
    color: var(--phx-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .smeta {
    font-size: 11px;
    color: var(--phx-text-3);
    margin-top: 1px;
  }
  .smeta .lv {
    color: var(--phx-accent);
    font-weight: 600;
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
  .stepper .val {
    min-width: 46px;
    text-align: center;
    font-family: var(--phx-font-mono);
    font-size: 12px;
    color: var(--phx-text);
  }
  .srange {
    flex: none;
    width: 240px;
    height: 4px;
    appearance: none;
    -webkit-appearance: none;
    border-radius: 2px;
    outline: none;
    cursor: pointer;
  }
  .srange::-webkit-slider-thumb {
    appearance: none;
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #fff;
    box-shadow:
      0 1px 4px rgba(0, 0, 0, 0.5),
      0 0 0 0.5px rgba(0, 0, 0, 0.1);
  }

  /* ---- 组内空态卡（稿图 06 空态：该档案此分组为空） ---- */
  .gempty {
    padding: 26px 16px;
    text-align: center;
    color: var(--phx-text-3);
  }
  .gempty-ico {
    display: flex;
    justify-content: center;
    opacity: 0.6;
  }
  .gempty-ico :global(svg) {
    width: 30px;
    height: 30px;
    transform: rotate(90deg);
  }
  .gempty .t {
    font-size: 13px;
    color: var(--phx-text-2);
    margin-top: 6px;
  }

  /* ---- 一键设置全部（自绘浮层面板） ---- */
  .menu-mask {
    position: fixed;
    inset: 0;
    z-index: 99;
  }
  .panel {
    position: fixed;
    z-index: 100;
    width: 300px;
    padding: 16px;
    background: var(--phx-surface-2);
    border-radius: 14px;
    box-shadow: var(--phx-shadow-win);
  }
  .panel h4 {
    font-size: 13px;
    font-weight: 600;
  }
  .p-desc {
    font-size: 11px;
    color: var(--phx-text-3);
    margin: 3px 0 12px;
  }
  .p-valrow {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 14px;
  }
  .p-valrow .srange {
    flex: 1;
    width: auto;
  }
  .p-actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
</style>
