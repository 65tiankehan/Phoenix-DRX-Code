<script lang="ts">
  // 10 检视页：按 docs/prototypes/10 稿图逐项落地。
  // 双页签：检视（档案内武器表 + 改装详情弹窗）/ 图鉴（服务器官方改装预设，只读）。
  // 检视入口：仓库页武器行「检视」钮 goto(/inspect?id=)，onMount 读参数自动开弹窗。
  // 零件口径：Rust inspect_weapon 聚合（弹匣内弹药不计入）；加入预设写入用户预设（写前自动备份）。
  import { onMount } from 'svelte'
  import { page } from '$app/state'
  import { listen } from '@tauri-apps/api/event'
  import { save } from '@tauri-apps/plugin-dialog'
  import { t } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import {
    phxProfile,
    phxStash,
    phxInspect,
    type PhxBuildView,
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
    trash: '<path d="M4 7h16M9 7V5h6v2m-8 0 1 13h8l1-13"/>',
    close: '<path d="m3 3 14 14M17 3 3 17"/>',
    caret: '<path d="m9 5 7 7-7 7"/>',
    out: '<path d="M12 15V4m0 0 4 4m-4-4-4 4M5 20h14"/>'
  }

  type Seg = 'view' | 'lib'

  let currentId = $state('')
  let weapons = $state<PhxStashItemRow[]>([])
  let builds = $state<PhxBuildView[]>([])
  let loaded = $state(false)
  /** 首次加载中：骨架屏缓冲；数据到位或确认无档案后关闭 */
  let booting = $state(true)
  let saving = $state(false)
  let seg = $state<Seg>('view')
  let fName = $state('')
  /** 改装详情弹窗（检视） */
  let sheetOpen = $state(false)
  let sheetItem = $state<PhxStashItemRow | null>(null)
  let view = $state<PhxBuildView | null>(null)
  /** 移除确认弹层 */
  let dlgOpen = $state(false)

  /** 图鉴按名称筛选（诚实空态：有预设但无匹配单独提示） */
  const visBuilds = $derived.by(() => {
    const q = fName.trim().toLowerCase()
    if (!q) return builds
    return builds.filter((b) => b.name.toLowerCase().includes(q))
  })

  const gsubText = (b: PhxBuildView): string =>
    t(b.hasModded ? 'inspect.gsub.modded' : 'inspect.gsub', { n: b.parts.length })

  /** 即时保存：op 列表交 Rust 逐条执行；reload = 成功后重读对账 */
  async function runOps(ops: PhxStashOp[], reload: boolean): Promise<void> {
    if (!currentId || !ops.length) return
    const id = currentId
    saving = true
    try {
      await phxStash.save(id, { ops })
      playSound('success')
      toast.success(t('inspect.toast.saved'))
      if (reload) await loadAll(id)
    } catch (e) {
      toast.error(t('inspect.toast.failed'))
      // 放置失败等错误信息透出
      if (typeof e === 'string' && e) toast.error(e)
      if (reload) await loadAll(id)
    } finally {
      saving = false
    }
  }

  /** 打开改装详情弹窗（行内「检视」与仓库页直达共用） */
  async function openView(it: PhxStashItemRow): Promise<void> {
    if (!currentId) return
    const v = await phxInspect.inspect(currentId, it.id)
    if (!v) {
      toast.error(t('inspect.toast.notfound'))
      return
    }
    playSound('pop')
    sheetItem = it
    view = v
    sheetOpen = true
  }

  /** 当前武器存为用户预设（弹窗保持打开；本页不展示用户预设，无需刷新） */
  async function addPreset(): Promise<void> {
    if (!currentId || !sheetItem) return
    saving = true
    try {
      await phxInspect.presetAdd(currentId, sheetItem.id)
      playSound('gain')
      toast.success(t('inspect.toast.saved'))
    } catch (e) {
      toast.error(t('inspect.toast.failed'))
      if (typeof e === 'string' && e) toast.error(e)
    } finally {
      saving = false
    }
  }

  /** 图鉴预设加入仓库：走仓库放置引擎，成功后重读对账 */
  function addBuild(b: PhxBuildView): void {
    playSound('gain')
    void runOps([{ op: 'addBuild', presetId: b.id }], true)
  }

  /** 文件名净化：替换非法字符 */
  const safeName = (name: string): string => `${name.replace(/[\\/:*?"<>|]/g, '_')}.json`

  /** 导出弹窗内当前武器（取消 = 静默返回） */
  async function exportSheet(): Promise<void> {
    if (!currentId || !sheetItem || !view) return
    const path = await save({
      defaultPath: safeName(view.name),
      filters: [{ name: 'JSON', extensions: ['json'] }]
    })
    if (!path) return
    try {
      await phxInspect.exportWeapon(currentId, sheetItem.id, path)
      toast.success(t('inspect.toast.exported', { name: view.name }))
    } catch (e) {
      toast.error(t('inspect.toast.failed'))
      if (typeof e === 'string' && e) toast.error(e)
    }
  }

  /** 导出图鉴预设 */
  async function exportBuild(b: PhxBuildView): Promise<void> {
    const path = await save({
      defaultPath: safeName(b.name),
      filters: [{ name: 'JSON', extensions: ['json'] }]
    })
    if (!path) return
    try {
      await phxInspect.exportPreset(path, b.id)
      toast.success(t('inspect.toast.exported', { name: b.name }))
    } catch (e) {
      toast.error(t('inspect.toast.failed'))
      if (typeof e === 'string' && e) toast.error(e)
    }
  }

  function closeSheet(): void {
    playSound('ui')
    sheetOpen = false
  }

  /** 移除弹窗内武器：先确认（复用仓库页移除文案），成功后关弹窗并重读 */
  function askRemove(): void {
    playSound('alert')
    dlgOpen = true
  }

  function confirmDlg(): void {
    if (!currentId || !sheetItem) return
    const wid = sheetItem.id
    sheetOpen = false
    view = null
    sheetItem = null
    weapons = weapons.filter((w) => w.id !== wid)
    void runOps([{ op: 'removeItems', ids: [wid] }], true)
  }

  /** 全量刷新：当前档案 → 检视页数据（图鉴独立加载，不依赖档案） */
  async function loadAll(id: string): Promise<void> {
    const data = await phxStash.load(id)
    if (!data) {
      weapons = []
      loaded = false
      return
    }
    weapons = data.items.filter((i) => i.isWeapon)
    loaded = true
  }

  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    currentId = id ?? ''
    loaded = false
    booting = true
    seg = 'view'
    fName = ''
    sheetOpen = false
    sheetItem = null
    view = null
    try {
      builds = await phxInspect.loadGlobalBuilds()
    } catch {
      builds = []
    }
    if (!currentId) {
      weapons = []
      booting = false
      return
    }
    await loadAll(currentId)
    booting = false
  }

  onMount(() => {
    void refresh().then(() => {
      // 仓库页「检视」直达：URL 带 id 参数时自动打开详情弹窗
      const direct = page.url.searchParams.get('id')
      if (direct) {
        const it = weapons.find((w) => w.id === direct)
        if (it) void openView(it)
      }
    })
    // 托盘「切换存档」后刷新检视页（Rust 侧 emit）
    const off = listen('tray:profile-switched', () => void refresh())
    return () => {
      void off.then((unlisten) => unlisten())
    }
  })
