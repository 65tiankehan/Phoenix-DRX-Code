<script lang="ts">
  import { onMount } from 'svelte'
  import { listen } from '@tauri-apps/api/event'
  import { open, save } from '@tauri-apps/plugin-dialog'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import {
    phxProfile,
    phxPresets,
    type PhxBuildView,
    type PhxEquipmentPresetView,
    type PhxProfileHero,
    phxArsenal,
    type PhxArsenalSet
  } from '$lib/ipc'
  import PhxDialog from '$lib/components/PhxDialog.svelte'
  import PhxIcon from '$lib/components/PhxIcon.svelte'
  import PhxTooltip from '$lib/components/PhxTooltip.svelte'
  import PhxSegmented from '$lib/components/PhxSegmented.svelte'
  import { playSound } from '$lib/sound'

  /** 骨架屏行序号生成 */
  const skRows = (n: number) => [...Array(n).keys()]

  type Seg = 'weapon' | 'equipment'

  /** 内联图标（markup 注入 PhxIcon） */
  const ICON = {
    search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-3.8-3.8"/>',
    warn: '<path d="M12 3.5 22 20H2L12 3.5Z"/><path d="M12 10v4.5"/><circle cx="12" cy="17.2" r=".4" fill="currentColor"/>',
    caret: '<path d="m9 5 7 7-7 7"/>',
    eye: '<path d="M2.5 12S6 6.5 12 6.5 21.5 12 21.5 12 18 17.5 12 17.5 2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="2.6"/>'
  }

  let currentId = $state('')
  let weapons = $state<PhxBuildView[]>([])
  let equipment = $state<PhxEquipmentPresetView[]>([])
  let hero = $state<PhxProfileHero | null>(null)
  let booting = $state(true)
  let saving = $state(false)
  let seg = $state<Seg>('weapon')
  let fName = $state('')
  let openIds = $state<string[]>([])
  let dlgOpen = $state(false)
  let dlgHeading = $state('')
  let dlgText = $state('')
  let dlgAction: (() => Promise<void>) | null = null

  /** 页头说明：SPT 版本 · 档案已加载 · 预设说明（稿图三段拼接） */
  const metaText = $derived.by(() => {
    const v = hero?.gameVersion
    return v ? `${t('home.meta.loaded', { v })} · ${t('presets.meta')}` : t('presets.meta')
  })

  const visWeapons = $derived.by(() => {
    const q = fName.trim().toLowerCase()
    if (!q) return weapons
    return weapons.filter((b) => b.name.toLowerCase().includes(q) || b.rootName.toLowerCase().includes(q))
  })
  const visEquipment = $derived.by(() => {
    const q = fName.trim().toLowerCase()
    if (!q) return equipment
    return equipment.filter((e) => e.name.toLowerCase().includes(q))
  })

  /** 过滤时显示「n / m」，否则纯数字 */
  const gcText = (rows: number, total: number): string => (rows !== total ? `${rows} / ${total}` : `${rows}`)
  const slotName = (slot: string): string => t(`stash.slot.${slot}` as MessageKey)
  /** 文件名净化：替换非法字符 */
  const safeName = (name: string): string => `${name.replace(/[\\/:*?"<>|]/g, '_')}.json`

  /** 只重拉预设数据（写操作后对账） */
  async function loadAll(): Promise<void> {
    if (!currentId) return
    const data = await phxPresets.load(currentId)
    weapons = data?.weapons ?? []
    equipment = data?.equipment ?? []
  }

  /** 全量刷新：首次进入 / 托盘切档 */
  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    currentId = id ?? ''
    booting = true
    seg = 'weapon'
    fName = ''
    openIds = []
    hero = null
    if (!currentId) {
      weapons = []
      equipment = []
      booting = false
      return
    }
    const [data, h] = await Promise.all([phxPresets.load(currentId), phxProfile.load(currentId)])
    hero = h
    weapons = data?.weapons ?? []
    equipment = data?.equipment ?? []
    booting = false
  }

  function askDlg(heading: string, text: string, action: () => Promise<void>): void {
    playSound('alert')
    dlgHeading = heading
    dlgText = text
    dlgAction = action
    dlgOpen = true
  }

  function confirmDlg(): void {
    const fn = dlgAction
    dlgAction = null
    if (fn) void fn()
  }

  function failToast(e: unknown): void {
    toast.error(t('presets.toast.failed'))
    if (typeof e === 'string' && e) toast.error(e)
  }

  /** 统一写操作：saving 互斥 + 成功音 + 写后对账 */
  async function runWrite(fn: () => Promise<unknown>): Promise<void> {
    if (!currentId || saving) return
    saving = true
    try {
      await fn()
      playSound('success')
      toast.success(t('presets.toast.saved'))
    } catch (e) {
      failToast(e)
    } finally {
      saving = false
      await loadAll()
    }
  }

  /** 武器预设加入仓库（拷贝语义，预设保留，故不重读预设） */
  async function toStash(b: PhxBuildView): Promise<void> {
    if (!currentId || saving) return
    playSound('gain')
    saving = true
    try {
      await phxPresets.toStash(currentId, 'weapon', b.id)
      playSound('success')
      toast.success(t('presets.toast.saved'))
    } catch (e) {
      failToast(e)
    } finally {
      saving = false
    }
  }

  async function exportOne(kind: Seg, target: string, name: string): Promise<void> {
    if (!currentId || saving) return
    const path = await save({
      defaultPath: safeName(name),
      filters: [{ name: 'JSON', extensions: ['json'] }]
    })
    if (!path) return
    saving = true
    try {
      await phxPresets.export(currentId, kind, target, path)
      playSound('success')
      toast.success(t('presets.toast.exported', { name }))
    } catch (e) {
      failToast(e)
    } finally {
      saving = false
    }
  }

  function removeOne(kind: Seg, target: string, name: string): void {
    if (!currentId || saving) return
    askDlg(t('presets.dlg.removeTitle'), t('presets.dlg.removeText', { name }), () =>
      runWrite(() => phxPresets.remove(currentId, kind, target))
    )
  }

  function removeAll(): void {
    if (!currentId || saving) return
    const cur = seg === 'weapon' ? weapons : equipment
    if (cur.length === 0) {
      toast.warning(t('presets.toast.noneRemove'))
      return
    }
    const kind: Seg = seg
    askDlg(
      t(kind === 'weapon' ? 'presets.dlg.clearWeapons' : 'presets.dlg.clearEquip'),
      '',
      () => runWrite(() => phxPresets.removeAll(currentId, kind))
    )
  }

  async function exportAll(): Promise<void> {
    if (!currentId || saving) return
    const cur = seg === 'weapon' ? weapons : equipment
    if (cur.length === 0) {
      toast.warning(t('presets.toast.noneExport'))
      return
    }
    const dir = await open({ directory: true })
    if (!dir || typeof dir !== 'string') return
    const kind: Seg = seg
    saving = true
    try {
      const n = await phxPresets.exportAll(currentId, kind, dir)
      playSound('success')
      toast.success(t('presets.toast.exportedAll', { n }))
    } catch (e) {
      failToast(e)
    } finally {
      saving = false
    }
  }

  async function importFiles(): Promise<void> {
    if (!currentId || saving) return
    const picked = await open({
      multiple: true,
      filters: [{ name: 'JSON', extensions: ['json'] }]
    })
    const list = Array.isArray(picked) ? picked : picked ? [picked] : []
    if (list.length === 0) return
    const kind: Seg = seg
    saving = true
    try {
      const r = await phxPresets.import(currentId, kind, list)
      if (r.imported > 0) {
        playSound('success')
        toast.success(t('presets.toast.imported', { n: r.imported }))
      }
      if (r.failed.length > 0) {
        toast.warning(t('presets.toast.importFailed', { m: r.failed.length, list: r.failed.join('、') }))
      }
    } catch (e) {
      failToast(e)
    } finally {
      saving = false
      await loadAll()
    }
  }

  function toggleCard(id: string): void {
    playSound('ui')
    openIds = openIds.includes(id) ? openIds.filter((x) => x !== id) : [...openIds, id]
  }

  /** 预设内物品检视（待办：暂以轻提示占位） */
  function inspectSlot(): void {
    toast.warning(t('presets.toast.inspectPending'))
  }

  // —— 套装军械库（稿图 32，0.2.0 第 3 段；导航裁决=预设页 tab）——
  type View = 'mine' | 'arsenal'
  let view = $state<View>('mine')
  let sets = $state<PhxArsenalSet[]>([])
  let chosen = $state<string>('')
  const chosenSet = $derived(sets.find((s) => s.id === chosen) ?? null)
  let applying = $state(false)

  const fmtRub = (n: number): string => n.toLocaleString('zh-CN')

  function kindLabel(kind: string): string {
    switch (kind) {
      case 'armor': return t('arsenal.kind.armor')
      case 'helmet': return t('arsenal.kind.helmet')
      case 'backpack': return t('arsenal.kind.backpack')
      default: return t('arsenal.kind.rig')
    }
  }

  async function loadArsenal(): Promise<void> {
    try {
      sets = await phxArsenal.load()
      if (sets.length > 0 && !sets.some((s) => s.id === chosen)) chosen = sets[0].id
    } catch (e) {
      toast.error(String(e))
    }
  }
  async function applySet(weaponOnly: boolean): Promise<void> {
    if (!chosenSet || applying) return
    applying = true
    try {
      await phxArsenal.apply(chosenSet.id, weaponOnly)
      toast.success(t(weaponOnly ? 'arsenal.doneWeapon' : 'arsenal.done'))
    } catch (e) {
      toast.error(String(e))
    } finally {
      applying = false
    }
  }

  onMount(() => {
    void loadArsenal()
    void refresh()
    const off = listen('tray:profile-switched', () => void refresh())
    return () => {
      void off.then((unlisten) => unlisten())
    }
  })
