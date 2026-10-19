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
  import PhxSegmented from '$lib/components/PhxSegmented.svelte'
  import { playSound } from '$lib/sound'

  /** 骨架屏行序号生成 */
  const skRows = (n: number) => [...Array(n).keys()]

  const ICON = {
    search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-3.8-3.8"/>',
    warn: '<path d="M12 3.5 22 20H2L12 3.5Z"/><path d="M12 10v4.5"/><circle cx="12" cy="17.2" r=".4" fill="currentColor"/>',
    plus: '<path d="M5 12h14M12 5v14"/>',
    trash: '<path d="M4 7h16M9 7V5h6v2m-8 0 1 13h8l1-13"/>'
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
  let addable = $state<{ tpl: string; name: string; desc: string; categoryId: string }[]>([])
  let rubles = $state(0)
  let dollars = $state(0)
  let euros = $state(0)
  let bonusRows = $state(0)
  let hasModded = $state(false)
  let loaded = $state(false)
  /** 首次加载中：骨架屏缓冲；数据到位或确认无档案后关闭 */
  let booting = $state(true)
  let saving = $state(false)
  let seg = $state<Seg>('stash')
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
      if (curCat !== rootId && a.categoryId !== curCat) return false
      if (!q) return true
      return a.name.toLowerCase().includes(q) || (aDesc && a.desc.toLowerCase().includes(q))
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

  const metaText = $derived(loaded ? t('stash.meta.loaded') : t('stash.meta.none'))

  const fmtMoney = (n: number): string => n.toLocaleString('en-US')

  /** 装备槽名 i18n：stash.slot.{slotId}，意外槽位原样显示 slotId */
  const slotName = (slot: string): string => t(`stash.slot.${slot}` as MessageKey)

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
    void runOps([{ op: 'addItem', tpl }], true)
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
    void refresh()
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
      <div class="toolbar">
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <span class="fspacer"></span>
        <i class="phx-sk sfld-sk"></i>
        <i class="phx-sk sfld-sk"></i>
      </div>
      <div class="scroll">
        <div class="stashmeta">
          {#each skRows(3) as r (r)}
            <i class="phx-sk mcap-sk"></i>
          {/each}
          <div class="bonus">
            <i class="phx-sk slim" style="width:56px;height:14px"></i>
            <i class="phx-sk step-sk"></i>
          </div>
        </div>
        <section class="qgroup">
          <div class="ghead">
            <i class="phx-sk slim" style="width:72px"></i>
            <i class="phx-sk gc-sk"></i>
          </div>
          <div class="qcard">
            {#each skRows(6) as r (r)}
              <div class="mrow">
                <i class="phx-sk av-sk"></i>
                <i class="phx-sk" style="width:62%"></i>
                <span></span>
                <i class="phx-sk" style="width:70%"></i>
                <div class="acts">
                  <i class="phx-sk abtn-sk"></i>
                  <i class="phx-sk abtn-sk"></i>
                </div>
              </div>
            {/each}
          </div>
          <div class="tfoot">
            <i class="phx-sk slim" style="width:128px"></i>
          </div>
        </section>
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
        <div class="sfield">
          <span class="sico"><PhxIcon markup={ICON.search} /></span>
          <input type="text" placeholder={t('stash.search.name')} bind:value={fName} />
        </div>
        <div class="sfield">
          <span class="sico"><PhxIcon markup={ICON.search} /></span>
          <input type="text" placeholder={t('stash.search.id')} bind:value={fId} />
        </div>
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
        <!-- 货币与附加行数：含 modded 物品时加钱区禁用（稿图注 4） -->
        <div class="stashmeta">
          {#each MONEY as m (m.tpl)}
            <div class="mcap" class:disabled={hasModded}>
              <span class="sym">{m.sym}</span>
              <span class="val">{fmtMoney(m.get())}</span>
              <PhxTooltip label={t('stash.aria.addMoney', { sym: m.sym })}>
                <button class="madd" type="button" aria-label={t('stash.aria.addMoney', { sym: m.sym })} disabled={hasModded || saving} onclick={() => addMoney(m.tpl, m.add)}>
                  <PhxIcon markup={ICON.plus} />
                </button>
              </PhxTooltip>
            </div>
          {/each}
          <div class="bonus">
            <span>{t('stash.bonus')}</span>
            <div class="step">
              <button type="button" aria-label="-2" onclick={() => stepBonus(-2)}>−</button>
              <span class="num">{bonusRows}</span>
              <button type="button" aria-label="+2" onclick={() => stepBonus(2)}>＋</button>
            </div>
          </div>
        </div>
        {#if hasModded}
          <div class="warn">
            <span class="warn-ico"><PhxIcon markup={ICON.warn} /></span>
            <span>{t('stash.warn.modded')}</span>
          </div>
        {/if}
        <section class="qgroup">
          <div class="ghead">
            <span class="gtitle">{t('stash.g.items')}</span>
            <span class="gcount">{visItems.length}</span>
          </div>
          <div class="qcard">
            {#each visItems as it (it.id)}
              <div class="mrow">
                <span class="icon-b">{it.name.charAt(0)}</span>
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
          {#if visItems.length}
            <div class="tfoot">
              {t('stash.foot', { n: visItems.length, m: moddedCount })}
            </div>
          {/if}
        </section>
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
            <div class="qcard">
              {#each visAddable as a (a.tpl)}
                <div class="arow">
                  <span class="icon-b">{a.name.charAt(0)}</span>
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
                  <span class="icon-b">{s.item.name.charAt(0)}</span>
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
            <!-- 口袋：特殊槽，内嵌物品列表（稿图注 6） -->
            <div class="pwrap">
              <div class="phead">{t('stash.slot.Pockets')}</div>
              {#each pockets as p (p.id)}
                <div class="pitem">
                  <span class="icon-b small">{p.name.charAt(0)}</span>
                  <div class="iname">{p.name}{p.count > 1 ? ` ×${p.count}` : ''}</div>
                  <span class="iid">{p.tpl}</span>
                  <PhxTooltip label={t('stash.aria.del')}>
                    <button class="abtn del" type="button" aria-label={t('stash.aria.del')} onclick={() => askRemove(p.id)}>
                      <PhxIcon markup={ICON.trash} />
                    </button>
                  </PhxTooltip>
                </div>
              {/each}
              {#if !pockets.length}<div class="pempty">{t('stash.row.empty')}</div>{/if}
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

  .boot .mcap-sk {
    width: 92px;
    height: 30px;
    border-radius: 999px;
  }

  .boot .step-sk {
    width: 88px;
    height: 26px;
    border-radius: 8px;
  }

  .boot .gc-sk {
    width: 24px;
    height: 16px;
    border-radius: 8px;
  }

  .boot .av-sk {
    width: 30px;
    height: 30px;
    border-radius: 7px;
  }

  .boot .abtn-sk {
    width: 26px;
    height: 26px;
    border-radius: 7px;
  }

  .boot .tfoot {
    display: flex;
    justify-content: center;
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
  .stashmeta {
    margin-top: 16px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .mcap {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 6px 0 12px;
    background: var(--phx-surface-2);
    border-radius: 999px;
  }
  .mcap.disabled {
    opacity: 0.5;
  }
  .mcap .sym {
    font-size: 12px;
    font-weight: 700;
    color: var(--phx-text-2);
  }
  .mcap .val {
    font-family: var(--phx-font-mono);
    font-size: 12px;
    color: var(--phx-text);
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
  .bonus {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--phx-text-2);
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
  .icon-b {
    width: 30px;
    height: 30px;
    border-radius: 7px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    font-weight: 700;
    flex: none;
  }
  .icon-b.small {
    width: 26px;
    height: 26px;
    font-size: 11px;
    border-radius: 6px;
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
  }
  .pitem {
    display: grid;
    grid-template-columns: 26px minmax(0, 1fr) minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    padding: 7px 14px 7px 26px;
  }
  .pempty {
    font-size: 12px;
    color: var(--phx-text-3);
    padding: 7px 14px 7px 26px;
  }
</style>
