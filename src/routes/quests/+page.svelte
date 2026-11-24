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
  import PhxTooltip from '$lib/components/PhxTooltip.svelte'
  import PhxSegmented from '$lib/components/PhxSegmented.svelte'
  import { playSound } from '$lib/sound'

  /** 骨架屏行序号生成 */
  const skRows = (n: number) => [...Array(n).keys()]

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
    { id: '579dc571d53a0658a154fbec', key: 'traders.name.fence', en: 'Fence', av: 'F' },
    { id: '638f541a29ffd1183d187f57', key: 'traders.name.lightkeeper', en: 'Lightkeeper', av: 'L' },
    { id: '656f0f98d80a697f855d34b1', key: 'traders.name.btrdriver', en: 'BTR Driver', av: 'B' },
    { id: '6617beeaa9cfa777ca915b7c', key: 'traders.name.ref', en: 'Ref', av: 'R' }
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
  /** 左栏商人筛选（'' = 全部） */
  let curTrader = $state('')
  /** 右栏详情选中的任务 qid（'' = 未选） */
  let selQid = $state('')
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

  /** 已应用的筛选（不含分组）：左栏计数与中栏列表共用同一口径 */
  const filtered = $derived.by(() => {
    const q = search.trim().toLowerCase()
    return rows.filter((r) => {
      if (seg === 'undone' && r.status === 'Success') return false
      if (seg === 'done' && r.status !== 'Success') return false
      if (curTrader && r.trader !== curTrader) return false
      if (q && !`${r.questName} ${traderDisplay(r.trader)}`.toLowerCase().includes(q)) return false
      return true
    })
  })

  /** 过滤 + 分组（组内 0 行不显示组） */
  const visGroups = $derived.by(() =>
    GROUPS.map((g) => ({
      key: g.key,
      title: t(g.title),
      rows: filtered.filter((r) => (r.type || 'Standart') === g.key)
    })).filter((g) => g.rows.length > 0)
  )

  /** 左栏商人分组：只列真的有任务的商人（含未知），按计数降序 */
  const traderGroups = $derived.by(() => {
    // 这里只是派生计算用的局部累加器，不参与响应式，用普通对象即可
    const counts: Record<string, number> = {}
    for (const r of filtered) counts[r.trader] = (counts[r.trader] ?? 0) + 1
    const known = KNOWN.filter((k) => counts[k.id]).map((k) => ({
      id: k.id,
      label: t(k.key),
      av: k.av,
      n: counts[k.id] ?? 0
    }))
    const unknown = counts['unknown'] ?? 0
    const out = unknown ? [...known, { id: 'unknown', label: t('traders.name.unknown'), av: '？', n: unknown }] : known
    return out.sort((a, b) => b.n - a.n)
  })

  /** 选中的任务（右栏详情） */
  const selQuest = $derived(rows.find((r) => r.qid === selQid) ?? null)

  function pickTrader(id: string): void {
    playSound('ui')
    curTrader = curTrader === id ? '' : id
  }

  function pickQuest(row: PhxQuestRow): void {
    playSound('ui')
    selQid = selQid === row.qid ? '' : row.qid
  }

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

  <div class="body">
    <!-- 骨架层：镜像概览条 + 工具行 + 任务分组真实布局；数据就绪加 .gone 淡出，与新内容交叉淡化（感官层基线 §二） -->
    <div class="boot phx-boot" class:gone={!booting}>
      <div class="statbar">
        <i class="phx-sk trk-sk"></i>
        <i class="phx-sk slim" style="width:170px"></i>
      </div>
      <!-- 骨架按三栏布局镜像：左商人筛选 / 中分组列表 / 右详情卡 -->
      <div class="toolbar">
        <span class="fspacer"></span>
        <i class="phx-sk search-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk ghost-sk"></i>
      </div>
      <div class="scroll">
        <div class="q3">
          <aside class="tside">
            {#each skRows(6) as r (r)}
              <div class="titem">
                <i class="phx-sk tav-sk"></i>
                <i class="phx-sk slim" style="width:58px"></i>
                <i class="phx-sk slim" style="width:22px;margin-left:auto"></i>
              </div>
            {/each}
          </aside>
          <section class="qgroup qmid">
            <div class="ghead">
              <i class="phx-sk slim" style="width:64px"></i>
              <i class="phx-sk gc-sk"></i>
              <i class="phx-sk chev-sk"></i>
            </div>
            <div class="qcard">
              <div class="qfold">
                {#each skRows(5) as r (r)}
                  <div class="qrow">
                    <i class="phx-sk tav-sk"></i>
                    <div class="qinfo">
                      <i class="phx-sk" style="width:158px"></i>
                      <i class="phx-sk slim" style="width:104px"></i>
                    </div>
                    <i class="phx-sk pill-sk"></i>
                    <i class="phx-sk del-sk"></i>
                  </div>
                {/each}
              </div>
            </div>
          </section>
          <aside class="qdetail">
            <div class="card mini">
              <i class="phx-sk slim" style="width:120px"></i>
              <i class="phx-sk slim" style="width:72px"></i>
              <div class="dblk">
                <i class="phx-sk slim" style="width:64px"></i>
                <i class="phx-sk" style="width:100%;height:8px"></i>
              </div>
              <div class="dblk">
                <i class="phx-sk slim" style="width:40px"></i>
                {#each skRows(3) as r (r)}
                  <div class="between">
                    <i class="phx-sk slim" style="width:52px"></i>
                    <i class="phx-sk slim" style="width:60px"></i>
                  </div>
                {/each}
              </div>
            </div>
          </aside>
        </div>
      </div>
    </div>

    {#if loaded}
    <div class="statbar phx-fade-in">
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
      <PhxSegmented
        bind:value={seg}
        ariaLabel={t('quests.seg.aria')}
        options={[
          { value: 'all', label: t('quests.seg.all') },
          { value: 'undone', label: t('quests.seg.undone') },
          { value: 'done', label: t('quests.seg.done') }
        ]}
      />
      <button class="btn ghost" type="button" onclick={fillMissing} disabled={!missing.length || saving}>
        {t('quests.act.fill')}
      </button>
    </div>

    <div class="scroll phx-fade-in">
      <div class="q3">
        <aside class="tside">
          <button class="titem" type="button" class:on={curTrader === ''} onclick={() => pickTrader('')}>
            <span class="tav sm">Σ</span>
            <span class="tlbl">{t('quests.seg.all')}</span>
            <span class="tn">{filtered.length}</span>
          </button>
          {#each traderGroups as tr (tr.id)}
            <button class="titem" type="button" class:on={curTrader === tr.id} onclick={() => pickTrader(tr.id)}>
              <span class="tav sm">{tr.av}</span>
              <span class="tlbl">{tr.label}</span>
              <span class="tn">{tr.n}</span>
            </button>
          {/each}
        </aside>

        <div class="qmid">
          {#each visGroups as g (g.key)}
            <section class="qgroup" class:folded={folded[g.key]}>
              <button class="ghead" type="button" onclick={() => toggleFold(g.key)}>
                <span class="gtitle">{g.title}</span>
                <span class="gcount">{g.rows.length}</span>
                <span class="gchev"><PhxIcon markup={ICON.chev} /></span>
              </button>
              <div class="qcard">
                <div class="qfold">
                {#each g.rows as row (row.qid)}
                  <div class="qrow" class:on={selQid === row.qid}>
                    <button class="qpick" type="button" onclick={() => pickQuest(row)}>
                      <span class="tav">{avOf(row)}</span>
                      <div class="qinfo">
                        <div class="qname">{row.questName || row.qid.slice(0, 8)}</div>
                        <div class="qmeta">
                          {traderDisplay(row.trader)}{#if row.unlockHint}<span class="unlock"> · {t('quests.hint.unlocked', { name: row.unlockHint })}</span>{/if}
                        </div>
                      </div>
                    </button>
                    <button class="pill {pillOf(row.status)}" type="button" onclick={(e) => openMenu(e, row)}>
                      {statusLabel(row.status)}
                    </button>
                    <PhxTooltip label={t('quests.aria.del')}>
                      <button class="qdel" type="button" aria-label={t('quests.aria.del')} onclick={() => askRemove(row)}>
                        <PhxIcon markup={ICON.trash} />
                      </button>
                    </PhxTooltip>
                  </div>
                {/each}
                </div>
              </div>
            </section>
          {/each}
          {#if !visGroups.length}<div class="empty">{t('quests.empty.none')}</div>{/if}
        </div>

        <aside class="qdetail">
          {#if selQuest}
            <div class="card mini">
              <div class="mini-h">
                <span class="dname">{selQuest.questName || selQuest.qid.slice(0, 8)}</span>
                <span class="pill {pillOf(selQuest.status)}">{statusLabel(selQuest.status)}</span>
              </div>
              <div class="dmeta">{traderDisplay(selQuest.trader)}</div>
              {#if selQuest.condTotal > 0}
                <div class="dblk">
                  <div class="dcond-h">
                    <span class="dh">{t('quests.detail.conds')}</span>
                    <span class="dnum">{selQuest.condDone} / {selQuest.condTotal}</span>
                  </div>
                  {#if selQuest.condFlags.length}
                    <div class="dots">
                      {#each selQuest.condFlags as ok, i (i)}
                        <span class="dot" class:ok></span>
                      {/each}
                    </div>
                  {:else}
                    <div class="dcond">
                      <div class="dbar"><i style={`width:${Math.round((selQuest.condDone / selQuest.condTotal) * 100)}%`}></i></div>
                    </div>
                  {/if}
                </div>
              {/if}
              {#if selQuest.rewardExp || selQuest.rewardStanding || selQuest.rewardMoney}
                <div class="dblk">
                  <div class="dh">{t('quests.detail.rewards')}</div>
                  {#if selQuest.rewardExp}<div class="drow"><span>{t('quests.detail.exp')}</span><span class="dnum">+{selQuest.rewardExp.toLocaleString('en-US')}</span></div>{/if}
                  {#if selQuest.rewardStanding}<div class="drow"><span>{t('quests.detail.standing')}</span><span class="dnum">+{selQuest.rewardStanding}</span></div>{/if}
                  {#if selQuest.rewardMoney}<div class="drow"><span>{t('quests.detail.money')}</span><span class="dnum">+{selQuest.rewardMoney.toLocaleString('en-US')}</span></div>{/if}
                </div>
              {/if}
              <div class="dacts">
                <button class="btn ghost sm" type="button" onclick={(e) => openMenu(e, selQuest!)}>{t('quests.detail.setStatus')}</button>
                <button class="btn danger-ghost sm" type="button" onclick={() => askRemove(selQuest!)}>{t('quests.aria.del')}</button>
              </div>
            </div>
          {:else}
            <div class="card mini empty-detail">{t('quests.detail.hint')}</div>
          {/if}
        </aside>
      </div>
    </div>
  {:else}
    <div class="empty">{t('quests.empty.noprofile')}</div>
  {/if}
  </div>
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
    padding: 0 24px 104px;
  }
  .empty {
    margin-top: 18px;
    padding: 60px 0;
    text-align: center;
    font-size: 13px;
    color: var(--phx-text-3);
  }

  /* ---- 加载骨架屏缓冲 ---- */
  /* 页级定位包装层：包住常驻骨架层与内容，骨架 absolute 覆盖时避开页头 */
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* 骨架内复用真实布局 class（自带边距），仅覆写控件形状 */
  .boot .trk-sk {
    flex: 1;
    height: 6px;
    border-radius: 3px;
  }

  .boot .search-sk {
    width: 240px;
    height: 32px;
    border-radius: 9px;
  }

  .boot .seg-sk {
    width: 64px;
    height: 30px;
    border-radius: 8px;
  }

  .boot .ghost-sk {
    width: 86px;
    height: 30px;
    border-radius: 8px;
    margin-left: auto;
  }

  .boot .gc-sk {
    width: 24px;
    height: 16px;
    border-radius: 8px;
  }

  .boot .chev-sk {
    width: 12px;
    height: 12px;
    border-radius: 3px;
    margin-left: auto;
  }

  .boot .tav-sk {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    flex: none;
  }

  .boot .qinfo {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .boot .pill-sk {
    width: 60px;
    height: 24px;
    border-radius: 12px;
    flex: none;
  }

  .boot .del-sk {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    flex: none;
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
    justify-content: flex-end;
    gap: 8px;
  }
  .search-wrap {
    position: relative;
    flex: none;
    width: 260px;
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
    /* 展开收起高度过渡：单行 grid 1fr↔0fr（感官层基线档位） */
    display: grid;
    grid-template-rows: 1fr;
    transition: grid-template-rows var(--phx-dur-norm) var(--phx-ease-emphasized);
  }
  .qgroup.folded .qcard {
    grid-template-rows: 0fr;
  }
  /* 收起动画结束后再移出可聚焦范围；展开瞬间恢复 */
  .qfold {
    min-height: 0;
    overflow: hidden;
    transition: visibility 0s var(--phx-dur-norm);
  }
  .qgroup.folded .qfold {
    visibility: hidden;
  }
  .qgroup:not(.folded) .qfold {
    visibility: visible;
    transition: visibility 0s;
  }
  .qrow {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 14px;
    min-height: 48px;
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

  /* ———————— 三栏：商人分组 / 任务列表 / 详情（稿图 06 屏） ———————— */
  .q3 {
    display: grid;
    grid-template-columns: 168px minmax(0, 1fr) 276px;
    gap: 14px;
    align-items: start;
    /* 工具行靠右后与右栏同 x 区间，拉开一点距离免得看着挤在一起 */
    margin-top: 16px;
  }
  .tside {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
    border-radius: var(--phx-r-card);
    background: var(--phx-surface);
    border: 0.5px solid var(--phx-sep);
  }
  .titem {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--phx-text);
    font-size: 12.5px;
    text-align: left;
    cursor: pointer;
  }
  .titem:hover { background: var(--phx-fill-quaternary); }
  .titem.on { background: var(--phx-accent-soft); color: var(--phx-accent); }
  .tav.sm {
    width: 22px;
    height: 22px;
    font-size: 10.5px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--phx-surface-3);
    color: var(--phx-text-2);
  }
  .titem.on .tav.sm { background: var(--phx-accent); color: #fff; }
  .tlbl { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tn { font-size: 11px; color: var(--phx-text-3); font-variant-numeric: tabular-nums; }
  .qmid { display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .qpick {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr);
    align-items: center;
    gap: 9px;
    border: 0;
    background: transparent;
    padding: 0;
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }
  .qrow.on { background: var(--phx-accent-soft); }
  .qrow .qpick { flex: 1; min-width: 0; }
  .qpick { grid-template-columns: 28px minmax(0, 1fr); }
  .qdetail { position: sticky; top: 0; }
  .card.mini {
    padding: 12px 13px;
    border-radius: var(--phx-r-card);
    background: var(--phx-surface);
    border: 0.5px solid var(--phx-sep);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .mini-h {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .dname { font-size: 13px; font-weight: 600; color: var(--phx-text); }
  .dmeta { font-size: 11.5px; color: var(--phx-text-3); }
  .dblk { border-top: 0.5px solid var(--phx-sep); padding-top: 8px; }
  .dh { font-size: 11px; color: var(--phx-text-3); margin-bottom: 5px; }
  .dcond { display: flex; align-items: center; gap: 8px; }
  .dcond-h { display: flex; align-items: baseline; justify-content: space-between; }
  .dcond-h .dh { margin-bottom: 6px; }
  .dots { display: flex; flex-wrap: wrap; gap: 4px; }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--phx-surface-3);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep);
  }
  .dot.ok { background: var(--phx-green); box-shadow: none; }
  .dbar {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--phx-surface-3);
    overflow: hidden;
  }
  .dbar i { display: block; height: 100%; background: var(--phx-green); }
  .drow {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    color: var(--phx-text-2);
    padding: 2px 0;
  }
  .dnum { font-variant-numeric: tabular-nums; color: var(--phx-text); }
  .dacts { display: flex; gap: 8px; margin-top: 4px; }
  .empty-detail { font-size: 12px; color: var(--phx-text-3); line-height: 1.6; }</style>
