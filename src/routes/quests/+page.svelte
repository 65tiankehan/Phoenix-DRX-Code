<script lang="ts">
  // 05 任务页：四组任务列表（常规/每周/每日/Scav 每日），按 docs/prototypes/05 稿图逐项落地。
  // 数据口径：Rust 侧已完成状态合法性修正（就近修正）、完成联动配方增量与写前备份；
  // 前端只管本地行编辑 + 800ms 防抖全量自动保存（稿图无保存按钮）。
  // 状态 10 枚举值归并 6 色（稿图 .st-* 口径）；重复任务组（周常/日常/Scav）菜单无「锁定」。
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxProfile, phxQuests, type PhxQuestRow } from '$lib/ipc'
  import PhxDialog from '$lib/components/PhxDialog.svelte'
  import PhxIcon from '$lib/components/PhxIcon.svelte'
  import PhxSkeleton from '$lib/components/PhxSkeleton.svelte'
  import { playSound } from '$lib/sound'

  /** 稿图图标（24 系同源路径：列表勾行 / 搜索镜 / 下箭头 / 垃圾桶） */
  const ICON = {
    chev: '<path d="M6 9.5 12 15.5 18 9.5"/>',
    search:
      '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-3.8-3.8"/>',
    trash: '<path d="M4 7h16M9 7V5h6v2m-8 0 1 13h8l1-13"/>'
  }

  /** 已知商人（与 04 商人页同一份实证 ID：key 为 i18n 名，av 为稿图头像字） */
  const KNOWN: Array<{ id: string; key: MessageKey; en: string; av: string }> = [
    { id: '54cb50c76803fa8b248b4571', key: 'traders.name.prapor', en: 'Prapor', av: 'П' },
    { id: '54cb57776803fa99248b456e', key: 'traders.name.therapist', en: 'Therapist', av: 'Т' },
    { id: '58330581ace78e27b8b10cee', key: 'traders.name.skier', en: 'Skier', av: 'S' },
    { id: '5935c25fb3acc3127c3d8cd9', key: 'traders.name.peacekeeper', en: 'Peacekeeper', av: 'P' },
    { id: '5a7c2eca46aef81a7ca2145d', key: 'traders.name.mechanic', en: 'Mechanic', av: 'M' },
    { id: '5ac3b934156ae10c4430e83c', key: 'traders.name.ragman', en: 'Ragman', av: 'R' },
    { id: '5c0647fdd443bc2504c2d371', key: 'traders.name.jaeger', en: 'Jaeger', av: 'J' },
    { id: '579dc571d53a0658a154fbec', key: 'traders.name.fence', en: 'Fence', av: 'F' }
  ]

  /** QuestStatus 10 枚举值（菜单序 = 稿图文案序）；重复组无 Locked */
  const STATUSES: Array<{ en: string; key: MessageKey }> = [
    { en: 'Locked', key: 'quests.st.Locked' },
    { en: 'AvailableForStart', key: 'quests.st.AvailableForStart' },
    { en: 'Started', key: 'quests.st.Started' },
    { en: 'AvailableForFinish', key: 'quests.st.AvailableForFinish' },
    { en: 'Success', key: 'quests.st.Success' },
    { en: 'Fail', key: 'quests.st.Fail' },
    { en: 'FailRestartable', key: 'quests.st.FailRestartable' },
    { en: 'MarkedAsFailed', key: 'quests.st.MarkedAsFailed' },
    { en: 'Expired', key: 'quests.st.Expired' },
    { en: 'AvailableAfter', key: 'quests.st.AvailableAfter' }
  ]

  /** 10 态归并 6 色（稿图 .st-*）：成功/进行/可交付/开放/锁定/失败 */
  const STATUS_PILL: Record<string, string> = {
    Success: 'st-success',
    Started: 'st-progress',
    AvailableForFinish: 'st-ready',
    AvailableForStart: 'st-open',
    AvailableAfter: 'st-open',
    Locked: 'st-locked',
    Expired: 'st-locked',
    Fail: 'st-fail',
    FailRestartable: 'st-fail',
    MarkedAsFailed: 'st-fail'
  }

  /** 分组固定序（Rust 判型只输出这四种，未知归常规） */
  const GROUPS: Array<{ key: string; title: MessageKey }> = [
    { key: 'Standart', title: 'quests.g.standart' },
    { key: 'Weekly', title: 'quests.g.weekly' },
    { key: 'Daily', title: 'quests.g.daily' },
    { key: 'Daily_Savage', title: 'quests.g.savage' }
  ]

  const SEGS: Array<{ k: 'all' | 'undone' | 'done'; key: MessageKey }> = [
    { k: 'all', key: 'quests.seg.all' },
    { k: 'undone', key: 'quests.seg.undone' },
    { k: 'done', key: 'quests.seg.done' }
  ]

  let currentId = $state('')
  let rows = $state<PhxQuestRow[]>([])
  let missing = $state<PhxQuestRow[]>([])
  let loaded = $state(false)
  /** 首次加载中：骨架屏缓冲（感官层基线 §二）；数据到位或确认无档案后关闭 */
  let booting = $state(true)
  let saving = $state(false)
  let search = $state('')
  let seg = $state<'all' | 'undone' | 'done'>('all')
  let folded = $state<Record<string, boolean>>({})
  let menuRow = $state<PhxQuestRow | null>(null)
  let menuX = $state(0)
  let menuY = $state(0)
  let dlgOpen = $state(false)
  /** 待删除行（弹层文本与删除动作共用） */
  let dlgRow = $state<PhxQuestRow | null>(null)
  /** 自动保存防抖（非渲染态） */
  let saveTimer: ReturnType<typeof setTimeout> | undefined
  let dirty = false

  const metaText = $derived(loaded ? t('quests.meta.loaded') : t('quests.meta.none'))

  /** 进度统计（全量口径，不受筛选影响） */
  const total = $derived(rows.length)
  const doneCount = $derived(rows.filter((r) => r.status === 'Success').length)
  const pct = $derived(total ? Math.round((doneCount / total) * 100) : 0)

  /** 过滤 + 分组（组内 0 行不显示组） */
  const visGroups = $derived.by(() => {
    const q = search.trim().toLowerCase()
    return GROUPS.map((g) => ({
      key: g.key,
      title: t(g.title),
      rows: rows.filter((r) => {
        if ((r.type || 'Standart') !== g.key) return false
        if (seg === 'undone' && r.status === 'Success') return false
        if (seg === 'done' && r.status !== 'Success') return false
        if (q && !`${r.questName} ${traderDisplay(r.trader)}`.toLowerCase().includes(q)) return false
        return true
      })
    })).filter((g) => g.rows.length > 0)
  })

  const knownOf = (tid: string) => KNOWN.find((k) => k.id === tid)

  /** 商人副行「中文名 英文名」；未知或查无一律显示「未知商人」 */
  function traderDisplay(tid: string): string {
    const k = knownOf(tid)
    return k ? `${t(k.key)} ${k.en}` : t('traders.name.unknown')
  }

  /** 头像字：已知商人用稿图西里尔/拉丁原值，未知用「？」 */
  function avOf(row: PhxQuestRow): string {
    return knownOf(row.trader)?.av ?? '？'
  }

  /** 状态文案：10 枚举走本地化，意外值原样显示 */
  function statusLabel(en: string): string {
    const s = STATUSES.find((x) => x.en === en)
    return s ? t(s.key) : en
  }

  const pillOf = (s: string): string => STATUS_PILL[s] ?? 'st-open'

  /** 菜单可用状态：常规任务 10 态全量，重复任务组无「锁定」 */
  function menuStatuses(row: PhxQuestRow) {
    return row.type === 'Standart' ? STATUSES : STATUSES.filter((s) => s.en !== 'Locked')
  }

  function toggleFold(k: string): void {
    playSound('ui')
    folded[k] = !folded[k]
  }

  /** 弹出状态菜单：贴行右对齐（fixed，宽 160 + 边距 8） */
  function openMenu(e: MouseEvent, row: PhxQuestRow): void {
    e.stopPropagation()
    playSound('pop')
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
    menuX = Math.max(8, Math.min(r.right - 160, window.innerWidth - 168))
    menuY = r.bottom + 6
    menuRow = row
  }

  function setStatus(row: PhxQuestRow, s: string): void {
    row.status = s
    menuRow = null
    scheduleSave()
  }

  function askRemove(row: PhxQuestRow): void {
    playSound('alert')
    dlgRow = row
    dlgOpen = true
  }

  function confirmRemove(): void {
    if (!dlgRow) return
    rows = rows.filter((r) => r.qid !== dlgRow?.qid)
    scheduleSave()
  }

  /** 补齐缺失：把 templates 有而档案没有的任务按锁定态补入（Rust 侧已带判型与名字） */
  function fillMissing(): void {
    if (!missing.length) return
    playSound('gain')
    rows = [...rows, ...missing]
    missing = []
    scheduleSave()
  }

  /** 标记全部完成：含补入行，全部置为已完成 */
  function markAllDone(): void {
    if (!loaded || !rows.length) return
    playSound('success')
    for (const r of rows) r.status = 'Success'
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
      await phxQuests.save(id, { rows: rows.map((r) => ({ qid: r.qid, status: r.status })) })
      toast.success(t('quests.toast.saved'))
      playSound('success')
    } catch {
      dirty = true
      toast.error(t('quests.toast.failed'))
    } finally {
      saving = false
    }
  }

  /** 全量刷新：当前档案 → 任务页数据 */
  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    currentId = id ?? ''
    loaded = false
    booting = true
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = undefined
    dirty = false
    menuRow = null
    search = ''
    seg = 'all'
    if (!currentId) {
      rows = []
      missing = []
      booting = false
      return
    }
    const data = await phxQuests.load(currentId)
    if (!data) {
      rows = []
      missing = []
      booting = false
      return
    }
    rows = data.rows
    missing = data.missing
    loaded = true
    booting = false
  }

  onMount(() => {
    void refresh()
    // 托盘「切换存档」后刷新任务页（Rust 侧 emit；本页卸载时退订并落盘未保存变更）
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
      <div class="large-title">{t('nav.quests')}</div>
      <div class="head-meta">{metaText}</div>
    </div>
    <div class="head-actions">
      <button class="btn primary" type="button" onclick={markAllDone} disabled={!loaded || !rows.length || saving}>
        {t('quests.act.markAll')}
      </button>
    </div>
  </header>

  {#if booting}
    <div class="boot">
      <PhxSkeleton lines={6} />
    </div>
  {:else if loaded}
    <div class="statbar">
      <div class="track"><div class="fill" style={`width:${pct}%`}></div></div>
      <span class="stat">{t('quests.stat.done', { done: doneCount, total, pct })}</span>
    </div>

    <div class="toolbar">
      <div class="search-wrap">
        <span class="search-ico"><PhxIcon markup={ICON.search} /></span>
        <input
          class="search"
          type="text"
          placeholder={t('quests.search.ph')}
          aria-label={t('quests.search.ph')}
          bind:value={search}
        />
      </div>
      <div class="seg">
        {#each SEGS as s (s.k)}
          <button type="button" class:on={seg === s.k} onclick={() => { playSound('ui'); seg = s.k }}>{t(s.key)}</button>
        {/each}
      </div>
      <button class="btn ghost" type="button" style="margin-left:auto" onclick={fillMissing} disabled={!missing.length || saving}>
        {t('quests.act.fill')}
      </button>
    </div>

    <div class="scroll phx-hide-scroll">
      {#each visGroups as g (g.key)}
        <section class="qgroup" class:folded={folded[g.key]}>
          <button class="ghead" type="button" onclick={() => toggleFold(g.key)}>
            <span class="gtitle">{g.title}</span>
            <span class="gcount">{g.rows.length}</span>
            <span class="gchev"><PhxIcon markup={ICON.chev} /></span>
          </button>
          <div class="qcard">
            {#each g.rows as row (row.qid)}
              <div class="qrow">
                <span class="tav">{avOf(row)}</span>
                <div class="qinfo">
                  <div class="qname">{row.questName || row.qid.slice(0, 8)}</div>
                  <div class="qmeta">
                    {traderDisplay(row.trader)}{#if row.unlockHint}<span class="unlock"> · {t('quests.hint.unlocked', { name: row.unlockHint })}</span>{/if}
                  </div>
                </div>
                <button class="pill {pillOf(row.status)}" type="button" onclick={(e) => openMenu(e, row)}>
                  {statusLabel(row.status)}
                </button>
                <button class="qdel" type="button" aria-label={t('quests.aria.del')} onclick={() => askRemove(row)}>
                  <PhxIcon markup={ICON.trash} />
                </button>
              </div>
            {/each}
          </div>
        </section>
      {/each}
      {#if !visGroups.length}<div class="empty">{t('quests.empty.none')}</div>{/if}
    </div>
  {:else}
    <div class="empty">{t('quests.empty.noprofile')}</div>
  {/if}
</div>

{#if menuRow}
  <div class="menu-mask" role="presentation" onclick={() => (menuRow = null)}></div>
  <div class="qmenu show" style={`left:${menuX}px; top:${menuY}px`} role="menu">
    {#each menuStatuses(menuRow) as s (s.en)}
      <button
        type="button"
        role="menuitem"
        class:cur={menuRow.status === s.en}
        onclick={() => {
          if (menuRow) setStatus(menuRow, s.en)
        }}
      >
        {t(s.key)}
      </button>
    {/each}
  </div>
{/if}

<PhxDialog
  bind:open={dlgOpen}
  heading={t('quests.dlg.rm.title')}
  text={t('quests.dlg.rm.text', { name: dlgRow?.questName ?? '' })}
  danger
  confirmText={t('quests.dlg.rm.ok')}
  onconfirm={confirmRemove}
/>

<style>
  /* ---- 公共（与 traders 页同源） ---- */
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

  /* ---- 概览条（稿图 05 专属） ---- */
  .statbar {
    margin: 16px 24px 0;
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .track {
    flex: 1;
    height: 6px;
    border-radius: 3px;
    background: var(--phx-fill-tertiary);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    border-radius: 3px;
    background: var(--phx-green);
    transition: width var(--phx-dur-norm) var(--phx-ease-out);
  }
  .stat {
    flex: none;
    font-family: var(--phx-font-mono);
    font-size: 12px;
    color: var(--phx-text-2);
  }

  /* ---- 工具行：搜索 + 状态分段 + 补齐缺失 ---- */
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

  /* ---- 任务分组（iOS grouped list：组头在卡外，行为卡内列表） ---- */
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
  .qrow {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    min-height: 54px;
  }
  .qrow + .qrow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .tav {
    width: 34px;
    height: 34px;
    flex: none;
    border-radius: 50%;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    font-weight: 700;
  }
  .qinfo {
    flex: 1;
    min-width: 0;
  }
  .qname {
    font-size: 14px;
    font-weight: 600;
    color: var(--phx-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .qmeta {
    font-size: 11px;
    color: var(--phx-text-3);
    margin-top: 2px;
  }
  .qmeta .unlock {
    color: var(--phx-accent);
  }

  /* ---- 状态胶囊（点击弹出 10 态菜单） ---- */
  .pill {
    flex: none;
    height: 24px;
    padding: 0 10px;
    border: none;
    border-radius: 12px;
    font-family: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition:
      filter 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .pill:hover {
    filter: brightness(1.25);
  }
  .st-success {
    background: var(--phx-green-dim);
    color: var(--phx-green);
  }
  .st-progress {
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .st-ready {
    background: var(--phx-orange-dim);
    color: var(--phx-orange);
  }
  .st-open {
    background: var(--phx-fill-quaternary);
    color: var(--phx-text-2);
  }
  .st-locked {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text-3);
  }
  .st-fail {
    background: var(--phx-red-dim);
    color: var(--phx-red);
  }

  /* ---- 行删除（自绘确认弹层） ---- */
  .qdel {
    flex: none;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--phx-text-3);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  .qdel :global(svg) {
    width: 14px;
    height: 14px;
  }
  .qdel:hover {
    background: var(--phx-red-dim);
    color: var(--phx-red);
  }

  /* ---- 状态菜单（自绘下拉，10 态） ---- */
  .menu-mask {
    position: fixed;
    inset: 0;
    z-index: 99;
  }
  .qmenu {
    position: fixed;
    z-index: 100;
    width: 160px;
    padding: 4px;
    background: var(--phx-surface-2);
    border-radius: 10px;
    box-shadow: var(--phx-shadow-win);
  }
  .qmenu.show {
    display: block;
  }
  .qmenu button {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--phx-text);
    font-family: inherit;
    font-size: 13px;
    padding: 6px 10px;
    border-radius: 6px;
    cursor: pointer;
  }
  .qmenu button:hover {
    background: var(--phx-fill-tertiary);
  }
  .qmenu button.cur {
    color: var(--phx-accent);
    font-weight: 600;
  }
</style>