</script>

<div class="page">
  <div class="main-head">
    {#if seg === 'view'}
      <div>
        <div class="large-title">{t('inspect.title')}</div>
        <div class="head-meta">{t('inspect.meta.view')}</div>
      </div>
    {:else}
      <div>
        <div class="large-title">{t('inspect.libTitle')}</div>
        <div class="head-meta">{t('inspect.meta.lib')}</div>
      </div>
      <div class="head-actions">
        <span class="gcount">{t('inspect.libCount', { n: visBuilds.length })}</span>
      </div>
    {/if}
  </div>

  <div class="body">
    <!-- 骨架层：镜像工具行 + 武器列表真实布局；数据就绪加 .gone 淡出，与新内容交叉淡化（感官层基线 §二） -->
    <div class="boot phx-boot" class:gone={!booting}>
      <div class="toolbar">
        <i class="phx-sk seg-sk"></i>
        <i class="phx-sk seg-sk"></i>
        <div class="fspacer"></div>
      </div>
      <div class="scroll">
        <div class="qcard">
          {#each skRows(6) as r (r)}
            <div class="mrow">
              <i class="phx-sk av-sk"></i>
              <i class="phx-sk" style="width:62%"></i>
              <span></span>
              <i class="phx-sk" style="width:70%"></i>
              <div class="acts">
                <i class="phx-sk abtn-sk"></i>
              </div>
            </div>
          {/each}
        </div>
      </div>
    </div>
  {#if !booting}
    <div class="toolbar phx-fade-in">
      <PhxSegmented
        bind:value={seg}
        ariaLabel={t('inspect.seg.aria')}
        options={[
          { value: 'view', label: t('inspect.seg.view') },
          { value: 'lib', label: t('inspect.seg.lib') }
        ]}
      />
      <div class="fspacer"></div>
      {#if seg === 'lib'}
        <div class="sfield">
          <span class="sico"><PhxIcon markup={ICON.search} /></span>
          <input bind:value={fName} placeholder={t('inspect.f.name')} />
        </div>
      {/if}
    </div>

    <div class="scroll phx-fade-in">
      {#if seg === 'view'}
        {#if !loaded}
          <div class="empty">{t('stash.empty.noprofile')}</div>
        {:else if !weapons.length}
          <div class="empty">{t('inspect.empty.view')}</div>
        {:else}
          <div class="qcard">
            {#each weapons as it (it.id)}
              <div class="mrow">
                <div class="icon-b">{it.name.charAt(0)}</div>
                <div class="iname">{it.name}{#if it.count > 1}&nbsp;×{it.count}{/if}</div>
                <div class="mmod" class:show={it.modded}><PhxIcon markup={ICON.warn} /></div>
                <div class="iid">{it.id}</div>
                <div class="acts">
                  <PhxTooltip label={t('inspect.aria.view')}>
                    <button
                      class="abtn"
                      type="button"
                      aria-label={t('inspect.aria.view')}
                      onclick={() => openView(it)}
                    >
                      <PhxIcon markup={ICON.search} />
                    </button>
                  </PhxTooltip>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      {:else}
        {#if !builds.length}
          <div class="empty">{t('inspect.empty.lib')}</div>
        {:else if !visBuilds.length}
          <div class="empty">{t('inspect.empty.libMatch')}</div>
        {:else}
          {#each visBuilds as b (b.id)}
            <div class="gcard">
              <div class="gcard-head">
                <div class="icon-b">{b.name.charAt(0)}</div>
                <div>
                  <div class="gname">{b.name}</div>
                  <div class="gsub">{gsubText(b)}</div>
                </div>
                <div class="mmod" class:show={b.hasModded}><PhxIcon markup={ICON.warn} /></div>
                <div class="gacts">
                  <button class="btn" type="button" disabled={!currentId || saving} onclick={() => addBuild(b)}>
                    <PhxIcon markup={ICON.plus} /> {t('inspect.g.add')}
                  </button>
                  <button class="btn ghost" type="button" onclick={() => exportBuild(b)}>
                    <PhxIcon markup={ICON.out} /> {t('inspect.g.export')}
                  </button>
                </div>
              </div>
              <div class="gstats">
                <div class="gchip">
                  <span class="k">{t('inspect.stats.ergo')}</span><span class="v">{b.ergonomics}</span>
                </div>
                <div class="gchip">
                  <span class="k">{t('inspect.stats.recoilV')}</span><span class="v">{b.recoilUp}</span>
                </div>
                <div class="gchip">
                  <span class="k">{t('inspect.stats.recoilH')}</span><span class="v">{b.recoilBack}</span>
                </div>
              </div>
              <div class="gparts">
                {#each b.parts as p (p.id)}
                  <div class="pchip">
                    <div class="icon-b chipico">{p.name.charAt(0)}</div>
                    {p.name}
                  </div>
                {/each}
              </div>
            </div>
          {/each}
        {/if}
      {/if}
    </div>
  {/if}
  </div>
</div>

{#if sheetOpen && view && sheetItem}
  <div class="mask">
    <div class="sheet" role="dialog" aria-modal="true" aria-label={view.name}>
      <div class="sheet-head">
        <div class="sheet-title">{view.name}</div>
        <div class="mmod" class:show={view.hasModded}><PhxIcon markup={ICON.warn} /></div>
        <PhxTooltip label={t('inspect.sheet.close')}>
          <button class="sheet-x" type="button" aria-label={t('inspect.sheet.close')} onclick={closeSheet}>
            <PhxIcon markup={ICON.close} />
          </button>
        </PhxTooltip>
      </div>
      <div class="sheet-body">
        <div class="hero">
          <div class="icon-b heroico">{view.rootName.charAt(0)}</div>
          <div>
            <div class="hname">{view.rootName}</div>
            <div class="hcal">{t('inspect.hero.sub')}</div>
          </div>
        </div>
        <div class="wbuild">
          <div class="panel stats">
            <div class="stat"><span class="k">{t('inspect.stats.ergo')}</span><span class="v">{view.ergonomics}</span></div>
            <div class="stat"><span class="k">{t('inspect.stats.recoilV')}</span><span class="v">{view.recoilUp}</span></div>
            <div class="stat"><span class="k">{t('inspect.stats.recoilH')}</span><span class="v">{view.recoilBack}</span></div>
          </div>
          <div class="panel parts">
            <div class="phead open">
              <span class="caret"><PhxIcon markup={ICON.caret} /></span>
              <span class="pt">{t('inspect.parts')}</span>
              <span class="gcount">{view.parts.length}</span>
            </div>
            {#each view.parts as p (p.id)}
              <div class="prow">
                <div class="icon-b small">{p.name.charAt(0)}</div>
                <div class="pname">{p.name}</div>
                <div class="pslot">{p.slotId}</div>
              </div>
            {/each}
            <div class="pnote">{t('inspect.pnote')}</div>
          </div>
        </div>
      </div>
      <div class="sheet-foot">
        <button class="btn" type="button" disabled={saving} onclick={addPreset}>
          <PhxIcon markup={ICON.plus} /> {t('inspect.addPreset')}
        </button>
        <button class="btn ghost" type="button" onclick={exportSheet}>
          <PhxIcon markup={ICON.out} /> {t('inspect.g.export')}
        </button>
        <button class="btn danger-ghost" type="button" disabled={saving} onclick={askRemove}>
          <PhxIcon markup={ICON.trash} /> {t('inspect.remove')}
        </button>
      </div>
    </div>
  </div>
{/if}

<PhxDialog
  bind:open={dlgOpen}
  heading={t('stash.dlg.rm.title')}
  text={t('stash.dlg.rm.text', { name: sheetItem?.name ?? '' })}
  danger
  confirmText={t('stash.dlg.ok')}
  onconfirm={confirmDlg}
/>

<style>
  /* ---- 公共（与 stash / quests / skills / hideout 页同源） ---- */
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
  /* 稿图 .btn 带 svg（加入预设 / 导出 / 加入仓库）；stash 同类按钮无 svg 故生产底本未含 */
  .btn {
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

  /* ---- 加载骨架屏缓冲 ---- */
  /* 加载态包装层：boot 覆盖层 + 内容同层交叉淡化 */
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .boot {
    margin: 16px 24px 0;
  }

  .boot .seg-sk {
    width: 64px;
    height: 30px;
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

  /* ---- 工具行：子页分段 + 图鉴筛选框 ---- */
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

  /* ---- 计数徽标（图鉴头 + 零件表头共用，与 stash .gcount 同源） ---- */
  .gcount {
    font-family: var(--phx-font-mono);
    font-size: 11px;
    color: var(--phx-text-3);
    background: var(--phx-fill-tertiary);
    border-radius: 999px;
    padding: 1px 8px;
  }

  /* ---- 检视武器表 ---- */
  .qcard {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow: hidden;
  }
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
  .iname {
    font-size: 12.5px;
    color: var(--phx-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
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
  .abtn:hover {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
  }
  /* ---- 检视弹窗（sheet）与图鉴卡片（gcard）专属样式 ---- */
  .mask {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    z-index: var(--phx-z-modal);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .sheet {
    width: 620px;
    max-height: 660px;
    max-width: calc(100vw - 48px);
    background: var(--phx-surface);
    box-shadow:
      inset 0 0 0 0.5px var(--phx-sep-strong),
      0 24px 60px rgba(0, 0, 0, 0.55);
    border-radius: 14px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .sheet-head {
    padding: 16px 18px 12px;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .sheet-title {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.1px;
  }
  .sheet-x {
    margin-left: auto;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    cursor: pointer;
    background: transparent;
    border: none;
    color: var(--phx-text-2);
    display: flex;
    align-items: center;
    justify-content: center;
    transition:
      background 0.12s,
      color 0.12s;
  }
  .sheet-x:hover {
    background: var(--phx-fill-quaternary);
    color: var(--phx-text);
  }
  .sheet-x :global(svg) {
    width: 12px;
    height: 12px;
  }
  .sheet-body {
    padding: 2px 18px 4px;
    overflow-y: auto;
    min-height: 0;
  }
  .sheet-foot {
    padding: 12px 18px 16px;
    display: flex;
    gap: 8px;
    border-top: 0.5px solid var(--phx-sep);
  }
  .hero {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 0 12px;
  }
  .heroico {
    width: 36px;
    height: 36px;
    border-radius: 9px;
  }
  .hname {
    font-size: 14.5px;
    font-weight: 700;
  }
  .hcal {
    font-size: 11.5px;
    color: var(--phx-text-3);
    margin-top: 2px;
  }
  .wbuild {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.05fr);
    gap: 14px;
  }
  .panel {
    display: flex;
    flex-direction: column;
    background: var(--phx-surface-2);
    border-radius: 10px;
    box-shadow: inset 0 0 0 0.5px var(--phx-sep);
    padding: 4px 12px;
  }
  .stat {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 9px 2px;
  }
  .stat + .stat {
    border-top: 0.5px solid var(--phx-sep);
  }
  .stat .k {
    font-size: 12px;
    color: var(--phx-text-2);
  }
  .stat .v {
    font-family: var(--phx-mono);
    font-size: 13px;
    color: var(--phx-text);
  }
  .phead {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 9px 2px;
  }
  .pt {
    font-size: 12px;
    font-weight: 600;
    color: var(--phx-text-2);
  }
  .caret {
    color: var(--phx-text-3);
    display: flex;
    align-items: center;
  }
  .caret :global(svg) {
    width: 10px;
    height: 10px;
    transition: transform 0.15s;
  }
  .phead.open .caret :global(svg) {
    transform: rotate(90deg);
  }
  .prow {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 2px;
  }
  .prow + .prow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .pname {
    font-size: 12.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pslot {
    margin-left: auto;
    font-family: var(--phx-mono);
    font-size: 11px;
    color: var(--phx-text-3);
    flex: none;
  }
  .pnote {
    font-size: 11px;
    color: var(--phx-text-3);
    padding: 9px 2px 2px;
  }
  .gcard {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    margin-top: 12px;
    overflow: hidden;
  }
  .gcard-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
  }
  .gcard-head .icon-b {
    width: 32px;
    height: 32px;
    border-radius: 8px;
  }
  .gname {
    font-size: 13.5px;
    font-weight: 700;
  }
  .gsub {
    font-size: 11.5px;
    color: var(--phx-text-3);
    margin-top: 2px;
  }
  .gacts {
    margin-left: auto;
    display: flex;
    gap: 8px;
  }
  .gstats {
    display: flex;
    gap: 6px;
    padding: 0 14px 12px;
  }
  .gchip {
    height: 26px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    background: var(--phx-surface-2);
    border-radius: 7px;
    box-shadow: inset 0 0 0 0.5px var(--phx-sep);
  }
  .gchip .k {
    font-size: 11px;
    color: var(--phx-text-2);
  }
  .gchip .v {
    font-family: var(--phx-mono);
    font-size: 11.5px;
  }
  .gparts {
    border-top: 0.5px solid var(--phx-sep);
    padding: 6px 14px 10px;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .pchip {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px 0 4px;
    background: var(--phx-surface-2);
    border-radius: 8px;
    box-shadow: inset 0 0 0 0.5px var(--phx-sep);
    font-size: 11.5px;
    color: var(--phx-text-2);
  }
  .chipico {
    width: 22px;
    height: 22px;
    border-radius: 5px;
  }
</style>
