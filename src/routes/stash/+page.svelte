<script lang="ts">
  // 09 仓库页：按 docs/prototypes/09 稿图逐项落地（仅 PMC；Scav 装备与口袋归 11 屏）。
  // 三子页签：仓库表（货币条 + 附加行数 + 物品表）/ 添加物品（分类树 + 列表）/ 装备槽。
  // 表格口径（非游戏方格）；modded = 模板不在服务器物品库；含 modded 时加钱区禁用并警示。
  // 保存模式：即时保存（op 列表逐条执行，Rust 侧放置引擎）——添加 / 删除成功后 refresh
  // 对账，加钱 / 附加行数本地数值已同步无需重读。武器行「检视」钮跳转检视页弹窗
  // （10 屏已落地）；「开容器」钮待容器窗落地后接入（诚实原则：不做假开关，暂不渲染）。
  import { onMount } from 'svelte'
  import { goto } from '$app/navigation'
  import { resolve } from '$app/paths'
  import { listen } from '@tauri-apps/api/event'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import {
    phxProfile,
    phxStash,
    type PhxStashItemRow,
    type PhxStashOp
  } from '$lib/ipc'
  import PhxDialog from '$lib/components/PhxDialog.svelte'
  import PhxIcon from '$lib/components/PhxIcon.svelte'
  import PhxTooltip from '$lib/components/PhxTooltip.svelte'
  import PhxItemIcon from '$lib/components/PhxItemIcon.svelte'
  import PhxSegmented from '$lib/components/PhxSegmented.svelte'
  import { playSound } from '$lib/sound'
  import { navIntent } from '$lib/nav'

  /** 骨架屏行序号生成 */
  const skRows = (n: number) => [...Array(n).keys()]

  const ICON = {
    search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-3.8-3.8"/>',
    warn: '<path d="M12 3.5 22 20H2L12 3.5Z"/><path d="M12 10v4.5"/><circle cx="12" cy="17.2" r=".4" fill="currentColor"/>',
    plus: '<path d="M5 12h14M12 5v14"/>',
    trash: '<path d="M4 7h16M9 7V5h6v2m-8 0 1 13h8l1-13"/>',
    // 取出到仓库：纸箱出口（向上离开容器）
    out: '<path d="M12 4v9m0-9-3 3m3-3 3 3"/><path d="M5 14v5h14v-5"/>'
  }

  /** 三货币（tpl 与 Rust 侧常量对应；稿图胶囊序 ₽ / € / $） */
  const MONEY: Array<{ tpl: string; sym: string; get: () => number; add: number }> = [
    { tpl: '5449016a4bdc2d6f028b456f', sym: '₽', get: () => rubles, add: 100000 },
    { tpl: '569668774bdc2da2298b4568', sym: '€', get: () => euros, add: 10000 },
    { tpl: '5696686a4bdc2da3298b456a', sym: '$', get: () => dollars, add: 10000 }
  ]

  type Seg = 'stash' | 'add' | 'equip'

  let currentId = $state('')
  let items = $state<PhxStashItemRow[]>([])
  let equipment = $state<{ slot: string; item: PhxStashItemRow | null }[]>([])
  let pockets = $state<PhxStashItemRow[]>([])
  let categories = $state<{ id: string; name: string; parentId: string | null }[]>([])
  let addable = $state<
    {
      tpl: string
      name: string
      shortName: string
      desc: string
      categoryId: string
      w: number
      h: number
    }[]
  >([])
  /** 添加物品的目标：仓库网格 / 口袋（口袋格是 1×1，只列 1×1 物品） */
  let addTarget = $state<'stash' | 'pocket'>('stash')
  /** 口袋格名：口袋模板的四个 1×1 格（pocket1..pocket4） */
  const POCKET_CELLS = ['pocket1', 'pocket2', 'pocket3', 'pocket4']
  let rubles = $state(0)
  let dollars = $state(0)
  let euros = $state(0)
  let bonusRows = $state(0)
  let hasModded = $state(false)
  /** 仓库网格尺寸（0 = 服务器没给，退回列表视图） */
  let stashCols = $state(0)
  let stashRows = $state(0)
  let loaded = $state(false)
  /** 首次加载中：骨架屏缓冲；数据到位或确认无档案后关闭 */
  let booting = $state(true)
  let saving = $state(false)
  let seg = $state<Seg>('stash')
  /** 仓库主区视图：格子（默认，稿图 09 屏）/ 列表（表格，便于批量找 id） */
  let viewMode = $state<'grid' | 'list'>('grid')
  /** 单格物品只放缩写：取名称首段（空格/括号前）最多 6 个字 */
  function shortOf(name: string): string {
    const head = name.split(/[\s(（]/)[0] ?? name
    return head.slice(0, 6)
  }

  let fName = $state('')
  let fId = $state('')
  let aName = $state('')
  let aDesc = $state(false)
  let curCat = $state('')
  /** 确认弹层（删单件 / 清空仓库 / 清空装备共用） */
  let dlgOpen = $state(false)
  let dlgKind = $state<'one' | 'stash' | 'equip'>('one')
  let dlgId = $state('')

  const rootId = $derived(categories.find((c) => c.parentId === null)?.id ?? '')
  /** 顶层分类：parent 是根；子分类：parent 是顶层（稿图两级树） */
  const topCats = $derived(categories.filter((c) => c.parentId === rootId && rootId))
  const subsOf = (topId: string) => categories.filter((c) => c.parentId === topId)

  /** 当前分类的可添加列表；选中根分类 = 全部物品（稿图「全部物品」语义） */
  const visAddable = $derived.by(() => {
    const q = aName.trim().toLowerCase()
    return addable.filter((a) => {
      // 加到口袋时只列 1×1 的物品：口袋格就是 1×1，多格物品后端会直接拒绝
      if (addTarget === 'pocket' && (a.w !== 1 || a.h !== 1)) return false
      if (curCat !== rootId && a.categoryId !== curCat) return false
      if (!q) return true
      // 名称 / 短名（M4A1）/ 模板 id 三者都命中，中文名与英文缩写都能搜
      return (
        a.name.toLowerCase().includes(q) ||
        a.shortName.toLowerCase().includes(q) ||
        a.tpl.toLowerCase().includes(q) ||
        (aDesc && a.desc.toLowerCase().includes(q))
      )
    })
  })

  const visItems = $derived.by(() => {
    const q = fName.trim().toLowerCase()
    const kid = fId.trim().toLowerCase()
    return items.filter(
      (it) =>
        (!q || it.name.toLowerCase().includes(q)) && (!kid || it.tpl.toLowerCase().includes(kid))
    )
  })
  const moddedCount = $derived(visItems.filter((i) => i.modded).length)
  /** 有格位的物品（容器内子件 x = -1，不参与格子绘制） */
  const placed = $derived(visItems.filter((i) => i.x >= 0 && i.y >= 0))
  /** 格子视图可用：档案给出了网格尺寸且至少一件物品有坐标 */
  const canGrid = $derived(stashCols > 0 && stashRows > 0)
  /** 格子视图里被点选的物品（点一下选中，再决定检视/删除，避免误触） */
  let selId = $state('')
  /** 拖动摆放：dragId 非空表示正在拖某件物品 */
  let dragId = $state('')
  let dragDx = 0
  let dragDy = 0
  let gstageEl = $state<HTMLDivElement | null>(null)
  /** 跟随光标的幽灵块：拖起来要看得见手上拿着什么 */
  let dragPos = $state<{ x: number; y: number } | null>(null)
  let dragBox = $state({ w: 0, h: 0, lx: 0, ly: 0 })
  let dragLabel = $state('')
  /** 刚落位的物品：松手成功后闪一下，让「放下了」看得见 */
  let justMoved = $state('')
  const selItem = $derived(visItems.find((i) => i.id === selId) ?? null)
  /** 占用格数（旋转时宽高互换）；空格数 = 总格 - 占用 */
  const usedCells = $derived(
    placed.reduce((n, i) => n + (i.rot ? i.h : i.w) * (i.rot ? i.w : i.h), 0)
  )
  const freeCells = $derived(Math.max(0, stashCols * stashRows - usedCells))
  /** 右侧「添加物品」快捷结果：按当前搜索词过滤，最多 3 条 */
  const quickAddable = $derived.by(() => {
    const q = aName.trim().toLowerCase()
    const hit = q
      ? addable.filter(
          (a) =>
            a.name.toLowerCase().includes(q) ||
            a.shortName.toLowerCase().includes(q) ||
            a.tpl.toLowerCase().includes(q)
        )
      : addable
    return hit.slice(0, 3)
  })

  /** 底格坐标列表（行优先）；比 Array(n) + Math.floor 干净，也免掉未使用占位变量 */
  const gridCells = $derived.by(() => {
    const out: Array<{ x: number; y: number }> = []
    for (let y = 0; y < stashRows; y++) {
      for (let x = 0; x < stashCols; x++) out.push({ x, y })
    }
    return out
  })

  function switchView(mode: 'grid' | 'list'): void {
    playSound('toggle')
    viewMode = mode
  }

  function selectItem(id: string): void {
    playSound('ui')
    selId = selId === id ? '' : id
  }

  type Placed = (typeof placed)[number]

  /** 指针位置 → 0 基格坐标（夹在网格范围内） */
  function cellSize(): number {
    if (!gstageEl) return 0
    const box = gstageEl.getBoundingClientRect()
    return (box.width - 4 * (stashCols - 1)) / stashCols
  }

  function startDrag(ev: PointerEvent, it: Placed): void {
    if (ev.button !== 0 || !canGrid) return
    const el = ev.currentTarget as HTMLElement
    const rect = el.getBoundingClientRect()
    dragDx = ev.clientX - rect.left
    dragDy = ev.clientY - rect.top
    dragBox = { w: rect.width, h: rect.height, lx: dragDx, ly: dragDy }
    dragLabel = it.w * it.h > 1 ? it.name : shortOf(it.name)
    dragId = it.id
    dragPos = { x: ev.clientX, y: ev.clientY }
    el.setPointerCapture(ev.pointerId)
  }

  /** 拖动过程：只更新幽灵坐标，不碰其它 state，避免整张格子重渲染 */
  function moveDrag(ev: PointerEvent): void {
    if (!dragId) return
    dragPos = { x: ev.clientX, y: ev.clientY }
  }

  async function endDrag(ev: PointerEvent, it: Placed): Promise<void> {
    const wasDragging = dragId === it.id
    dragId = ''
    dragPos = null
    if (!wasDragging || !gstageEl) return
    const box = gstageEl.getBoundingClientRect()
    const step = cellSize() + 4
    // 用「指针 - 抓取偏移」反推左上角所在格，抓边角时整体才不会偏一格
    const left = ev.clientX - dragDx - box.left
    const top = ev.clientY - dragDy - box.top
    const w = it.rot ? it.h : it.w
    const nx = Math.max(0, Math.min(stashCols - w, Math.round(left / step)))
    const ny = Math.max(0, Math.round(top / step))
    if (nx === it.x && ny === it.y) {
      selectItem(it.id)
      return
    }
    const movedId = it.id
    await runOps([{ op: 'moveItem', id: movedId, x: nx, y: ny, rot: it.rot }], true)
    justMoved = movedId
    setTimeout(() => {
      if (justMoved === movedId) justMoved = ''
    }, 700)
  }



  const metaText = $derived(loaded ? t('stash.meta.loaded') : t('stash.meta.none'))

  const fmtMoney = (n: number): string => n.toLocaleString('en-US')

  /** 装备槽名 i18n：stash.slot.{slotId}，意外槽位原样显示 slotId */
  const slotName = (slot: string): string => t(`stash.slot.${slot}` as MessageKey)

  /** 清除检索：名称与模板编号一起清空 */
  function clearFilters(): void {
    playSound('ui')
    fName = ''
    fId = ''
  }

  function toggleDesc(): void {
    playSound('toggle')
    aDesc = !aDesc
  }

  /** 即时保存：op 列表交 Rust 逐条执行；reload = 成功后重读对账 */
  async function runOps(ops: PhxStashOp[], reload: boolean): Promise<void> {
    if (!currentId || !ops.length) return
    const id = currentId
    saving = true
    try {
      await phxStash.save(id, { ops })
      playSound('success')
      toast.success(t('stash.toast.saved'))
      if (reload) await loadAll(id)
    } catch (e) {
      toast.error(t('stash.toast.failed'))
      // 放置失败（如仓库已满）等错误信息透出
      if (typeof e === 'string' && e) toast.error(e)
      if (reload) await loadAll(id)
    } finally {
      saving = false
    }
  }

  function addMoney(tpl: string, amount: number): void {
    if (hasModded) return
    playSound('gain')
    if (tpl === '5449016a4bdc2d6f028b456f') rubles += amount
    else if (tpl === '569668774bdc2da2298b4568') euros += amount
    else dollars += amount
    void runOps([{ op: 'addMoney', tpl, amount }], false)
  }

  function stepBonus(delta: number): void {
    const v = Math.max(0, bonusRows + delta)
    if (v === bonusRows) return
    playSound('ui')
    bonusRows = v
    void runOps([{ op: 'setBonusRows', count: v }], false)
  }

  function askRemove(id: string): void {
    playSound('alert')
    dlgKind = 'one'
    dlgId = id
    dlgOpen = true
  }

  function askClearStash(): void {
    if (!items.length) return
    playSound('alert')
    dlgKind = 'stash'
    dlgOpen = true
  }

  function askClearEquip(): void {
    if (!equipment.some((s) => s.item) && !pockets.length) return
    playSound('alert')
    dlgKind = 'equip'
    dlgOpen = true
  }

  function confirmDlg(): void {
    if (dlgKind === 'one') {
      items = items.filter((i) => i.id !== dlgId)
      void runOps([{ op: 'removeItems', ids: [dlgId] }], true)
    } else if (dlgKind === 'stash') {
      const ids = items.map((i) => i.id)
      items = []
      void runOps([{ op: 'removeItems', ids }], true)
    } else {
      const ids = [
        ...equipment.filter((s) => s.item).map((s) => s.item!.id),
        ...pockets.map((p) => p.id)
      ]
      equipment = equipment.map((s) => ({ ...s, item: null }))
      pockets = []
      void runOps([{ op: 'removeItems', ids }], true)
    }
  }

  function addItem(tpl: string): void {
    playSound('gain')
    if (addTarget === 'pocket') {
      void runOps([{ op: 'addToPocket', tpl }], true)
      return
    }
    void runOps([{ op: 'addItem', tpl }], true)
  }

  /** 口袋内换格（pocket1..pocket4）；目标格有物品时后端两者互换 */
  function moveInPocket(id: string, slot: string): void {
    void runOps([{ op: 'moveInPocket', id, slot }], true)
  }

  /** 口袋取出到仓库（走仓库放置引擎找空位） */
  function takeOutOfPocket(id: string): void {
    playSound('gain')
    void runOps([{ op: 'pocketTakeOut', id }], true)
  }

  /** 全量刷新：当前档案 → 仓库页数据（仅 PMC） */
  async function loadAll(id: string): Promise<void> {
    const data = await phxStash.load(id)
    if (!data) {
      items = []
      equipment = []
      pockets = []
      categories = []
      addable = []
      stashCols = 0
      stashRows = 0
      loaded = false
      return
    }
    items = data.items
    equipment = data.equipment
    pockets = data.pockets
    categories = data.categories
    addable = data.addable
    rubles = data.rubles
    dollars = data.dollars
    euros = data.euros
    bonusRows = data.bonusRows
    hasModded = data.hasModded
    stashCols = data.gridCols
    stashRows = data.gridRows
    // 默认选中第一个顶层分类（无子分类时退回根 = 全部物品）
    curCat = topCats[0]?.id ?? rootId
    loaded = true
  }

  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    currentId = id ?? ''
    loaded = false
    booting = true
    seg = 'stash'
    fName = ''
    fId = ''
    aName = ''
    aDesc = false
    if (!currentId) {
      items = []
      equipment = []
      pockets = []
      categories = []
      addable = []
      booting = false
      return
    }
    await loadAll(currentId)
    booting = false
  }

  onMount(() => {
    // 首页「口袋管理」直达装备栏：refresh() 内部会把 seg 重置回「仓库」，所以等它落地后再切
    const wantEquip = navIntent.stashTab === 'equip'
    if (wantEquip) navIntent.stashTab = ''
    void refresh().then(() => {
      if (wantEquip) seg = 'equip'
    })
    // 托盘「切换存档」后刷新仓库页（Rust 侧 emit）
    const off = listen('tray:profile-switched', () => void refresh())
    return () => {
      void off.then((unlisten) => unlisten())
    }
  })
