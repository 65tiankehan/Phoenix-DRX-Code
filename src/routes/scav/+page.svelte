<script lang="ts">
  // 11 Scav 页：按 docs/prototypes/11-scav.html 稿图逐项落地。
  // 两子页签：Scav 装备（12 槽 + 口袋内嵌行，与 09 装备子页同构）/ Scav 口袋（增强子页：平铺表 + 单框过滤）。
  // 数据源 characters.scav（PMC 数据不动）；保存模式即时保存（仅 RemoveItems，写入前 Rust 侧自动备份）。
  // 口袋为装备特殊槽（SlotId=Pockets）：装备子页内嵌行与口袋子页展示同一份数据，删除双向联动。
  import { onMount } from 'svelte'
  import { goto } from '$app/navigation'
  import { resolve } from '$app/paths'
  import { listen } from '@tauri-apps/api/event'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxProfile, phxScav, type PhxStashItemRow, type PhxScavOp } from '$lib/ipc'
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
    trash: '<path d="M4 7h16M9 7V5h6v2m-8 0 1 13h8l1-13"/>'
  }

  type Seg = 'equip' | 'pocket'

  let currentId = $state('')
  let equipment = $state<{ slot: string; item: PhxStashItemRow | null }[]>([])
  let pockets = $state<PhxStashItemRow[]>([])
  let pocketsName = $state('')
  let loaded = $state(false)
  /** 首次加载中：骨架屏缓冲；数据到位或确认无档案后关闭 */
  let booting = $state(true)
  let saving = $state(false)
  let seg = $state<Seg>('equip')
  /** 口袋子页单框实时过滤（稿图口径：仅口袋子页有过滤框） */
  let fName = $state('')
  /** 确认弹层（删单件 / 清空装备共用） */
  let dlgOpen = $state(false)
  let dlgKind = $state<'one' | 'equip'>('one')
  let dlgId = $state('')

  const visPockets = $derived.by(() => {
    const q = fName.trim().toLowerCase()
    return q ? pockets.filter((p) => p.name.toLowerCase().includes(q)) : pockets
  })

  const metaText = $derived(loaded ? t('scav.meta.loaded') : t('stash.meta.none'))

  /** 装备槽名 i18n：stash.slot.{slotId}，意外槽位原样显示 slotId（与 PMC 装备同款映射） */
  const slotName = (slot: string): string => t(`stash.slot.${slot}` as MessageKey)

  /** 即时保存：op 列表交 Rust 逐条执行；reload = 成功后重读对账 */
  async function runOps(ops: PhxScavOp[], reload: boolean): Promise<void> {
    if (!currentId || !ops.length) return
    const id = currentId
    saving = true
    try {
      await phxScav.save(id, { ops })
      playSound('success')
      toast.success(t('scav.toast.saved'))
      if (reload) await loadAll(id)
    } catch (e) {
      toast.error(t('stash.toast.failed'))
      if (typeof e === 'string' && e) toast.error(e)
      if (reload) await loadAll(id)
    } finally {
      saving = false
    }
  }

  function askRemove(id: string): void {
    playSound('alert')
    dlgKind = 'one'
    dlgId = id
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
      // 装备槽与口袋同一份物品 ID 空间：两处状态同步移除（双向联动）
      equipment = equipment.map((s) => (s.item?.id === dlgId ? { ...s, item: null } : s))
      pockets = pockets.filter((p) => p.id !== dlgId)
      void runOps([{ op: 'removeItems', ids: [dlgId] }], true)
    } else {
      // 清空装备与 PMC 口径一致：装备槽全部物品 + 口袋清空
      const ids = [
        ...equipment.filter((s) => s.item).map((s) => s.item!.id),
        ...pockets.map((p) => p.id)
      ]
      equipment = equipment.map((s) => ({ ...s, item: null }))
      pockets = []
      void runOps([{ op: 'removeItems', ids }], true)
    }
  }

  /** 全量刷新：当前档案 → Scav 页数据（仅 characters.scav） */
  async function loadAll(id: string): Promise<void> {
    const data = await phxScav.load(id)
    if (!data) {
      equipment = []
      pockets = []
      pocketsName = ''
      loaded = false
      return
    }
    equipment = data.equipment
    pockets = data.pockets
    pocketsName = data.pocketsName
    loaded = true
  }

  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    currentId = id ?? ''
    loaded = false
    booting = true
    seg = 'equip'
    fName = ''
    if (!currentId) {
      equipment = []
      pockets = []
      pocketsName = ''
      booting = false
      return
    }
    await loadAll(currentId)
    booting = false
  }

  onMount(() => {
    void refresh()
    // 托盘「切换存档」后刷新 Scav 页（Rust 侧 emit）
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
      {#if seg === 'equip'}
        <button
          class="btn danger-ghost"
          type="button"
          onclick={askClearEquip}
          disabled={saving || (!equipment.some((s) => s.item) && !pockets.length)}
        >
          {t('scav.act.clearEquip')}
        </button>
      {/if}
    </div>
  </header>

  <div class="body">
    <!-- 骨架层：镜像工具行 + 装备槽/口袋真实布局；数据就绪加 .gone 淡出，与新内容交叉淡化（感官层基线 §二） -->
    <div class="boot phx-boot" class:gone={!booting}>
      <div class="toolbar">
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <span class="fspacer"></span>
      </div>
      <div class="scroll">
        <section class="qgroup">
          <div class="ghead">
            <i class="phx-sk slim" style="width:64px"></i>
            <i class="phx-sk gc-sk"></i>
          </div>
          <div class="qcard">
            {#each skRows(6) as r (r)}
              <div class="srow">
                <i class="phx-sk" style="width:76px;height:12px"></i>
                <i class="phx-sk av-sk"></i>
                <i class="phx-sk" style="width:64%"></i>
                <span></span>
                <i class="phx-sk" style="width:72%"></i>
                <div class="acts">
                  <i class="phx-sk abtn-sk"></i>
                </div>
              </div>
            {/each}
            <div class="pwrap">
              <div class="phead"><i class="phx-sk slim" style="width:52px;height:12px"></i></div>
              {#each skRows(2) as r (r)}
                <div class="pitem">
                  <i class="phx-sk avsm-sk"></i>
                  <i class="phx-sk" style="width:58%"></i>
                  <i class="phx-sk" style="width:66%"></i>
                  <i class="phx-sk abtn-sk"></i>
                </div>
              {/each}
            </div>
          </div>
          <div class="tfoot">
            <i class="phx-sk slim" style="width:120px"></i>
          </div>
        </section>
      </div>
    </div>
  {#if loaded}
    <div class="toolbar phx-fade-in">
      <PhxSegmented
        bind:value={seg}
        ariaLabel={t('scav.seg.aria')}
        options={[
          { value: 'equip', label: t('scav.seg.equip') },
          { value: 'pocket', label: t('scav.seg.pocket') }
        ]}
      />
      <span class="fspacer"></span>
      {#if seg === 'pocket'}
        <div class="sfield">
          <span class="sico"><PhxIcon markup={ICON.search} /></span>
          <input type="text" placeholder={t('stash.search.name')} bind:value={fName} />
        </div>
      {/if}
    </div>

    <div class="scroll phx-fade-in">
      {#if seg === 'equip'}
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
                    {#if s.item.isWeapon}
                      <!-- 检视入口（10 屏弹窗已落地）：武器行跳转检视页并自动开弹窗 -->
                      <PhxTooltip label={t('stash.aria.inspect')}>
                        <button
                          class="abtn"
                          type="button"
                          aria-label={t('stash.aria.inspect')}
                          onclick={() => {
                            playSound('pop')
                            goto(resolve(`/inspect?id=${s.item!.id}`))
                          }}
                        >
                          <PhxIcon markup={ICON.search} />
                        </button>
                      </PhxTooltip>
                    {/if}
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
            <!-- 口袋：装备的特殊槽（SlotId=Pockets），内嵌行与「Scav 口袋」子页同一份数据 -->
            <div class="pwrap">
              <div class="phead">{t('stash.slot.Pockets')}</div>
              {#each pockets as p (p.id)}
                <div class="pitem">
                  <span class="icon-b small">{p.name.charAt(0)}</span>
                  <div class="iname">{p.name}{p.count > 1 ? ` ×${p.count}` : ''}</div>
                  <span class="iid">{p.tpl}</span>
                  <PhxTooltip label={t('scav.aria.delPocket')}>
                    <button class="abtn del" type="button" aria-label={t('scav.aria.delPocket')} onclick={() => askRemove(p.id)}>
                      <PhxIcon markup={ICON.trash} />
                    </button>
                  </PhxTooltip>
                </div>
              {/each}
              {#if !pockets.length}<div class="pempty">{t('scav.pocket.empty')}</div>{/if}
            </div>
          </div>
          <div class="tfoot">{t('scav.foot.equip', { n: equipment.length })}</div>
        </section>
      {:else}
        <!-- 口袋变体只读展示卡（变体切换归「角色」页，Scav 侧只读） -->
        <div class="vcard">
          <span class="icon-b">{pocketsName.charAt(0)}</span>
          <div>
            <div class="vlabel">{t('scav.v.label')}</div>
            <div class="vname">{pocketsName}</div>
          </div>
          <div class="vnote">{t('scav.v.note')}</div>
        </div>
        <section class="qgroup">
          <div class="ghead">
            <span class="gtitle">{t('scav.g.pocket')}</span>
            <span class="gcount">{visPockets.length}</span>
          </div>
          <div class="qcard">
            {#each visPockets as p (p.id)}
              <div class="mrow">
                <span class="icon-b">{p.name.charAt(0)}</span>
                <div class="iname">{p.name}{p.count > 1 ? ` ×${p.count}` : ''}</div>
                <span class="mmod" class:show={p.modded}><PhxIcon markup={ICON.warn} /></span>
                <span class="iid">{p.tpl}</span>
                <div class="acts">
                  <PhxTooltip label={t('scav.aria.delPocket')}>
                    <button class="abtn del" type="button" aria-label={t('scav.aria.delPocket')} onclick={() => askRemove(p.id)}>
                      <PhxIcon markup={ICON.trash} />
                    </button>
                  </PhxTooltip>
                </div>
              </div>
            {/each}
            {#if !visPockets.length}<div class="empty">{t('stash.empty.none')}</div>{/if}
          </div>
          {#if visPockets.length}
            <div class="tfoot">{t('scav.foot.pocket', { n: visPockets.length, m: pockets.length })}</div>
          {/if}
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
  heading={dlgKind === 'one' ? t('scav.dlg.rm.title') : t('scav.dlg.equip.title')}
  text={dlgKind === 'one'
    ? t('scav.dlg.rm.text', {
        name: [...equipment.flatMap((s) => (s.item ? [s.item] : [])), ...pockets].find(
          (i) => i.id === dlgId
        )?.name ?? ''
      })
    : t('scav.dlg.equip.text')}
  danger
  confirmText={t('stash.dlg.ok')}
  onconfirm={confirmDlg}
/>

<style>
  /* ---- 页面骨架与头部（与 stash 页同构） ---- */
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

  .boot .avsm-sk {
    width: 26px;
    height: 26px;
    border-radius: 6px;
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

  /* ---- 工具行：子页分段 + 过滤框 ---- */
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

  /* ---- 分组 / 卡片 ---- */
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
  .tfoot {
    padding: 12px 14px;
    text-align: center;
    font-size: 11px;
    color: var(--phx-text-3);
  }

  /* ---- 口袋表行（口袋子页平铺表） ---- */
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

  /* ---- 装备槽行 + 内嵌口袋 ---- */
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

  /* ---- 口袋变体卡（口袋子页顶部） ---- */
  .vcard {
    margin-top: 16px;
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    padding: 12px 16px;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .vcard .vlabel {
    font-size: 12px;
    color: var(--phx-text-2);
  }
  .vcard .vname {
    font-size: 13px;
    font-weight: 600;
  }
  .vcard .vnote {
    margin-left: auto;
    font-size: 11.5px;
    color: var(--phx-text-3);
  }
</style>
