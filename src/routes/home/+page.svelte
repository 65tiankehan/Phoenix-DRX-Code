<script lang="ts">
  // 02 首页：hero 档案卡 + 2×2 仪表台 17 入口，按 docs/prototypes/02 稿图逐项落地
  import { onMount } from 'svelte'
  import { goto } from '$app/navigation'
  import { resolve } from '$app/paths'
  import { listen } from '@tauri-apps/api/event'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxProfile, type PhxProfileHero, type PhxProfileMeta } from '$lib/ipc'
  import PhxDropdown from '$lib/components/PhxDropdown.svelte'
  import PhxIcon from '$lib/components/PhxIcon.svelte'

  /** 入口图标：稿图同款 svg inner markup（wrapper 统一由模板提供） */
  const ICON = {
    info: '<circle cx="12" cy="8" r="3.6"/><path d="M5 20c1.4-3.6 4-5 7-5s5.6 1.4 7 5"/>',
    skills: '<path d="M12 3l2.2 6.8L21 12l-6.8 2.2L12 21l-2.2-6.8L3 12l6.8-2.2Z"/>',
    mastery: '<circle cx="12" cy="12" r="7"/><path d="M12 3v4M12 17v4M3 12h4M17 12h4"/>',
    health:
      '<path d="M12 20s-7-4.5-7-9.5C5 7.5 7 6 9 6c1.4 0 2.4.7 3 1.7C12.6 6.7 13.6 6 15 6c2 0 4 1.5 4 4.5 0 5-7 9.5-7 9.5Z"/>',
    clothes:
      '<path d="M8.5 4 4 7l2 3 2-1v11h8V9l2 1 2-3-4.5-3c-1 1.4-2 2-3.5 2s-2.5-.6-3.5-2Z"/>',
    scav:
      '<circle cx="9" cy="8.5" r="3"/><path d="M3.5 19c1-3 3-4.5 5.5-4.5s4.5 1.5 5.5 4.5"/><path d="M15.5 5.6a3 3 0 1 1 1.5 5.7"/><path d="M16.5 14.8c2.2.5 3.7 1.9 4.4 4.2"/>',
    traders: '<path d="M6 8h12l-1 12H7L6 8Z"/><path d="M9 8V6.5a3 3 0 0 1 6 0V8"/>',
    stash:
      '<path d="M3.5 8.5 12 4l8.5 4.5v7L12 20l-8.5-4.5v-7Z"/><path d="M3.5 8.5 12 13l8.5-4.5M12 13v7"/>',
    pockets:
      '<rect x="3.5" y="6" width="17" height="13" rx="2.5"/><path d="M15.5 11.5h5v3h-5a1.5 1.5 0 1 1 0-3Z"/>',
    encyclopedia:
      '<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z"/>',
    inspect: '<circle cx="11" cy="11" r="6.5"/><path d="m16 16 4.5 4.5"/>',
    issues: '<path d="M12 4 2.5 20h19L12 4Z"/><path d="M12 10v4M12 16.8v.2"/>',
    backup:
      '<rect x="4" y="5" width="16" height="4" rx="1"/><path d="M6 9v9a1 1 0 0 0 1 1h10a1 1 0 0 0 1-1V9"/><path d="M12 12v4M10 14l2 2 2-2"/>',
    settings:
      '<circle cx="12" cy="12" r="3"/><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M18.7 5.3l-2.1 2.1M7.4 16.6l-2.1 2.1"/>',
    quests:
      '<rect x="5" y="4" width="14" height="17" rx="2"/><path d="M9 4.5V3h6v1.5"/><path d="m9.5 13 2 2 3.5-3.5"/>',
    hideout: '<path d="M3.5 19h17L12 5 3.5 19Z"/><path d="m9.5 19 2.5-5 2.5 5"/>',
    presets:
      '<path d="M5 6h14M5 12h14M5 18h14"/><circle cx="9" cy="6" r="1.8"/><circle cx="15" cy="12" r="1.8"/><circle cx="8" cy="18" r="1.8"/>'
  } as const

  /** 入口可达路由（resolve 强类型要求字面量） */
  type EntryRoute =
    | '/character'
    | '/skills'
    | '/health'
    | '/traders'
    | '/stash'
    | '/inspect'
    | '/issues'
    | '/backup'
    | '/settings'
    | '/quests'
    | '/hideout'
    | '/presets'

  interface DashEntry {
    key: keyof typeof ICON
    to?: EntryRoute
    /** 无落地屏的入口：点击提示将在后续版本提供 */
    soon?: boolean
    /** 跨两列（02 稿图「检视物品」） */
    span2?: boolean
  }

  interface DashPanel {
    titleKey: MessageKey
    items: DashEntry[]
    single: boolean
  }

  const leftPanels: DashPanel[] = [
    {
      titleKey: 'home.panel.character',
      single: false,
      items: [
        { key: 'info', to: '/character' },
        { key: 'skills', to: '/skills' },
        { key: 'mastery', to: '/skills' },
        { key: 'health', to: '/health' },
        { key: 'clothes', to: '/character' },
        { key: 'scav', to: '/character' }
      ]
    },
    {
      titleKey: 'home.panel.economy',
      single: false,
      items: [
        { key: 'traders', to: '/traders' },
        { key: 'stash', to: '/stash' },
        { key: 'pockets', soon: true },
        { key: 'encyclopedia', soon: true },
        { key: 'inspect', to: '/inspect', span2: true }
      ]
    }
  ]

  const rightPanels: DashPanel[] = [
    {
      titleKey: 'home.panel.maintain',
      single: true,
      items: [
        // 问题检测红徽章：实算逻辑在问题检测屏落地后接入（诚实原则，0 时隐藏，本屏暂不渲染）
        { key: 'issues', to: '/issues' },
        { key: 'backup', to: '/backup' },
        { key: 'settings', to: '/settings' }
      ]
    },
    {
      titleKey: 'home.panel.progress',
      single: true,
      items: [
        { key: 'quests', to: '/quests' },
        { key: 'hideout', to: '/hideout' },
        { key: 'presets', to: '/presets' }
      ]
    }
  ]

  let profiles = $state<PhxProfileMeta[]>([])
  let currentId = $state('')
  let hero = $state<PhxProfileHero | null>(null)
  let saving = $state(false)

  const ddOptions = $derived(profiles.map((p) => ({ value: p.id, label: p.id })))

  const metaText = $derived.by(() => {
    if (!hero) return t('home.meta.none')
    const v = hero.gameVersion
    return v ? t('home.meta.loaded', { v }) : t('home.meta.loadedPlain')
  })

  const levelText = $derived.by(() => {
    if (!hero) return ''
    return hero.side
      ? t('home.hero.level', { n: hero.level, side: hero.side })
      : t('home.hero.levelOnly', { n: hero.level })
  })

  async function loadHeroData(): Promise<void> {
    hero = currentId ? await phxProfile.load(currentId) : null
  }

  /** 全量刷新：列表 → 当前选择（失效回退首个）→ 概览数据 */
  async function refresh(): Promise<void> {
    const list = await phxProfile.list()
    profiles = list
    let id = await phxProfile.current()
    if (id && !list.some((p) => p.id === id)) id = null
    if (!id && list.length > 0) id = list[0].id
    if (id && id !== currentId) await phxProfile.use(id)
    currentId = id ?? ''
    await loadHeroData()
  }

  async function onPick(id: string): Promise<void> {
    if (id === currentId) return
    currentId = id
    await phxProfile.use(id)
    await loadHeroData()
  }

  /** 保存修改：写回前主进程强制自动备份（领域铁律） */
  async function onSave(): Promise<void> {
    if (!hero || saving) return
    saving = true
    try {
      await phxProfile.save(hero.id)
      toast.success(t('home.toast.saved'))
    } catch {
      toast.error(t('home.toast.failed'))
    } finally {
      saving = false
    }
  }

  function onEntry(entry: DashEntry): void {
    if (entry.soon) {
      toast.warning(t('home.toast.soon'))
      return
    }
    if (entry.to) void goto(resolve(entry.to))
  }

  onMount(() => {
    void refresh()
    // 托盘「切换存档」后刷新首页（Rust 侧 emit；本页卸载时退订）
    const off = listen('tray:profile-switched', () => void refresh())
    return () => {
      void off.then((unlisten) => unlisten())
    }
  })