</script>

<div class="page">
  <header class="main-head">
    <div>
      <div class="large-title">{t('nav.stash')}</div>
      <div class="head-meta">{metaText}</div>
    </div>
    <div class="head-actions">
      {#if seg === 'stash'}
        <button class="btn ghost" type="button" onclick={() => { playSound('ui'); seg = 'add' }} disabled={!loaded}>
          {t('stash.act.addItem')}
        </button>
        <button class="btn ghost" type="button" onclick={() => switchView(viewMode === 'grid' ? 'list' : 'grid')} disabled={!canGrid}>
          {t('stash.view.toggle')}
        </button>
        <button class="btn danger-ghost" type="button" onclick={askClearStash} disabled={!items.length || saving}>
          {t('stash.act.clearStash')}
        </button>
      {:else if seg === 'add'}
        <button class="btn ghost" type="button" onclick={() => { playSound('ui'); curCat = rootId }} disabled={!loaded}>
          {t('stash.act.allItems')}
        </button>
      {:else}
        <button class="btn danger-ghost" type="button" onclick={askClearEquip} disabled={saving || (!equipment.some((s) => s.item) && !pockets.length)}>
          {t('stash.act.clearEquip')}
        </button>
      {/if}
    </div>
  </header>

  <div class="body">
    <!-- 骨架层：镜像工具行 + 货币条/物品列表真实布局；数据就绪加 .gone 淡出，与新内容交叉淡化（感官层基线 §二） -->
    <div class="boot phx-boot" class:gone={!booting}>
      <!-- 骨架按新布局镜像：子页签 + 搜索行 / 左格子墙 + 右三卡 -->
      <div class="toolbar">
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <span class="fspacer"></span>
        <i class="phx-sk sfld-sk" style="flex:1;max-width:420px"></i>
        <i class="phx-sk sfld-sk" style="width:230px"></i>
        <i class="phx-sk abtn-sk"></i>
      </div>
      <div class="scroll">
        <div class="stashbody">
          <section class="qgroup grow">
            <div class="ghead">
              <i class="phx-sk slim" style="width:72px"></i>
              <span class="fspacer"></span>
              <i class="phx-sk abtn-sk"></i>
              <i class="phx-sk abtn-sk"></i>
            </div>
            <div class="gskel">
              {#each skRows(30) as r (r)}
                <i class="phx-sk gskel-cell"></i>
              {/each}
            </div>
          </section>
          <aside class="side">
            {#each skRows(3) as c (c)}
              <div class="card mini">
                <i class="phx-sk slim" style="width:72px"></i>
                {#each skRows(3) as r (r)}
                  <div class="between">
                    <i class="phx-sk slim" style="width:54px"></i>
                    <i class="phx-sk slim" style="width:70px"></i>
                  </div>
                {/each}
              </div>
            {/each}
          </aside>
        </div>
      </div>
    </div>
  {#if loaded}
    <div class="toolbar phx-fade-in">
      <PhxSegmented
        bind:value={seg}
        ariaLabel={t('stash.seg.aria')}
        options={[
          { value: 'stash', label: t('stash.seg.stash') },
          { value: 'add', label: t('stash.seg.add') },
          { value: 'equip', label: t('stash.seg.equip') }
        ]}
      />
      <span class="fspacer"></span>
      {#if seg === 'stash'}
        <div class="sfield growf">
          <span class="sico"><PhxIcon markup={ICON.search} /></span>
          <input type="text" placeholder={t('stash.search.inStash')} bind:value={fName} />
        </div>
        <div class="sfield idf">
          <input type="text" placeholder={t('stash.search.byTpl')} bind:value={fId} />
        </div>
        <button class="btn ghost sm" type="button" onclick={clearFilters} disabled={!fName && !fId}>
          {t('stash.filter.clear')}
        </button>
      {:else if seg === 'add'}
        <div class="sfield">
          <span class="sico"><PhxIcon markup={ICON.search} /></span>
          <input type="text" placeholder={t('stash.search.name')} bind:value={aName} />
        </div>
        <button class="ckdesc" type="button" onclick={toggleDesc} aria-pressed={aDesc}>
          <span class="ck" class:on={aDesc}></span>{t('stash.search.desc')}
        </button>
      {/if}
    </div>

    <div class="scroll phx-fade-in">
      {#if seg === 'stash'}
        {#if hasModded}
          <div class="warn">
            <span class="warn-ico"><PhxIcon markup={ICON.warn} /></span>
            <span>{t('stash.warn.modded')}</span>
          </div>
        {/if}
        <div class="stashbody">
          <section class="qgroup grow">
            <div class="ghead">
              <span class="gtitle">{viewMode === 'grid' ? t('stash.g.grid') : t('stash.g.items')}</span>
              <span class="gcount">{visItems.length}</span>
              <span class="fspacer"></span>
              {#if canGrid}
                <div class="vsw">
                  <button type="button" class:on={viewMode === 'grid'} onclick={() => switchView('grid')}>{t('stash.view.grid')}</button>
                  <button type="button" class:on={viewMode === 'list'} onclick={() => switchView('list')}>{t('stash.view.list')}</button>
                </div>
              {/if}
            </div>
            {#if viewMode === 'grid' && canGrid}
              <div class="gridwrap">
                <!-- 选中条常驻占位：只在选中时插入会把格子整体往下顶，鼠标一移就「跳一下」 -->
                <div class="gselslot">
                {#if selItem}
                  <div class="gsel">
                    <span class="gsname">{selItem.name}{selItem.count > 1 ? ` ×${selItem.count}` : ''}</span>
                    <span class="gsid">{selItem.tpl}</span>
                    <span class="fspacer"></span>
                    {#if selItem.isWeapon}
                      <button class="gsbtn" type="button" onclick={() => { playSound('pop'); goto(resolve(`/inspect?id=${selItem!.id}`)) }}>
                        {t('stash.aria.inspect')}
                      </button>
                    {/if}
                    <button class="gsbtn del" type="button" onclick={() => askRemove(selItem!.id)}>
                      {t('stash.aria.del')}
                    </button>
                  </div>
                {:else}
                  <div class="gsel idle">{t('stash.grid.hint')}</div>
                {/if}
                </div>
                <div class="gstage" bind:this={gstageEl} style="grid-template-columns:repeat({stashCols}, minmax(0, 1fr))">
                  <!-- 单层 CSS Grid：底格与物品共用同一套网格线。格子边长由 aspect-ratio 撑出，
                       物品用 grid-column / grid-row 占位，尺寸自动跟随容器，不再手算像素。 -->
                  {#each gridCells as c (`${c.x}:${c.y}`)}
                    <span class="gcell" style="grid-column:{c.x + 1};grid-row:{c.y + 1}"></span>
                  {/each}
                  {#each placed as it (it.id)}
                    <button
                      class="gitem"
                      class:on={selId === it.id}
                      class:dragging={dragId === it.id}
                      class:pulse={justMoved === it.id}
                      type="button"
                      style="grid-column:{it.x + 1} / span {it.rot ? it.h : it.w};grid-row:{it.y + 1} / span {it.rot ? it.w : it.h}"
                      onpointerdown={(ev) => startDrag(ev, it)}
                      onpointermove={moveDrag}
                      onpointerup={(ev) => endDrag(ev, it)}
                    >
                      <PhxItemIcon tpl={it.tpl} name={it.name} size={it.w * it.h > 1 ? 'lg' : 'md'} square />
                      <span class="gname" class:wide={it.w * it.h > 1}>
                        {it.w * it.h > 1 ? it.name : shortOf(it.name)}
                      </span>
                      {#if it.count > 1}<span class="gnum">×{it.count}</span>{/if}
                      {#if it.modded}<span class="gmod"><PhxIcon markup={ICON.warn} /></span>{/if}
                    </button>
                  {/each}
                </div>

                {#if dragPos}
                  <div
                    class="gdrag"
                    style="transform:translate3d({dragPos.x - dragBox.lx}px, {dragPos.y - dragBox.ly}px, 0) scale(1.03);width:{dragBox.w}px;height:{dragBox.h}px"
                  >
                    <span class="gdrag-name">{dragLabel}</span>
                  </div>
                {/if}


                <div class="ghint">{t('stash.grid.hint')}</div>
                {#if !visItems.length}<div class="empty">{t('stash.grid.empty')}</div>{/if}
              </div>
            {:else}
              <div class="qcard">
                {#each visItems as it (it.id)}
                  <div class="mrow">
                    <PhxItemIcon tpl={it.tpl} name={it.name} square />
                    <div class="iname">{it.name}{it.count > 1 ? ` ×${it.count}` : ''}</div>
                    <span class="mmod" class:show={it.modded}><PhxIcon markup={ICON.warn} /></span>
                    <span class="iid">{it.tpl}</span>
                    <div class="acts">
                      {#if it.isWeapon}
                        <!-- 检视入口（10 屏弹窗已落地）：武器行跳转检视页并自动开弹窗 -->
                        <PhxTooltip label={t('stash.aria.inspect')}>
                          <button class="abtn" type="button" aria-label={t('stash.aria.inspect')} onclick={() => { playSound('pop'); goto(resolve(`/inspect?id=${it.id}`)) }}>
                            <PhxIcon markup={ICON.search} />
                          </button>
                        </PhxTooltip>
                      {/if}
                      <PhxTooltip label={t('stash.aria.del')}>
                        <button class="abtn del" type="button" aria-label={t('stash.aria.del')} onclick={() => askRemove(it.id)}>
                          <PhxIcon markup={ICON.trash} />
                        </button>
                      </PhxTooltip>
                    </div>
                  </div>
                {/each}
                {#if !visItems.length}<div class="empty">{t('stash.empty.none')}</div>{/if}
              </div>
            {/if}
            {#if visItems.length}
              <div class="tfoot">
                {t('stash.foot', { n: visItems.length, m: moddedCount })}
              </div>
            {/if}
          </section>

          <aside class="side">
            <div class="card mini">
              <div class="mini-h">{t('stash.money.title')}<span class="muted" style="font-weight:400">{t('stash.money.note')}</span></div>
              {#each MONEY as m (m.tpl)}
                <div class="mrow">
                  <span class="msym">{m.sym}</span>
                  <span class="mval">{fmtMoney(m.get())}</span>
                  <PhxTooltip label={t('stash.aria.addMoney', { sym: m.sym })}>
                    <button class="madd" type="button" aria-label={t('stash.aria.addMoney', { sym: m.sym })} disabled={hasModded || saving} onclick={() => addMoney(m.tpl, m.add)}>
                      <PhxIcon markup={ICON.plus} />
                    </button>
                  </PhxTooltip>
                </div>
              {/each}
              <div class="macts">
                <button class="chip" type="button" disabled={hasModded || saving} onclick={() => addMoney(MONEY[0].tpl, 1000000)}>+ {t('stash.money.m1')}</button>
                <button class="chip" type="button" disabled={hasModded || saving} onclick={() => addMoney(MONEY[0].tpl, 10000000)}>+ {t('stash.money.m10')}</button>
              </div>
            </div>

            <div class="card mini">
              <div class="mini-h">{t('stash.stat.title')}</div>
              <div class="between"><span class="muted">{t('stash.stat.items')}</span><span class="num">{items.length}</span></div>
              <div class="between">
                <span class="muted">{t('stash.stat.used')}</span>
                <span class="num">{usedCells} / {freeCells}</span>
              </div>
              <div class="between" style="border-top:1px solid var(--phx-sep);padding-top:8px;margin-top:8px">
                <span class="muted">{t('stash.bonus')}</span>
                <div class="step">
                  <button type="button" aria-label="-2" onclick={() => stepBonus(-2)}>−</button>
                  <span class="num">{bonusRows}</span>
                  <button type="button" aria-label="+2" onclick={() => stepBonus(2)}>＋</button>
                </div>
              </div>
            </div>

            <div class="card mini">
              <div class="mini-h">{t('stash.quick.add')}<span class="muted" style="font-weight:400">{t('stash.quick.from')}</span></div>
              <div class="sfield">
                <span class="sico"><PhxIcon markup={ICON.search} /></span>
                <input type="text" placeholder={t('stash.quick.ph')} bind:value={aName} />
              </div>
              {#each quickAddable as a (a.tpl)}
                <div class="qrow">
                  <span class="qname">{a.name}</span>
                  <span class="qiid">{a.tpl.slice(0, 8)}…</span>
                  <button class="qadd" type="button" disabled={saving} onclick={() => addItem(a.tpl)}>{t('stash.act.add')}</button>
                </div>
              {/each}
              {#if !quickAddable.length}<div class="qnone">{t('stash.empty.none')}</div>{/if}
            </div>
          </aside>
        </div>
      {:else if seg === 'add'}
        <!-- 警示横幅：对齐参考 ItemsAdding 顶部 Warning -->
        <div class="warn">
          <span class="warn-ico"><PhxIcon markup={ICON.warn} /></span>
          <span>{t('stash.warn.presets')}</span>
        </div>
        <div class="addwrap">
          <aside class="tree">
            {#each topCats as top (top.id)}
              <button class="titem" type="button" class:on={curCat === top.id} onclick={() => { playSound('ui'); curCat = top.id }}>
                <span class="tico"></span>{top.name}
              </button>
              {#each subsOf(top.id) as sub (sub.id)}
                <button class="titem sub" type="button" class:on={curCat === sub.id} onclick={() => { playSound('ui'); curCat = sub.id }}>
                  <span class="tico"></span>{sub.name}
                </button>
              {/each}
            {/each}
            <button class="titem" type="button" class:on={curCat === rootId} onclick={() => { playSound('ui'); curCat = rootId }}>
              <span class="tico"></span>{t('stash.tree.all')}
            </button>
          </aside>
          <section class="qgroup alist">
            <div class="ghead">
              <span class="gtitle">{t('stash.g.addable')}</span>
              <span class="gcount">{visAddable.length}</span>
            </div>
            <!-- 目标：仓库网格 / 口袋（口袋四个 1×1 格，满了要先去取出） -->
            <div class="atarget">
              <span class="atlabel">{t('stash.add.target')}</span>
              <button
                class="atchip"
                type="button"
                class:on={addTarget === 'stash'}
                onclick={() => { playSound('ui'); addTarget = 'stash' }}
              >{t('stash.add.target.stash')}</button>
              <button
                class="atchip"
                type="button"
                class:on={addTarget === 'pocket'}
                onclick={() => { playSound('ui'); addTarget = 'pocket' }}
              >{t('stash.add.target.pocket')}</button>
              {#if addTarget === 'pocket'}
                <span class="atnote">{pockets.length} / 4</span>
              {/if}
            </div>
            <div class="qcard">
              {#each visAddable as a (a.tpl)}
                <div class="arow">
                  <PhxItemIcon tpl={a.tpl} name={a.name} square />
                  <div class="iname">{a.name}</div>
                  <span class="iid">{a.tpl}</span>
                  <button class="addbtn" type="button" disabled={saving} onclick={() => addItem(a.tpl)}>
                    {t('stash.act.add')}
                  </button>
                </div>
              {/each}
              {#if !visAddable.length}<div class="empty">{t('stash.empty.none')}</div>{/if}
            </div>
          </section>
        </div>
      {:else}
        <section class="qgroup">
          <div class="ghead">
            <span class="gtitle">{t('stash.g.equip')}</span>
            <span class="gcount">{equipment.length}</span>
          </div>
          <div class="qcard">
            {#each equipment as s (s.slot)}
              <div class="srow">
                <span class="slotname">{slotName(s.slot)}</span>
                {#if s.item}
                  <PhxItemIcon tpl={s.item.tpl} name={s.item.name} square />
                  <div class="iname">{s.item.name}</div>
                  <span class="mmod" class:show={s.item.modded}><PhxIcon markup={ICON.warn} /></span>
                  <span class="iid">{s.item.tpl}</span>
                  <div class="acts">
                    <PhxTooltip label={t('stash.aria.unequip')}>
                      <button class="abtn del" type="button" aria-label={t('stash.aria.unequip')} onclick={() => askRemove(s.item!.id)}>
                        <PhxIcon markup={ICON.trash} />
                      </button>
                    </PhxTooltip>
                  </div>
                {:else}
                  <span class="icon-b empty-ico">—</span>
                  <div class="iname dim">{t('stash.row.empty')}</div>
                  <span class="mmod"></span>
                  <span class="iid dim">—</span>
                  <div class="acts"></div>
                {/if}
              </div>
            {/each}
            <!-- 口袋：四个 1×1 格（pocket1..pocket4），可换格 / 取出 / 删除 -->
            <div class="pwrap">
              <div class="phead">
                {t('stash.slot.Pockets')}
                <span class="pcount">{pockets.length} / 4</span>
              </div>
              {#each POCKET_CELLS as cell, ci (cell)}
                {@const p = pockets.find((x) => (x.slotId ?? '') === cell)}
                <div class="pitem" class:vacant={!p}>
                  <span class="pno">{ci + 1}</span>
                  {#if p}
                    <PhxItemIcon tpl={p.tpl} name={p.name} size="sm" square />
                    <div class="iname">{p.name}{p.count > 1 ? ` ×${p.count}` : ''}</div>
                    <div class="pmove">
                      {#each POCKET_CELLS as target, ti (target)}
                        <button
                          class="pcell"
                          type="button"
                          class:on={target === cell}
                          aria-label={`${t('stash.slot.Pockets')} ${ti + 1}`}
                          onclick={() => moveInPocket(p.id, target)}
                        >{ti + 1}</button>
                      {/each}
                    </div>
                    <PhxTooltip label={t('stash.pocket.takeout')}>
                      <button class="abtn" type="button" aria-label={t('stash.pocket.takeout')} disabled={saving} onclick={() => takeOutOfPocket(p.id)}>
                        <PhxIcon markup={ICON.out} />
                      </button>
                    </PhxTooltip>
                    <PhxTooltip label={t('stash.aria.del')}>
                      <button class="abtn del" type="button" aria-label={t('stash.aria.del')} onclick={() => askRemove(p.id)}>
                        <PhxIcon markup={ICON.trash} />
                      </button>
                    </PhxTooltip>
                  {:else}
                    <div class="pempty">{t('stash.pocket.empty')}</div>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        </section>
      {/if}
    </div>
  {:else}
    <div class="empty">{t('stash.empty.noprofile')}</div>
  {/if}
  </div>
</div>

<PhxDialog
  bind:open={dlgOpen}
  heading={dlgKind === 'one'
    ? t('stash.dlg.rm.title')
    : dlgKind === 'stash'
      ? t('stash.dlg.stash.title')
      : t('stash.dlg.equip.title')}
  text={dlgKind === 'one'
    ? t('stash.dlg.rm.text', { name: [...items, ...pockets].find((i) => i.id === dlgId)?.name ?? '' })
    : dlgKind === 'stash'
      ? t('stash.dlg.stash.text', { n: items.length })
      : t('stash.dlg.equip.text')}
  danger
  confirmText={t('stash.dlg.ok')}
  onconfirm={confirmDlg}
/>

<style>
  /* ---- 公共（与 quests / skills / hideout 页同源） ---- */
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
  .btn.ghost {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
  }
  .btn.ghost:not(:disabled):hover {
    background: var(--phx-fill-quaternary);
  }
  .btn.danger-ghost {
    background: transparent;
    color: var(--phx-red);
    box-shadow: inset 0 0 0 0.5px var(--phx-red-dim);
  }
  .btn.danger-ghost:not(:disabled):hover {
    background: var(--phx-red-dim);
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
    padding: 44px 0;
    text-align: center;
    color: var(--phx-text-3);
    font-size: 13px;
  }

  /* ---- 页级内容包装：骨架层与真实内容共用定位上下文 ---- */
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  /* ---- 加载骨架屏缓冲（镜像真实布局，复用原 class 几何，仅覆写控件形状） ---- */
  .boot .seg-sk {
    width: 64px;
    height: 30px;
    border-radius: 8px;
  }

  .boot .sfld-sk {
    width: 180px;
    height: 28px;
    border-radius: 8px;
  }





  .boot .abtn-sk {
    width: 26px;
    height: 26px;
    border-radius: 7px;
  }


  /* ---- 工具行：子页分段 + 过滤框 + 含描述勾选 ---- */
  .toolbar {
    margin: 12px 24px 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .fspacer {
    flex: 1;
  }
  .sfield {
    height: 28px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    border-radius: 8px;
    background: var(--phx-fill-tertiary);
  }
  .sico {
    display: flex;
    color: var(--phx-text-3);
    pointer-events: none;
  }
  .sico :global(svg) {
    width: 12px;
    height: 12px;
  }
  .sfield input {
    border: none;
    outline: none;
    background: transparent;
    font-family: inherit;
    font-size: 12px;
    color: var(--phx-text);
    width: 108px;
  }
  .sfield input::placeholder {
    color: var(--phx-text-3);
  }
  .ckdesc {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--phx-text-3);
    background: transparent;
    border: none;
    cursor: pointer;
    font-family: inherit;
    padding: 0;
  }
  .ck {
    width: 16px;
    height: 16px;
    border-radius: 5px;
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
    background: var(--phx-fill-tertiary);
    position: relative;
    flex: none;
  }
  .ck.on {
    background: var(--phx-accent);
    box-shadow: none;
  }
  .ck.on::after {
    content: '';
    position: absolute;
    left: 4.5px;
    top: 1.5px;
    width: 5px;
    height: 9px;
    border: solid #fff;
    border-width: 0 1.5px 1.5px 0;
    transform: rotate(45deg);
  }

  /* ---- 货币条 + 附加行数 ---- */
  .mrow {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
  }
  .msym {
    width: 18px;
    font-size: 12px;
    font-weight: 700;
    color: var(--phx-text-2);
    flex: 0 0 auto;
  }
  .mval {
    margin-left: auto;
    font-family: var(--phx-font-mono);
    font-size: 12px;
    color: var(--phx-text);
  }
  .macts {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
  .chip {
    flex: 1;
    height: 26px;
    border: 1px solid var(--phx-sep-strong);
    border-radius: 7px;
    background: var(--phx-surface-2);
    color: var(--phx-text-2);
    font-size: 11.5px;
    cursor: pointer;
  }
  .chip:not(:disabled):hover {
    background: var(--phx-surface-3);
    color: var(--phx-text);
  }
  .chip:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .madd {
    width: 20px;
    height: 20px;
    border: none;
    border-radius: 50%;
    cursor: pointer;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    transition:
      filter 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .madd:not(:disabled):hover {
    filter: brightness(1.25);
  }
  .madd:disabled {
    cursor: default;
  }
  .madd :global(svg) {
    width: 10px;
    height: 10px;
  }
  .step {
    display: flex;
    align-items: center;
    background: var(--phx-surface-2);
    border-radius: 8px;
  }
  .step button {
    width: 26px;
    height: 26px;
    border: none;
    background: transparent;
    color: var(--phx-text-2);
    cursor: pointer;
    font-size: 13px;
    font-family: inherit;
    border-radius: 8px;
    padding: 0;
    transition:
      background 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .step button:hover {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
  }
  .step .num {
    min-width: 30px;
    text-align: center;
    font-family: var(--phx-font-mono);
    font-size: 12px;
    color: var(--phx-text);
  }

  /* ---- 分组 / 警示横幅（orange 状态 token） ---- */
  .qgroup {
    margin-top: 16px;
  }
  .ghead {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 2px 8px;
  }
  .gtitle {
    font-size: 12px;
    font-weight: 600;
    color: var(--phx-text-2);
    letter-spacing: 0.3px;
  }
  .gcount {
    font-family: var(--phx-font-mono);
    font-size: 11px;
    color: var(--phx-text-3);
    background: var(--phx-fill-tertiary);
    border-radius: 999px;
    padding: 1px 8px;
  }
  .qcard {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow: hidden;
  }
  .warn {
    margin: 16px 0 0;
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--phx-orange-dim);
    border: 0.5px solid var(--phx-sep-strong);
    color: var(--phx-orange);
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .warn-ico {
    display: flex;
    flex: none;
  }
  .warn-ico :global(svg) {
    width: 14px;
    height: 14px;
  }
  .tfoot {
    padding: 12px 14px;
    text-align: center;
    font-size: 11px;
    color: var(--phx-text-3);
  }

  /* ---- 仓库表行 ---- */
  .mrow {
    display: grid;
    grid-template-columns: 30px minmax(0, 1.35fr) 18px minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
  }
  .mrow + .mrow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .icon-b.empty-ico {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text-3);
  }
  .iname {
    font-size: 12.5px;
    color: var(--phx-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .iname.dim {
    color: var(--phx-text-3);
  }
  .iid {
    font-family: var(--phx-font-mono);
    font-size: 11px;
    color: var(--phx-text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .iid.dim {
    color: var(--phx-text-3);
    opacity: 0.5;
  }
  .mmod {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--phx-red);
    visibility: hidden;
  }
  .mmod.show {
    visibility: visible;
  }
  .mmod :global(svg) {
    width: 14px;
    height: 14px;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: flex-end;
  }
  .abtn {
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 7px;
    cursor: pointer;
    background: transparent;
    color: var(--phx-text-3);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    transition:
      background 0.12s,
      color 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .abtn :global(svg) {
    width: 13px;
    height: 13px;
  }
  .abtn.del:hover {
    background: var(--phx-red-dim);
    color: var(--phx-red);
  }

  /* ---- 添加物品：分类树 + 列表 ---- */
  .addwrap {
    margin-top: 16px;
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .tree {
    width: 236px;
    flex: none;
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    padding: 8px;
  }
  .titem {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border-radius: 8px;
    border: none;
    background: transparent;
    font-size: 12.5px;
    color: var(--phx-text-2);
    cursor: pointer;
    font-family: inherit;
    text-align: left;
    transition:
      background 0.12s,
      color 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .titem:hover {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
  }
  .titem.on {
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    font-weight: 600;
  }
  .tico {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    background: var(--phx-fill-quaternary);
    flex: none;
  }
  .titem.on .tico {
    background: var(--phx-accent-dim);
  }
  .titem.sub {
    padding-left: 28px;
    font-size: 12px;
  }
  .alist {
    flex: 1;
    min-width: 0;
  }
  .arow {
    display: grid;
    grid-template-columns: 30px minmax(0, 1fr) minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
  }
  .arow + .arow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .addbtn {
    height: 26px;
    padding: 0 12px;
    border: none;
    border-radius: 7px;
    cursor: pointer;
    background: var(--phx-accent);
    color: #fff;
    font-size: 12px;
    font-family: inherit;
    font-weight: 600;
    transition:
      filter 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .addbtn:not(:disabled):hover {
    filter: brightness(1.1);
  }
  .addbtn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  /* ---- 装备槽 ---- */
  .srow {
    display: grid;
    grid-template-columns: 92px 30px minmax(0, 1.1fr) 18px minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    padding: 9px 14px;
  }
  .srow + .srow,
  .pitem + .pitem {
    border-top: 0.5px solid var(--phx-sep);
  }
  .slotname {
    font-size: 12px;
    font-weight: 600;
    color: var(--phx-text-2);
  }
  .pwrap {
    grid-column: 1 / -1;
    padding: 0 0 6px;
    border-top: 0.5px solid var(--phx-sep);
  }
  .phead {
    font-size: 12px;
    font-weight: 600;
    color: var(--phx-text-2);
    padding: 9px 14px 2px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pcount {
    color: var(--phx-text-3);
    font-weight: 400;
  }
  .pitem {
    display: grid;
    grid-template-columns: 18px 26px minmax(0, 1fr) auto auto auto;
    align-items: center;
    gap: 10px;
    padding: 7px 14px 7px 26px;
  }
  .pitem.vacant {
    opacity: 0.75;
  }
  .pno {
    font-size: 11px;
    color: var(--phx-text-3);
    text-align: right;
  }
  .pmove {
    display: flex;
    gap: 2px;
  }
  .pcell {
    width: 18px;
    height: 18px;
    border-radius: 4px;
    border: 0.5px solid var(--phx-sep);
    background: transparent;
    color: var(--phx-text-3);
    font-size: 10px;
    line-height: 1;
    cursor: pointer;
  }
  .pcell.on {
    border-color: var(--phx-accent);
    color: var(--phx-accent);
    background: color-mix(in srgb, var(--phx-accent) 12%, transparent);
  }
  .pcell:hover {
    border-color: var(--phx-accent);
  }
  .pempty {
    font-size: 12px;
    color: var(--phx-text-3);
    padding: 0;
    grid-column: 2 / -1;
  }
  /* 添加物品目标选择（仓库 / 口袋） */
  .atarget {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 2px 8px;
  }
  .atlabel {
    font-size: 12px;
    color: var(--phx-text-3);
  }
  .atchip {
    height: 24px;
    padding: 0 10px;
    border-radius: 999px;
    border: 0.5px solid var(--phx-sep);
    background: transparent;
    color: var(--phx-text-2);
    font-size: 12px;
    cursor: pointer;
  }
  .atchip.on {
    border-color: var(--phx-accent);
    color: var(--phx-accent);
    background: color-mix(in srgb, var(--phx-accent) 12%, transparent);
  }
  .atnote {
    font-size: 12px;
    color: var(--phx-text-3);
    margin-left: 2px;
  }

  /* ———————— 格子视图（稿图 09 屏） ———————— */
  .stashbody {
    display: grid;
    /* 主列吃掉剩余宽度、右栏固定 268：格子卡片铺满整行，格子本身按容器宽度
       自适应（recalcCell）并居中 —— 原来是 auto + justify-content:start，
       窗口一宽右侧就空出一大块（用户截图里绿框那块） */
    grid-template-columns: minmax(0, 1fr) 268px;
    gap: 14px;
    align-items: stretch;
    min-height: 0;
  }
  .stashbody .grow { min-width: 0; }
  .vsw {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 7px;
    background: var(--phx-surface-3);
  }
  .vsw button {
    padding: 3px 10px;
    font-size: 11.5px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--phx-text-2);
    cursor: pointer;
  }
  .vsw button.on {
    background: var(--phx-surface);
    color: var(--phx-text);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12);
  }
  .gskel {
    display: grid;
    grid-template-columns: repeat(10, minmax(0, 1fr));
    gap: 4px;
    padding: 12px;
  }
  .gskel-cell {
    aspect-ratio: 1;
    border-radius: 5px;
    height: auto;
  }
  .gridwrap {
    padding: 12px 14px 10px;
    border-radius: var(--phx-r-card);
    background: var(--phx-surface);
    border: 0.5px solid var(--phx-sep);
    /* 40~68 行会远超一屏：格子区自己滚，高度吃到页头与 dock 安全区之间，
       不再留出下方那片空白（原先是 -400，窗口一高就短一截） */
    max-height: calc(100vh - 332px);
    min-height: min(520px, calc(100vh - 332px));
    overflow-y: auto;
  }
  /* 格子舞台：两层同一个 grid 参数叠放 —— 底格层画格子，物品层用 grid-column/row 定位。
     格子尺寸完全由容器宽度按 1fr 均分决定，窗口怎么变都对，不再手算像素。 */
  .gstage {
    display: grid;
    gap: 4px;
    max-width: 100%;
  }
  .gcell {
    aspect-ratio: 1;
    border-radius: 5px;
    background: var(--phx-surface-3);
    border: 1px solid var(--phx-sep);
  }
  /* 物品块：稿图的做法 —— 居中显示，单格给缩写、多格给全名 */
  .gitem {
    position: relative;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    padding: 3px;
    border: 1px solid var(--phx-sep-strong);
    border-radius: 5px;
    background: var(--phx-surface);
    color: var(--phx-text-2);
    text-align: center;
    cursor: pointer;
    overflow: hidden;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .gitem:hover { background: var(--phx-surface-3); }
  /* 格子里物品图直接浮在格子底上，别再套一层灰方块（稿图里也是纯图标） */
  .gitem :global(.ico) {
    background: transparent;
  }
  .gitem.on {
    /* 选中态做成「标签页」：实心底 + 外圈描边 + 顶边指示条；
       不用 accent-soft（半透明会把格子中间透出来，看着像「变透明了」），
       也不动 font-weight（字重一变名字会重排，鼠标移进去就跳一下）。 */
    background: var(--phx-surface-3);
    border-color: var(--phx-accent);
    box-shadow: inset 0 0 0 1px var(--phx-accent);
    color: var(--phx-text);
  }
  .gitem.on::after {
    content: '';
    position: absolute;
    inset: 0 0 auto 0;
    height: 3px;
    background: var(--phx-accent);
    border-radius: 5px 5px 0 0;
    pointer-events: none;
  }
  /* 单格只给两三个字（稿图那样 AK / 甲 / 弹），多格才铺全名 */
  .gitem {
    cursor: grab;
    transition: none;
  }
  .gitem.dragging {
    opacity: 0.22;
    border-style: dashed;
  }
  /* 落位成功：边框闪一下，让「放下了」看得见 */
  .gitem.pulse {
    animation: gpulse 0.7s ease-out;
  }
  @keyframes gpulse {
    0% {
      box-shadow: 0 0 0 0 var(--phx-accent);
      border-color: var(--phx-accent);
    }
    100% {
      box-shadow: 0 0 0 14px transparent;
      border-color: var(--phx-sep-strong);
    }
  }
  /* 跟随光标的幽灵块：半透明 + 轻微倾斜，抓起来的即时反馈 */
  .gdrag {
    position: fixed;
    left: 0;
    top: 0;
    z-index: 40;
    pointer-events: none;
    display: flex;
    align-items: flex-start;
    padding: 6px;
    border-radius: 7px;
    background: var(--phx-surface);
    border: 1px solid var(--phx-accent);
    box-shadow: 0 8px 22px rgba(0, 0, 0, 0.18);
    opacity: 0.92;
    will-change: transform;
  }
  .gdrag-name {
    font-size: 12px;
    line-height: 1.25;
    color: var(--phx-text);
    overflow: hidden;
    display: -webkit-box;
    line-clamp: 2;
    -webkit-line-clamp: 2;
  }
  .gname {
    font-size: 9.5px;
    line-height: 1.15;
    color: inherit;
    display: -webkit-box;
    line-clamp: 2;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-all;
    max-width: 100%;
  }
  .gname.wide {
    font-size: 10.5px;
    font-weight: 600;
    word-break: break-word;
  }
  .gnum {
    font-size: 10px;
    color: var(--phx-text-2);
    font-variant-numeric: tabular-nums;
  }
  .gmod {
    position: absolute;
    top: 3px;
    right: 3px;
    width: 10px;
    height: 10px;
    color: var(--phx-warn);
  }
  /* 选中条占位槽：高度＝选中条本身（29px）+ 下边距 8px，出现/消失都不改布局 */
  .gselslot {
    min-height: 37px;
  }
  .gsel {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
    padding: 6px 9px;
    border-radius: 8px;
    /* 实心底，不用半透明的 accent-soft，避免透出下面的格子 */
    background: var(--phx-surface-3);
    border: 1px solid var(--phx-accent-dim);
    font-size: 12px;
  }
  /* 没选中时占位槽里放一句操作提示，既有用又不会留下空白带 */
  .gsel.idle {
    background: transparent;
    border-color: transparent;
    color: var(--phx-text-3);
    font-size: 11.5px;
  }
  .gsname { font-weight: 600; color: var(--phx-text); }
  .gsid { color: var(--phx-text-3); font-size: 10.5px; font-family: var(--phx-mono, ui-monospace); }
  .gsbtn {
    padding: 3px 9px;
    font-size: 11.5px;
    border: 0;
    border-radius: 6px;
    background: var(--phx-surface);
    color: var(--phx-text);
    cursor: pointer;
  }
  .gsbtn.del { color: var(--phx-danger); }
  .ghint {
    margin-top: 8px;
    font-size: 11px;
    color: var(--phx-text-3);
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 11px;
    /* 与左栏「仓库格子」标题行（.ghead）等高下移，让第一张卡与格子卡片顶边对齐 */
    margin-top: 40px;
  }
  .card.mini {
    padding: 12px 13px;
    border-radius: var(--phx-r-card);
    background: var(--phx-surface);
    border: 0.5px solid var(--phx-sep);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .mini-h {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    font-size: 12px;
    font-weight: 600;
    color: var(--phx-text);
    margin-bottom: 2px;
  }
  .card.mini .between {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 12px;
  }
  .card.mini .muted { color: var(--phx-text-2); }
  .card.mini .num { font-variant-numeric: tabular-nums; }
  .qname {
    font-size: 11.5px;
    color: var(--phx-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .qiid {
    font-size: 10px;
    color: var(--phx-text-3);
    font-family: var(--phx-mono, ui-monospace);
  }
  .qrow {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 7px;
    padding: 5px 0;
    border-top: 0.5px solid var(--phx-sep);
  }
  .qadd {
    padding: 2px 8px;
    font-size: 11px;
    border: 0;
    border-radius: 5px;
    background: var(--phx-accent);
    color: #fff;
    cursor: pointer;
  }
  .qadd:disabled { opacity: 0.5; cursor: default; }
  .qnone {
    font-size: 11.5px;
    color: var(--phx-text-3);
    padding: 6px 0;
  }</style>
