<script lang="ts">
  // 08 藏身处页：按 docs/prototypes/08 稿图逐项落地（仅 PMC，Scav 无藏身处）。
  // 三子页签：区域（步进器 + 滑条双控件同绑等级，父子区域联动）/ 生产配方（已获得
  // 勾选 ↔ 存档解锁清单）/ 进行中生产（实验性；完成 = 剩余小时折回开始时间戳后置满，
  // 删除带确认）。800ms 防抖全量自动保存（稿图无保存按钮），Rust 侧先备份并按
  // stages 数卡控；父子同值联动是前端 UI 语义（每跳按目标自己 maxLevel 卡控），
  // 写入侧不做联动（联动会把上限不一致的家族互相拉扯、顶超各区域上限）。
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { t } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import {
    phxProfile,
    phxHideout,
    type PhxHideoutAreaRow,
    type PhxHideoutRecipeRow,
    type PhxHideoutCraftRow
  } from '$lib/ipc'
  import PhxDialog from '$lib/components/PhxDialog.svelte'
  import PhxIcon from '$lib/components/PhxIcon.svelte'
  import PhxTooltip from '$lib/components/PhxTooltip.svelte'
  import PhxSegmented from '$lib/components/PhxSegmented.svelte'
  import { playSound } from '$lib/sound'

  /** 骨架屏行序号生成 */
  const skRows = (n: number) => [...Array(n).keys()]

  const ICON = {
    search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-3.8-3.8"/>',
    warn: '<path d="M12 3.5 22 20H2L12 3.5Z"/><path d="M12 10v4.5"/><circle cx="12" cy="17.2" r=".4" fill="currentColor"/>',
    trash:
      '<path d="M4 7h16M9 7V5h6v2m-8 0 1 13h8l1-13"/><path d="M10 11v6M14 11v6"/>'
  }

  /** 子页签序（稿图顶部三段分段；页头动作区与过滤框随子页切换） */
  type Seg = 'areas' | 'recipes' | 'crafts'

  let currentId = $state('')
  let areas = $state<PhxHideoutAreaRow[]>([])
  let recipes = $state<PhxHideoutRecipeRow[]>([])
  let crafts = $state<PhxHideoutCraftRow[]>([])
  let loaded = $state(false)
  /** 首次加载中：骨架屏缓冲（感官层基线 §二）；数据到位或确认无档案后关闭 */
  let booting = $state(true)
  let saving = $state(false)
  let seg = $state<Seg>('areas')
  let fAreaName = $state('')
  let fName = $state('')
  let fArea = $state('')
  /** 删除确认弹层（行删除 / 移除全部共用） */
  let dlgOpen = $state(false)
  let dlgKind = $state<'one' | 'all'>('one')
  let dlgId = $state('')
  /** 自动保存防抖（非渲染态） */
  let saveTimer: ReturnType<typeof setTimeout> | undefined
  let dirty = false

  const metaText = $derived(loaded ? t('hideout.meta.loaded') : t('hideout.meta.none'))

  const visAreas = $derived.by(() => {
    const q = fAreaName.trim().toLowerCase()
    return q ? areas.filter((a) => a.name.toLowerCase().includes(q)) : areas
  })
  const visRecipes = $derived.by(() => {
    const q = fName.trim().toLowerCase()
    const qa = fArea.trim().toLowerCase()
    return recipes.filter(
      (r) =>
        (!q || `${r.name} ${r.productTpl}`.toLowerCase().includes(q)) &&
        (!qa || r.area.toLowerCase().includes(qa))
    )
  })
  const visCrafts = $derived.by(() => {
    const q = fName.trim().toLowerCase()
    const qa = fArea.trim().toLowerCase()
    return crafts.filter(
      (r) =>
        (!q || `${r.name} ${r.productTpl}`.toLowerCase().includes(q)) &&
        (!qa || r.area.toLowerCase().includes(qa))
    )
  })

  /** 区域等级比例条（纯展示——稿图注 3 提案增补） */
  const pctOf = (a: PhxHideoutAreaRow): number =>
    a.maxLevel > 0 ? Math.round((a.level / a.maxLevel) * 100) : a.level > 0 ? 100 : 0

  /** 生产行进度百分比（提案增补——稿图注 5） */
  const pctOfCraft = (c: PhxHideoutCraftRow): number =>
    c.productionTime > 0
      ? Math.min(100, Math.round((c.progress / c.productionTime) * 100))
      : c.finished
        ? 100
        : 0

  /**
   * 区域等级写入（前端主联动）：复刻参考 HideoutArea.Level setter 语义——
   * 每一跳都按目标自己的 maxLevel 卡控后再赋值，同值递归同步给父与子
   * （Rust 写入侧仅按 stages 数卡控，不做联动）
   */
  function applyArea(levels: Map<number, number>, row: PhxHideoutAreaRow, v: number): void {
    const want = Math.max(0, Math.min(row.maxLevel, Math.round(v)))
    if ((levels.get(row.type) ?? row.level) === want) return
    levels.set(row.type, want)
    if (row.parentType != null) {
      const parent = areas.find((a) => a.type === row.parentType)
      if (parent) applyArea(levels, parent, want)
    }
    for (const child of areas.filter((a) => a.parentType === row.type)) {
      applyArea(levels, child, want)
    }
  }

  function setArea(row: PhxHideoutAreaRow, v: number): void {
    const clamped = Math.max(0, Math.min(row.maxLevel, Math.round(v)))
    if (row.level === clamped) return
    playSound('ui')
    const levels = new Map<number, number>()
    applyArea(levels, row, clamped)
    for (const a of areas) {
      const lv = levels.get(a.type)
      if (lv != null) a.level = lv
    }
    scheduleSave()
  }

  function stepArea(row: PhxHideoutAreaRow, delta: number): void {
    setArea(row, row.level + delta)
  }

  /** 全部拉满：每行独立置为各自上限（不走父子联动——联动会让上限不一致的家族互相拉扯） */
  function maxAllAreas(): void {
    if (!loaded || !areas.length) return
    playSound('success')
    for (const a of areas) a.level = a.maxLevel
    scheduleSave()
  }

  /** 已获得勾选 ↔ 存档解锁清单（参考 Added 布尔） */
  function toggleRecipe(row: PhxHideoutRecipeRow): void {
    playSound('toggle')
    row.added = !row.added
    scheduleSave()
  }

  /** 添加全部配方（参考 AddAllCrafts） */
  function addAllRecipes(): void {
    if (!loaded || !recipes.length) return
    playSound('success')
    for (const r of recipes) r.added = true
    scheduleSave()
  }

  /** 设为已完成（参考 SetCraftFinished：剩余小时 × 3600 折回开始时间戳后置满） */
  function finishCraft(c: PhxHideoutCraftRow): void {
    if (c.finished) return
    playSound('success')
    c.startTimestamp -= Math.round((c.productionTime - c.progress) * 3600)
    c.progress = c.productionTime
    c.finished = true
    scheduleSave()
  }

  /** 全部完成：仅当存在未完成生产时可用 */
  function finishAllCrafts(): void {
    if (!crafts.some((c) => !c.finished)) return
    playSound('success')
    for (const c of crafts) {
      if (!c.finished) {
        c.startTimestamp -= Math.round((c.productionTime - c.progress) * 3600)
        c.progress = c.productionTime
        c.finished = true
      }
    }
    scheduleSave()
  }

  function askRemoveCraft(id: string): void {
    playSound('alert')
    dlgKind = 'one'
    dlgId = id
    dlgOpen = true
  }

  function askRemoveAllCrafts(): void {
    if (!crafts.length) return
    playSound('alert')
    dlgKind = 'all'
    dlgId = ''
    dlgOpen = true
  }

  function confirmRemove(): void {
    if (dlgKind === 'all') {
      crafts = []
    } else {
      crafts = crafts.filter((c) => c.id !== dlgId)
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
      await phxHideout.save(id, {
        areas: areas.map((a) => ({ type: a.type, level: a.level })),
        unlocked: recipes.filter((r) => r.added).map((r) => r.id),
        crafts: crafts.map((c) => ({
          recipeId: c.id,
          progress: c.progress,
          productionTime: c.productionTime,
          startTimestamp: c.startTimestamp
        }))
      })
      toast.success(t('hideout.toast.saved'))
      playSound('success')
    } catch {
      dirty = true
      toast.error(t('hideout.toast.failed'))
    } finally {
      saving = false
    }
  }

  /** 全量刷新：当前档案 → 藏身处数据（仅 PMC） */
  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    currentId = id ?? ''
    loaded = false
    booting = true
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = undefined
    dirty = false
    seg = 'areas'
    fAreaName = ''
    fName = ''
    fArea = ''
    if (!currentId) {
      areas = []
      recipes = []
      crafts = []
      booting = false
      return
    }
    const data = await phxHideout.load(currentId)
    if (!data) {
      areas = []
      recipes = []
      crafts = []
      booting = false
      return
    }
    areas = data.areas
    recipes = data.recipes
    crafts = data.crafts
    loaded = true
    booting = false
  }

  onMount(() => {
    void refresh()
    // 托盘「切换存档」后刷新藏身处（Rust 侧 emit；本页卸载时退订并落盘未保存变更）
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
      <div class="large-title">{t('nav.hideout')}</div>
      <div class="head-meta">{metaText}</div>
    </div>
    <div class="head-actions">
      {#if seg === 'areas'}
        <button class="btn primary" type="button" onclick={maxAllAreas} disabled={!loaded || !areas.length || saving}>
          {t('hideout.act.maxAreas')}
        </button>
      {:else if seg === 'recipes'}
        <button class="btn primary" type="button" onclick={addAllRecipes} disabled={!loaded || !recipes.length || saving}>
          {t('hideout.act.addAll')}
        </button>
      {:else}
        <button class="btn primary" type="button" onclick={finishAllCrafts} disabled={!crafts.some((c) => !c.finished) || saving}>
          {t('hideout.act.finishAll')}
        </button>
        <button class="btn ghost" type="button" onclick={askRemoveAllCrafts} disabled={!crafts.length || saving}>
          {t('hideout.act.removeAll')}
        </button>
      {/if}
    </div>
  </header>

  <div class="body">
    <!-- 骨架层：镜像工具行 + 区域卡网格真实布局；数据就绪加 .gone 淡出，与新内容交叉淡化（感官层基线 §二） -->
    <div class="boot phx-boot" class:gone={!booting}>
      <div class="toolbar">
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <span class="fspacer"></span>
        <i class="phx-sk sfld-sk"></i>
      </div>
      <div class="scroll">
        <section class="qgroup">
          <div class="ghead">
            <i class="phx-sk slim" style="width:64px"></i>
            <i class="phx-sk gc-sk"></i>
          </div>
          <div class="qcard">
            <div class="zgrid">
              {#each skRows(4) as r (r)}
                <div class="zcard">
                  <div class="zhead">
                    <i class="phx-sk" style="width:96px"></i>
                    <i class="phx-sk slim" style="width:44px"></i>
                    <i class="phx-sk zlv-sk"></i>
                  </div>
                  <div class="zbar"><i class="phx-sk zfill-sk"></i></div>
                  <div class="zctl">
                    <i class="phx-sk step-sk"></i>
                    <i class="phx-sk range-sk"></i>
                  </div>
                </div>
              {/each}
            </div>
          </div>
        </section>
      </div>
    </div>
  {#if loaded}
    <div class="toolbar phx-fade-in">
      <PhxSegmented
        bind:value={seg}
        ariaLabel={t('hideout.seg.aria')}
        options={[
          { value: 'areas', label: t('hideout.seg.areas') },
          { value: 'recipes', label: t('hideout.seg.recipes') },
          { value: 'crafts', label: t('hideout.seg.crafts') }
        ]}
      />
      <span class="fspacer"></span>
      {#if seg === 'areas'}
        <div class="sfield">
          <span class="sico"><PhxIcon markup={ICON.search} /></span>
          <input type="text" placeholder={t('hideout.search.area')} bind:value={fAreaName} />
        </div>
      {:else}
        <div class="sfield">
          <span class="sico"><PhxIcon markup={ICON.search} /></span>
          <input type="text" placeholder={t('hideout.search.name')} bind:value={fName} />
        </div>
        <div class="sfield">
          <span class="sico"><PhxIcon markup={ICON.search} /></span>
          <input type="text" placeholder={t('hideout.search.area')} bind:value={fArea} />
        </div>
      {/if}
    </div>

    <div class="scroll phx-fade-in">
      {#if seg === 'areas'}
        <section class="qgroup">
          <div class="ghead">
            <span class="gtitle">{t('hideout.g.areas')}</span>
            <span class="gcount">{visAreas.length}</span>
          </div>
          <div class="qcard">
            <div class="zgrid">
              {#each visAreas as a (a.type)}
                <div class="zcard">
                  <div class="zhead">
                    <span class="zname">{a.name}</span>
                    <span class="zkey">type {a.type}</span>
                    <span class="zlv">{a.level} / {a.maxLevel}</span>
                  </div>
                  <div class="zbar"><i style={`width:${pctOf(a)}%`}></i></div>
                  <div class="zctl">
                    <div class="stepper">
                      <button type="button" aria-label={t('hideout.aria.dec', { name: a.name })} onclick={() => stepArea(a, -1)}>−</button>
                      <span class="val">{a.level}</span>
                      <button type="button" aria-label={t('hideout.aria.inc', { name: a.name })} onclick={() => stepArea(a, 1)}>+</button>
                    </div>
                    <input
                      class="range"
                      type="range"
                      min="0"
                      max={a.maxLevel}
                      step="1"
                      value={a.level}
                      aria-label={t('hideout.aria.level', { name: a.name })}
                      oninput={(e) => setArea(a, Number((e.currentTarget as HTMLInputElement).value))}
                    />
                  </div>
                </div>
              {/each}
              {#if !visAreas.length}<div class="empty">{t('hideout.empty.none')}</div>{/if}
            </div>
          </div>
        </section>
      {:else if seg === 'recipes'}
        <section class="qgroup">
          <div class="ghead">
            <span class="gtitle">{t('hideout.g.recipes')}</span>
            <span class="gcount">{visRecipes.length}</span>
          </div>
          <div class="qcard">
            {#each visRecipes as r (r.id)}
              <div class="crow">
                <span class="icon-b">{r.name.charAt(0)}</span>
                <div class="cinfo">
                  <span class="cname">{r.name}</span>
                  <span class="ckey">{r.productTpl}</span>
                </div>
                <span class="itag">{r.area}</span>
                <div class="ckwrap">
                  <span class="cklab">{t('hideout.row.added')}</span>
                  <button
                    class="ck"
                    class:on={r.added}
                    type="button"
                    role="checkbox"
                    aria-checked={r.added}
                    aria-label={t('hideout.aria.added', { name: r.name })}
                    onclick={() => toggleRecipe(r)}
                  ></button>
                </div>
              </div>
            {/each}
            {#if !visRecipes.length}<div class="empty">{t('hideout.empty.none')}</div>{/if}
          </div>
        </section>
      {:else}
        <!-- 实验性警示横幅：对齐参考编辑器进行中生产子页顶部的 Warning 区 -->
        <div class="warn">
          <span class="warn-ico"><PhxIcon markup={ICON.warn} /></span>
          <span>{t('hideout.warn.experimental')}</span>
        </div>
        <section class="qgroup">
          <div class="ghead">
            <span class="gtitle">{t('hideout.g.crafts')}</span>
            <span class="gcount">{crafts.length}</span>
          </div>
          <div class="qcard">
            {#each visCrafts as c (c.id)}
              <div class="crow">
                <span class="icon-b">{c.name.charAt(0)}</span>
                <div class="cinfo">
                  <span class="cname">{c.name}</span>
                  <span class="ckey">{c.productTpl}</span>
                </div>
                <span class="itag">{c.area}</span>
                <span class="stat" class:fin={c.finished} class:run={!c.finished}>
                  {c.finished ? t('hideout.st.finished') : t('hideout.st.running')}
                </span>
                <span class="wpct">{pctOfCraft(c)}%</span>
                {#if !c.finished}
                  <button class="minibtn" type="button" onclick={() => finishCraft(c)}>
                    {t('hideout.act.finish')}
                  </button>
                {/if}
                <PhxTooltip label={t('hideout.aria.del')}>
                  <button class="del" type="button" aria-label={t('hideout.aria.del')} onclick={() => askRemoveCraft(c.id)}>
                    <PhxIcon markup={ICON.trash} />
                  </button>
                </PhxTooltip>
              </div>
            {/each}
            {#if !visCrafts.length}<div class="empty">{t('hideout.empty.crafts')}</div>{/if}
          </div>
        </section>
      {/if}
    </div>
  {:else}
    <div class="empty">{t('hideout.empty.noprofile')}</div>
  {/if}
  </div>
</div>

<PhxDialog
  bind:open={dlgOpen}
  heading={dlgKind === 'all' ? t('hideout.dlg.rmAll.title') : t('hideout.dlg.rm.title')}
  text={dlgKind === 'all'
    ? t('hideout.dlg.rmAll.text')
    : t('hideout.dlg.rm.text', { name: crafts.find((c) => c.id === dlgId)?.name ?? '' })}
  danger
  confirmText={t('hideout.dlg.rm.ok')}
  onconfirm={confirmRemove}
/>

<style>
  /* ---- 公共（与 quests / skills / health 页同源） ---- */
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
    width: 160px;
    height: 28px;
    border-radius: 8px;
  }

  .boot .gc-sk {
    width: 24px;
    height: 16px;
    border-radius: 8px;
  }

  .boot .zlv-sk {
    width: 56px;
    height: 16px;
    border-radius: 8px;
    margin-left: auto;
  }

  .boot .zfill-sk {
    width: 45%;
    height: 100%;
    border-radius: 2px;
  }

  .boot .step-sk {
    width: 96px;
    height: 28px;
    border-radius: 8px;
  }

  .boot .range-sk {
    flex: 1;
    height: 4px;
    border-radius: 2px;
  }

  /* ---- 工具行：子页分段 + 过滤框 ---- */
  .toolbar {
    margin: 12px 24px 0;
    display: flex;
    align-items: center;
    gap: 10px;
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

  /* ---- 分组（iOS grouped list：组头在卡外） ---- */
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
  .qcard {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow: hidden;
  }

  /* ---- 区域卡：区域名 + 等级比例条（提案）+ 步进器/滑条双控件 ---- */
  .zgrid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 10px;
    padding: 12px;
  }
  .zcard {
    background: var(--phx-surface-2);
    border-radius: 9px;
    padding: 12px 14px;
  }
  .zhead {
    display: flex;
    align-items: baseline;
    gap: 7px;
    min-width: 0;
  }
  .zname {
    font-size: 14px;
    font-weight: 600;
    color: var(--phx-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .zkey {
    font-size: 11px;
    color: var(--phx-text-3);
    font-family: var(--phx-font-mono);
    white-space: nowrap;
  }
  .zlv {
    margin-left: auto;
    flex: none;
    font-family: var(--phx-font-mono);
    font-size: 11px;
    color: var(--phx-text-2);
    background: var(--phx-fill-tertiary);
    border-radius: 8px;
    padding: 1px 7px;
  }
  .zbar {
    height: 4px;
    border-radius: 2px;
    background: var(--phx-fill-quaternary);
    margin: 11px 0 13px;
    overflow: hidden;
  }
  .zbar i {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: var(--phx-accent);
    transition: width var(--phx-dur-norm) var(--phx-ease-out);
  }
  .zctl {
    display: flex;
    align-items: center;
    gap: 10px;
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
    min-width: 34px;
    text-align: center;
    font-family: var(--phx-font-mono);
    font-size: 12px;
    color: var(--phx-text);
  }
  .range {
    flex: 1;
    min-width: 0;
    appearance: none;
    -webkit-appearance: none;
    height: 4px;
    border-radius: 2px;
    background: var(--phx-fill-quaternary);
    outline: none;
    cursor: pointer;
  }
  .range::-webkit-slider-thumb {
    appearance: none;
    -webkit-appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    border: none;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
  }
  .range::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #fff;
    border: none;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
  }

  /* ---- 配方 / 进行中生产：行式列表 ---- */
  .crow {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 14px;
  }
  .crow + .crow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .icon-b {
    width: 30px;
    height: 30px;
    border-radius: 7px;
    flex: none;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    font-size: 12px;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .cinfo {
    min-width: 0;
    flex: 1;
    display: flex;
    align-items: baseline;
    gap: 7px;
  }
  .cname {
    font-size: 13px;
    font-weight: 600;
    color: var(--phx-text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ckey {
    font-size: 10.5px;
    color: var(--phx-text-3);
    font-family: var(--phx-font-mono);
    white-space: nowrap;
  }
  .itag {
    flex: none;
    font-size: 10.5px;
    color: var(--phx-text-2);
    background: var(--phx-fill-tertiary);
    border-radius: 7px;
    padding: 2px 8px;
  }
  /* 已获得勾选（自绘 checkbox，参考 Added 布尔） */
  .ckwrap {
    margin-left: auto;
    flex: none;
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .cklab {
    flex: none;
    font-size: 11px;
    color: var(--phx-text-3);
  }
  .ck {
    width: 20px;
    height: 20px;
    border-radius: 6px;
    border: 0.5px solid var(--phx-sep-strong);
    background: transparent;
    position: relative;
    cursor: pointer;
    padding: 0;
    transition:
      background 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .ck:hover {
    background: var(--phx-fill-quaternary);
  }
  .ck.on {
    background: var(--phx-accent);
    border-color: var(--phx-accent);
  }
  .ck.on::after {
    content: '';
    position: absolute;
    left: 6px;
    top: 2.5px;
    width: 5px;
    height: 9px;
    border-right: 2px solid #fff;
    border-bottom: 2px solid #fff;
    transform: rotate(45deg);
  }

  /* ---- 进行中生产：状态标签 / 进度 / 行内操作 ---- */
  .stat {
    flex: none;
    font-size: 11px;
    font-weight: 500;
    padding: 2px 9px;
    border-radius: 8px;
  }
  .stat.run {
    color: var(--phx-accent);
    background: var(--phx-accent-dim);
  }
  .stat.fin {
    color: var(--phx-text-2);
    background: var(--phx-fill-tertiary);
  }
  .wpct {
    flex: none;
    font-family: var(--phx-font-mono);
    font-size: 11px;
    color: var(--phx-text-3);
  }
  .minibtn {
    flex: none;
    height: 24px;
    padding: 0 10px;
    border: none;
    border-radius: 7px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    font-family: inherit;
    font-size: 11.5px;
    font-weight: 500;
    cursor: pointer;
    transition:
      background 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .minibtn:hover {
    filter: brightness(1.25);
  }
  .del {
    flex: none;
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--phx-text-3);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    transition:
      background 0.12s,
      color 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .del :global(svg) {
    width: 13px;
    height: 13px;
  }
  .del:hover {
    background: var(--phx-red-dim);
    color: var(--phx-red);
  }

  /* ---- 实验性警示横幅（orange 状态 token） ---- */
  .warn {
    margin: 16px 24px 0;
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
</style>