</script>

<div class="page">
  <div class="main-head">
    <div>
      <div class="large-title">{t('nav.home')}</div>
      <div class="head-meta">{metaText}</div>
    </div>
    <div class="head-actions">
      <button type="button" class="btn secondary" onclick={() => void goto(resolve('/server-select'))}>
        {t('home.act.back')}
      </button>
      <button type="button" class="btn primary" disabled={!hero || saving} onclick={() => void onSave()}>
        {t('home.act.save')}
      </button>
    </div>
  </div>

  <div class="scroll phx-hide-scroll">
    <section class="hero">
      <div class="hero-row1">
        <div class="avatar">
          <PhxIcon markup={ICON.info} />
        </div>
        <div class="hr-main">
          <div class="hr-label">{t('home.hero.label')}</div>
          <div class="hr-id">
            {#if profiles.length > 0}
              <span class="hero-dd">
                <PhxDropdown
                  value={currentId}
                  options={ddOptions}
                  label={t('home.hero.label')}
                  bare
                  onchange={(v) => void onPick(v)}
                />
              </span>
            {:else}
              <span class="hr-id-text">{t('home.empty.noprofile')}</span>
            {/if}
          </div>
        </div>
        {#if hero}
          <div class="hr-lvl">{levelText}</div>
        {/if}
      </div>
      <div class="hero-stats">
        <div class="stat">
          <div class="k">{t('home.hero.rub')}</div>
          <div class="v">{hero?.rub ?? '—'}</div>
        </div>
        <div class="stat">
          <div class="k">{t('home.hero.usd')}</div>
          <div class="v">{hero?.usd ?? '—'}</div>
        </div>
        <div class="stat">
          <div class="k">{t('home.hero.eur')}</div>
          <div class="v">{hero?.eur ?? '—'}</div>
        </div>
      </div>
    </section>

    <section class="dash">
      <div class="dash-col">
        {#each leftPanels as panel (panel.titleKey)}
          <div class="panel" class:single={panel.single}>
            <div class="panel-head"><span class="pt">{t(panel.titleKey)}</span></div>
            <div class="items">
              {#each panel.items as item (item.key)}
                <button
                  type="button"
                  class="item"
                  class:span2={item.span2}
                  onclick={() => onEntry(item)}
                >
                  <PhxIcon markup={ICON[item.key]} />
                  <span class="txt">{t(`home.item.${item.key}` as MessageKey)}</span>
                  <span class="chev">›</span>
                </button>
              {/each}
            </div>
          </div>
        {/each}
      </div>
      <div class="dash-col">
        {#each rightPanels as panel (panel.titleKey)}
          <div class="panel" class:single={panel.single}>
            <div class="panel-head"><span class="pt">{t(panel.titleKey)}</span></div>
            <div class="items">
              {#each panel.items as item (item.key)}
                <button
                  type="button"
                  class="item"
                  class:span2={item.span2}
                  onclick={() => onEntry(item)}
                >
                  <PhxIcon markup={ICON[item.key]} />
                  <span class="txt">{t(`home.item.${item.key}` as MessageKey)}</span>
                  <span class="chev">›</span>
                </button>
              {/each}
            </div>
          </div>
        {/each}
      </div>
    </section>
  </div>
</div>

<style>
  /* 02 稿图原值：页头 + hero 档案卡 + 2×2 仪表台 */
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
    gap: 8px;
    padding-bottom: 4px;
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
  .btn.secondary {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
  }
  .btn.secondary:hover {
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
    padding: 0 24px;
  }
  .hero {
    margin-top: 18px;
    background: var(--phx-surface-2);
    border-radius: var(--phx-r-card);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
  }
  .hero-row1 {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 16px;
  }
  .avatar {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }
  .avatar :global(svg) {
    width: 22px;
    height: 22px;
  }
  .hr-main {
    min-width: 0;
  }
  .hr-label {
    font-size: 12px;
    color: var(--phx-text-2);
  }
  .hr-id {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 3px;
  }
  .hr-id-text {
    font-family: var(--phx-font-mono);
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.2px;
  }
  /* bare 下拉的字体上下文：mono 15px 600，通过 font: inherit 传给触发钮 */
  .hero-dd {
    font-family: var(--phx-font-mono);
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.2px;
    max-width: 100%;
  }
  .hero-dd :global(.dd-val) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hr-lvl {
    margin-left: auto;
    font-size: 13px;
    color: var(--phx-text-2);
    flex: none;
  }
  .hero-stats {
    display: flex;
    border-top: 0.5px solid var(--phx-sep-strong);
    padding: 12px 4px;
  }
  .stat {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
  }
  .stat + .stat {
    border-left: 0.5px solid var(--phx-sep);
  }
  .stat .k {
    font-size: 12px;
    color: var(--phx-text-2);
  }
  .stat .v {
    font-family: var(--phx-font-mono);
    font-size: 17px;
    font-weight: 600;
  }
  .dash {
    margin-top: 14px;
    display: grid;
    grid-template-columns: 7fr 5fr;
    gap: 14px;
    align-items: stretch;
  }
  .dash-col {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }
  .panel {
    background: var(--phx-surface-2);
    border-radius: var(--phx-r-card);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
    padding: 4px 8px 8px;
  }
  .panel-head {
    display: flex;
    justify-content: space-between;
    padding: 10px 8px 6px;
  }
  .panel-head .pt {
    font-size: 13px;
    font-weight: 600;
    color: var(--phx-text-2);
  }
  .items {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px;
  }
  .panel.single .items {
    grid-template-columns: 1fr;
  }
  .item {
    height: 44px;
    padding: 0 10px;
    border: none;
    border-radius: 9px;
    background: transparent;
    color: var(--phx-text);
    font-size: 14px;
    font-family: inherit;
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
    text-align: left;
  }
  .item:hover {
    background: var(--phx-fill-quaternary);
  }
  .item :global(svg) {
    width: 16px;
    height: 16px;
    color: var(--phx-text-2);
    flex: none;
  }
  .item .txt {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item .chev {
    margin-left: auto;
    font-size: 16px;
    line-height: 1;
    color: var(--phx-text-3);
  }
  .span2 {
    grid-column: span 2;
  }
</style>