</script>

<div class="page">
  <header class="main-head">
    <div>
      <h1 class="large-title">{t('presets.title')}</h1>
      <div class="head-meta">{metaText}</div>
    </div>
    <div class="head-actions">
      <PhxSegmented
        size="md"
        role="tablist"
        ariaLabel={t('presets.view.aria')}
        bind:value={view}
        options={[
          { value: 'mine', label: t('presets.view.mine') },
          { value: 'arsenal', label: t('presets.view.arsenal') }
        ]}
      />
      {#if view === 'mine'}
      <button class="btn ghost" disabled={!currentId || saving} onclick={() => void importFiles()}>
        {t('presets.act.import')}
      </button>
      <button class="btn ghost" disabled={!currentId || saving} onclick={() => void exportAll()}>
        {t('presets.act.exportAll')}
      </button>
      <button class="btn danger-ghost" disabled={!currentId || saving} onclick={() => removeAll()}>
        {t('presets.act.removeAll')}
      </button>
      {/if}
    </div>
  </header>

  <div class="body">
    {#if view === 'arsenal'}
      <!-- ===== 套装军械库（稿图 32；价格仅展示，入库不扣钱；写入走备份链） ===== -->
      <div class="arsenal phx-fade-in">
        <div class="ar-wall">
          {#each sets as set (set.id)}
            <button class="ar-set" type="button" class:on={set.id === chosen} onclick={() => (chosen = set.id)}>
              <div class="ar-thumb" data-tier={set.tier}>
                <svg viewBox="0 0 108 54" fill="none">
                  <path class="l" d="M4 24 L14 21 L14 35 L7 34 Z"/>
                  <rect class="l" x="14" y="22" width="28" height="12" rx="1.5"/>
                  <path class="l" d="M24 34 L32 34 L30 46 L23 44 Z"/>
                  <path class="l" d="M34 34 L42 34 L45 50 L37 50 Z"/>
                  <rect class="l" x="42" y="24" width="26" height="9" rx="1.5"/>
                  <rect class="l" x="68" y="25" width="20" height="4.5"/>
                  <rect class="l" x="88" y="24" width="6" height="7" rx="1"/>
                  <path class="l2" d="M72 20 L74 20 L74 25"/>
                </svg>
                <span class="ar-tier">{set.tier}</span>
              </div>
              <div class="ar-body">
                <div class="ar-n">{set.name}</div>
                <div class="ar-price">≈ {fmtRub(set.priceRub)} ₽</div>
              </div>
            </button>
          {/each}
        </div>
        {#if chosenSet}
          <div class="ar-detail">
            <div class="ar-hero">
              <svg viewBox="0 0 108 54" fill="none">
                <path class="l" d="M4 24 L14 21 L14 35 L7 34 Z"/>
                <rect class="l" x="14" y="22" width="28" height="12" rx="1.5"/>
                <path class="l" d="M24 34 L32 34 L30 46 L23 44 Z"/>
                <path class="l" d="M34 34 L42 34 L45 50 L37 50 Z"/>
                <rect class="l" x="42" y="24" width="26" height="9" rx="1.5"/>
                <rect class="l" x="68" y="25" width="20" height="4.5"/>
                <rect class="l" x="88" y="24" width="6" height="7" rx="1"/>
                <path class="l2" d="M72 20 L74 20 L74 25"/>
                <rect class="l2" x="20" y="15" width="18" height="6" rx="2"/>
              </svg>
            </div>
            <div class="ar-h">
              <div class="ar-t">{chosenSet.name}</div>
              <div class="ar-sub">{chosenSet.desc}</div>
            </div>
            <div class="ar-list">
              {#if chosenSet.weapon}
                <div class="ar-cat">{t('arsenal.cat.weapon')}</div>
                <div class="ar-item">
                  <span class="ar-slot w">{t('arsenal.slot.weapon')}</span>
                  {#if chosenSet.weapon.name}
                    <span class="ar-in">{chosenSet.weapon.name}</span>
                  {:else}
                    <span class="ar-miss">{t('arsenal.missing', { name: chosenSet.weapon.detail })}</span>
                  {/if}
                </div>
              {/if}
              {#if chosenSet.pieces.length > 0}
                <div class="ar-cat">{t('arsenal.cat.gear')}</div>
                {#each chosenSet.pieces as piece, i (i)}
                  <div class="ar-item">
                    <span class="ar-slot g">{kindLabel(piece.kind)}</span>
                    {#if piece.name}
                      <span class="ar-in">{piece.name}</span>
                    {:else}
                      <span class="ar-miss">{t('arsenal.missing', { name: piece.detail })}</span>
                    {/if}
                  </div>
                {/each}
              {/if}
            </div>
            <div class="ar-f">
              <div class="ar-price-line">
                <span class="ar-num">≈ {fmtRub(chosenSet.priceRub)} ₽</span>
                <span class="ar-cap">{t('arsenal.priceNote')}</span>
              </div>
              <div class="ar-btns">
                <button class="btn ghost" type="button" disabled={applying} onclick={() => void applySet(true)}>
                  {t('arsenal.weaponOnly')}
                </button>
                <button class="btn primary" type="button" disabled={applying} onclick={() => void applySet(false)}>
                  {applying ? t('arsenal.applying') : t('arsenal.applyAll')}
                </button>
              </div>
              <div class="ar-safe">{t('arsenal.backupNote')}</div>
            </div>
          </div>
        {/if}
      </div>
    {:else}
    <!-- 骨架层：镜像工具条 + 武器预设卡真实布局；数据就绪加 .gone 淡出，与新内容交叉淡化（感官层基线 §二） -->
    <div class="boot phx-boot" class:gone={!booting}>
      <div class="toolbar">
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk grow"></i>
        <i class="phx-sk sfld-sk"></i>
      </div>
      <div class="scroll">
        <div class="ghead">
          <i class="phx-sk" style="width:56px"></i>
          <i class="phx-sk pill"></i>
        </div>
        {#each skRows(2) as c (c)}
          <section class="wcard">
            <div class="whead">
              <i class="phx-sk" style="width:118px"></i>
              <div class="wbtns">
                <i class="phx-sk wbtn-sk"></i>
                <i class="phx-sk wbtn-sk"></i>
                <i class="phx-sk wbtn-sk"></i>
              </div>
            </div>
            <div class="wbody">
              <div class="wweapon">
                <div class="wgun">
                  <i class="phx-sk iconb-sk"></i>
                  <div class="wgun-t">
                    <i class="phx-sk" style="width:150px"></i>
                    <i class="phx-sk slim" style="width:96px"></i>
                  </div>
                </div>
                <div class="attrs">
                  <div class="attr"><i class="phx-sk slim" style="width:34px"></i><i class="phx-sk" style="width:46px"></i></div>
                  <div class="attr"><i class="phx-sk slim" style="width:34px"></i><i class="phx-sk" style="width:46px"></i></div>
                  <div class="attr"><i class="phx-sk slim" style="width:34px"></i><i class="phx-sk" style="width:46px"></i></div>
                </div>
              </div>
              <div class="wparts">
                <div class="ptitle"><i class="phx-sk slim" style="width:64px"></i></div>
                {#each skRows(3) as p (p)}
                  <div class="part">
                    <i class="phx-sk dot-sk"></i>
                    <i class="phx-sk grow"></i>
                  </div>
                {/each}
              </div>
            </div>
          </section>
        {/each}
      </div>
    </div>

    {#if currentId}
      <div class="toolbar phx-fade-in">
      <PhxSegmented
        bind:value={seg}
        ariaLabel={t('presets.seg.aria')}
        options={[
          { value: 'weapon', label: t('presets.seg.weapon') },
          { value: 'equipment', label: t('presets.seg.equipment') }
        ]}
      />
      <div class="fspacer"></div>
      <div class="sfield">
        <span class="sico"><PhxIcon markup={ICON.search} /></span>
        <input bind:value={fName} placeholder={t('presets.searchPh')} />
      </div>
    </div>

    {#if seg === 'weapon'}
      <div class="scroll phx-fade-in">
        <div class="ghead">
          <div class="gtitle">{t('presets.seg.weapon')}</div>
          <div class="gcount">{gcText(visWeapons.length, weapons.length)}</div>
        </div>
        {#each visWeapons as b (b.id)}
          <div class="wcard">
            <div class="whead">
              <div class="wname">{b.name}</div>
              <div class="wmod" class:show={b.hasModded}><PhxIcon markup={ICON.warn} /></div>
              <div class="wbtns">
                <button class="wbtn" onclick={() => void toStash(b)}>{t('presets.w.toStash')}</button>
                <button class="wbtn" onclick={() => void exportOne('weapon', b.id, b.name)}>
                  {t('presets.act.export')}
                </button>
                <button class="wbtn danger" onclick={() => removeOne('weapon', b.id, b.name)}>
                  {t('presets.act.remove')}
                </button>
              </div>
            </div>
            <div class="wbody">
              <div class="wweapon">
                <div class="wgun">
                  <div class="icon-b">{b.rootName.slice(0, 1)}</div>
                  <div>
                    <div class="wgun-n">{b.rootName}</div>
                    <div class="wgun-s">{b.rootTpl}</div>
                  </div>
                </div>
                <div class="attrs">
                  <div class="attr">{t('presets.attr.ergo')}<div class="av">{b.ergonomics}</div></div>
                  <div class="attr">{t('presets.attr.up')}<div class="av">{b.recoilUp}</div></div>
                  <div class="attr">{t('presets.attr.back')}<div class="av">{b.recoilBack}</div></div>
                </div>
              </div>
              <div class="wparts">
                <div class="ptitle">{t('presets.w.parts')}</div>
                {#each b.parts as p (p.id)}
                  <div class="part"><span class="dot">{p.name.slice(0, 1)}</span>{p.name}</div>
                {/each}
              </div>
            </div>
          </div>
        {:else}
          <div class="empty">{weapons.length ? t('presets.empty.match') : t('presets.empty.weapon')}</div>
        {/each}
      </div>
    {:else}
      <div class="scroll phx-fade-in">
        <div class="warn"><PhxIcon markup={ICON.warn} />{t('presets.warn.equip')}</div>
        <div class="ghead">
          <div class="gtitle">{t('presets.seg.equipment')}</div>
          <div class="gcount">{gcText(visEquipment.length, equipment.length)}</div>
        </div>
        {#each visEquipment as e (e.id)}
          <div class="ecard" class:open={openIds.includes(e.id)}>
            <div class="ehead">
              <button
                class="etoggle"
                aria-expanded={openIds.includes(e.id)}
                onclick={() => toggleCard(e.id)}
              >
                <span class="ecaret"><PhxIcon markup={ICON.caret} /></span>
                <span class="ename">{e.name}</span>
              </button>
              <div class="ebtns">
                <button class="wbtn" onclick={() => void exportOne('equipment', e.id, e.name)}>
                  {t('presets.act.export')}
                </button>
                <button class="wbtn danger" onclick={() => removeOne('equipment', e.id, e.name)}>
                  {t('presets.act.remove')}
                </button>
              </div>
            </div>
            <div class="ebody">
              {#each e.slots as row (row.slot)}
                <div class="srow">
                  <div class="slotname">{slotName(row.slot)}</div>
                  {#if row.slot === 'Pockets'}
                    <div class="pwrap">
                      {#if e.pockets.length}
                        {#each e.pockets as p (p.id)}
                          <div class="pitem">
                            <div class="icon-b small">{p.name.slice(0, 1)}</div>
                            <div class="iname">{p.name}{p.count > 1 ? ` ×${p.count}` : ''}</div>
                            <div class="iid">{p.id}</div>
                            <div class="acts"></div>
                          </div>
                        {/each}
                      {:else}
                        <div class="pitem"><div class="pempty">{t('presets.pocket.empty')}</div></div>
                      {/if}
                    </div>
                  {:else if row.item}
                    <div class="icon-b">{row.item.name.slice(0, 1)}</div>
                    <div class="iname">{row.item.name}{row.item.count > 1 ? ` ×${row.item.count}` : ''}</div>
                    <div class="mmod" class:show={row.item.modded}><PhxIcon markup={ICON.warn} /></div>
                    <div class="iid">{row.item.id}</div>
                    <div class="acts">
                      {#if row.item.isWeapon || row.item.isContainer}
                        <PhxTooltip label={row.item.isWeapon ? t('stash.aria.inspect') : t('presets.aria.openContainer')}>
                          <button
                            class="abtn"
                            aria-label={row.item.isWeapon ? t('stash.aria.inspect') : t('presets.aria.openContainer')}
                            onclick={() => inspectSlot()}
                          >
                            <PhxIcon markup={ICON.eye} />
                          </button>
                        </PhxTooltip>
                      {/if}
                    </div>
                  {:else}
                    <div class="icon-b blank">—</div>
                    <div class="iname blank">{t('presets.slot.empty')}</div>
                    <div class="mmod"></div>
                    <div class="iid">—</div>
                    <div class="acts"></div>
                  {/if}
                </div>
              {/each}
            </div>
          </div>
        {:else}
          <div class="empty">{equipment.length ? t('presets.empty.match') : t('presets.empty.equipment')}</div>
        {/each}
      </div>
    {/if}
  {:else if !booting}
    <div class="scroll"><div class="empty">{t('stash.empty.noprofile')}</div></div>
  {/if}
      {/if}
</div>

  <PhxDialog
    bind:open={dlgOpen}
    heading={dlgHeading}
    text={dlgText}
    danger
    confirmText={t('stash.dlg.ok')}
    onconfirm={confirmDlg}
  />
</div>

<style>
  /* ===== 公共（与 inspect 页同源生产值）===== */
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
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }

  .btn :global(svg) {
    width: 14px;
    height: 14px;
    flex: none;
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

  /* 页级定位包装层：包住常驻骨架层与内容，骨架 absolute 覆盖时避开页头 */
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .boot {
    padding: 16px 24px 104px;
  }

  /* 骨架内复用真实布局 class，覆写其外距与滚动 */
  .boot .toolbar {
    margin: 0;
  }

  .boot .scroll {
    margin-top: 0;
    padding: 0;
  }

  /* 骨架控件形状（对齐真实控件几何） */
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

  .boot .wbtn-sk {
    width: 58px;
    height: 26px;
    border-radius: 7px;
  }

  .boot .iconb-sk {
    width: 38px;
    height: 38px;
    border-radius: 9px;
    flex: none;
  }

  .boot .wgun-t {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .boot .attr {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .boot .dot-sk {
    width: 20px;
    height: 20px;
    border-radius: 5px;
    flex: none;
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    margin-top: 12px;
    padding: 0 24px 104px;
  }

  .empty {
    padding: 44px 0;
    text-align: center;
    color: var(--phx-text-3);
    font-size: 13px;
  }

  /* ===== 工具条 ===== */
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

  /* ===== 通用条目件 ===== */
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

  .icon-b.blank {
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

  .iname.blank {
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
    justify-content: flex-end;
    gap: 6px;
  }

  .warn {
    margin: 16px 0 0;
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--phx-warn-bg);
    box-shadow: inset 0 0 0 0.5px var(--phx-warn-bd);
    color: var(--phx-warn);
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .warn :global(svg) {
    width: 14px;
    height: 14px;
    flex: none;
  }

  /* ===== 武器预设卡 ===== */
  .wcard {
    margin-top: 16px;
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow: hidden;
  }

  .whead {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
    border-bottom: 0.5px solid var(--phx-sep);
  }

  .wname {
    font-size: 13.5px;
    font-weight: 700;
  }

  .wmod {
    display: flex;
    align-items: center;
    color: var(--phx-danger);
    visibility: hidden;
    flex: none;
  }

  .wmod.show {
    visibility: visible;
  }

  .wmod :global(svg) {
    width: 14px;
    height: 14px;
  }

  .wbtns {
    margin-left: auto;
    display: flex;
    gap: 6px;
  }

  .wbtn {
    height: 26px;
    padding: 0 12px;
    border: none;
    border-radius: 7px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    cursor: pointer;
  }

  .wbtn:hover {
    filter: brightness(1.2);
  }

  .wbtn.danger {
    background: transparent;
    color: var(--phx-text-3);
  }

  .wbtn.danger:hover {
    background: var(--phx-danger-bg);
    color: var(--phx-danger);
  }

  .wbody {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.2fr);
  }

  .wweapon {
    padding: 14px 16px;
    border-right: 0.5px solid var(--phx-sep);
  }

  .wgun {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .wgun .icon-b {
    width: 38px;
    height: 38px;
    font-size: 14px;
  }

  .wgun-n {
    font-size: 13px;
    font-weight: 600;
  }

  .wgun-s {
    font-size: 11px;
    color: var(--phx-text-3);
    margin-top: 2px;
  }

  .attrs {
    display: flex;
    gap: 26px;
    margin-top: 16px;
  }

  .attr {
    font-size: 11px;
    color: var(--phx-text-3);
  }

  .attr .av {
    font-size: 15px;
    font-weight: 700;
    color: var(--phx-text);
    margin-top: 3px;
    font-variant-numeric: tabular-nums;
  }

  .wparts {
    min-width: 0;
  }

  .ptitle {
    font-size: 12px;
    font-weight: 600;
    color: var(--phx-text-2);
    padding: 12px 16px 4px;
  }

  .part {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 16px;
    padding: 7px 0;
    font-size: 12px;
    color: var(--phx-text-2);
    border-top: 0.5px solid var(--phx-sep);
  }

  .dot {
    width: 20px;
    height: 20px;
    border-radius: 5px;
    background: var(--phx-fill-tertiary);
    color: var(--phx-text-3);
    font-size: 10px;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }

  /* ===== 装备预设卡 ===== */
  .ecard {
    margin-top: 16px;
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow: hidden;
  }

  .ehead {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
  }

  .ehead:hover {
    background: var(--phx-fill-tertiary);
  }

  .etoggle {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    border: none;
    background: transparent;
    color: inherit;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    padding: 0;
  }

  .ecaret {
    display: flex;
    align-items: center;
    color: var(--phx-text-3);
    transition: transform 0.15s;
    flex: none;
  }

  .ecaret :global(svg) {
    width: 14px;
    height: 14px;
  }

  .ecard.open .ecaret {
    transform: rotate(90deg);
  }

  .ename {
    font-size: 13.5px;
    font-weight: 700;
  }

  .ebtns {
    margin-left: auto;
    display: flex;
    gap: 6px;
  }

  .ebody {
    display: none;
    border-top: 0.5px solid var(--phx-sep);
    padding-bottom: 6px;
  }

  .ecard.open .ebody {
    display: block;
  }

  .srow {
    display: grid;
    grid-template-columns: 92px 30px minmax(0, 1.1fr) 18px minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
    padding: 9px 14px;
  }

  .srow + .srow {
    border-top: 0.5px solid var(--phx-sep);
  }

  .slotname {
    font-size: 12px;
    font-weight: 600;
    color: var(--phx-text-2);
  }

  .pwrap {
    grid-column: 1 / -1;
    padding: 0 0 0 14px;
  }

  .pitem {
    display: grid;
    grid-template-columns: 26px minmax(0, 1fr) minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
    padding: 7px 0 7px 26px;
  }

  .pitem + .pitem {
    border-top: 0.5px solid var(--phx-sep);
  }

  .pempty {
    grid-column: 1 / -1;
    font-size: 12px;
    color: var(--phx-text-3);
  }

  .abtn {
    width: 26px;
    height: 26px;
    border: none;
    border-radius: 7px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    font-family: inherit;
    padding: 0;
  }

  .abtn:hover {
    filter: brightness(1.2);
  }

  .abtn :global(svg) {
    width: 13px;
    height: 13px;
  }

  /* ===== 套装军械库（稿图 32） ===== */
  .arsenal {
    display: flex; gap: 16px; flex: 1; min-height: 0;
    /* 左右 24px 页边距（对齐全站 .scroll/页头口径）；dock 避让 104px 同 +layout 注释 */
    padding: 0 24px; margin-bottom: 104px;
  }
  .ar-wall {
    width: 46%; flex: none; overflow-y: auto; scrollbar-width: none;
    display: grid; grid-template-columns: 1fr 1fr; gap: 12px; align-content: start;
    padding-bottom: 12px;
  }
  .ar-wall::-webkit-scrollbar { display: none; }
  .ar-set {
    display: flex; flex-direction: column; align-items: stretch;
    background: var(--phx-surface); border-radius: var(--phx-r-card); overflow: hidden;
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong); cursor: pointer; border: none;
    padding: 0; height: auto; font-family: inherit; text-align: left; color: var(--phx-text);
  }
  .ar-set.on { box-shadow: inset 0 0 0 1.5px var(--phx-accent); }
  .ar-thumb {
    height: 84px; flex: none; position: relative; overflow: hidden;
    display: flex; align-items: center; justify-content: center;
    background: linear-gradient(135deg, #16233c, #101013);
  }
  .ar-thumb svg { width: 104px; height: 52px; display: block; }
  .ar-set.on .ar-thumb { background: linear-gradient(135deg, #1d3050, #101013); }
  .ar-thumb .l { fill: rgba(0,145,255,.16); stroke: rgba(0,145,255,.6); stroke-width: 2; }
  .ar-thumb .l2 { fill: none; stroke: rgba(0,145,255,.4); stroke-width: 1.4; }
  .ar-tier {
    position: absolute; top: 8px; left: 8px; font-size: 10.5px; font-weight: 700;
    padding: 2px 8px; border-radius: 5px; background: rgba(0,0,0,.45); color: var(--phx-text-2);
  }
  .ar-body { padding: 9px 12px 11px; flex: none; }
  .ar-n { font-size: 12.5px; font-weight: 700; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ar-price { font-size: 10.5px; color: var(--phx-warn); margin-top: 3px; font-family: var(--phx-font-mono); }
  .ar-wall { align-content: start; grid-auto-rows: min-content; }
  .ar-detail {
    flex: 1; min-width: 0; background: var(--phx-surface); border-radius: var(--phx-r-card);
    display: flex; flex-direction: column; overflow: hidden; box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
  }
  .ar-hero {
    height: 140px; flex: none; display: flex; align-items: center; justify-content: center;
    background: linear-gradient(135deg, #16233c 0%, #101013 70%);
  }
  .ar-hero svg { width: 300px; height: 120px; }
  .ar-hero .l { fill: rgba(0,145,255,.16); stroke: rgba(0,145,255,.62); stroke-width: 2.2; }
  .ar-hero .l2 { fill: none; stroke: rgba(0,145,255,.42); stroke-width: 1.5; }
  .ar-h { padding: 14px 20px 8px; flex: none; }
  .ar-t { font-size: 18px; font-weight: 700; }
  .ar-sub { font-size: 12px; color: var(--phx-text-2); margin-top: 4px; line-height: 1.6; }
  .ar-list { flex: 1; min-height: 0; overflow-y: auto; padding: 0 20px 8px; scrollbar-width: none; }
  .ar-list::-webkit-scrollbar { display: none; }
  .ar-cat { font-size: 11px; font-weight: 700; letter-spacing: 0.06em; color: var(--phx-text-3); margin: 12px 0 6px; }
  .ar-item {
    display: flex; align-items: center; gap: 10px; padding: 7px 10px; border-radius: 9px;
    background: var(--phx-surface-2); margin-top: 5px; font-size: 12.5px;
  }
  .ar-slot { font-size: 10.5px; font-weight: 700; width: 48px; flex: none; text-align: center; padding: 2px 0; border-radius: 5px; }
  .ar-slot.w { background: var(--phx-accent-dim); color: var(--phx-accent); }
  .ar-slot.g { background: var(--phx-green-dim); color: var(--phx-green); }
  .ar-in { font-weight: 600; }
  .ar-miss { color: var(--phx-text-3); }
  .ar-f { flex: none; padding: 12px 20px 16px; border-top: 0.5px solid var(--phx-sep); }
  .ar-price-line { display: flex; align-items: baseline; gap: 8px; margin-bottom: 10px; }
  .ar-num { font-family: var(--phx-font-mono); font-size: 17px; font-weight: 700; color: var(--phx-warn); }
  .ar-cap { font-size: 11px; color: var(--phx-text-3); }
  .ar-btns { display: flex; gap: 10px; }
  .ar-safe { margin-top: 10px; font-size: 11px; color: var(--phx-text-3); }
</style>