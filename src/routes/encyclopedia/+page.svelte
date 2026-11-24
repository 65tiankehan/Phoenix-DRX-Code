<script lang="ts">
  // 物品图鉴：服务器数据库全量物品（只读浏览）+ 一键加入仓库。
  // 数据来自 encyclopedia_catalog（Rust 直读 templates/items.json + 分类节点），
  // 加入仓库复用 stash_save 的 addItem（写前自动备份、放置引擎在 Rust 侧）。
  import { onMount } from 'svelte'
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { t } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxProfile, phxStash, phxEncyclopedia, type PhxCatalog, type PhxCatalogItem } from '$lib/ipc'
  import PhxIcon from '$lib/components/PhxIcon.svelte'
  import { playSound } from '$lib/sound'

  const ICON = {
    search: '<circle cx="11" cy="11" r="6.5"/><path d="m20 20-3.8-3.8"/>',
    caret: '<path d="m9 5 7 7-7 7"/>',
    plus: '<path d="M5 12h14M12 5v14"/>',
    close: '<path d="m3 3 14 14M17 3 3 17"/>',
    box: '<path d="M4 7.5 12 3l8 4.5v9L12 21l-8-4.5v-9Z"/><path d="m4 7.5 8 4.5 8-4.5M12 12v9"/>'
  }

  let currentId = $state('')
  let catalog = $state<PhxCatalog | null>(null)
  let loading = $state(true)
  let failed = $state('')

  /** 搜索词（名称 / 短名 / 模板 id 三路命中） */
  let query = $state('')
  /** 选中的分类（空 = 全部） */
  let picked = $state('')
  /** 折叠的分类 id 集合 */
  let collapsed = $state.raw(new SvelteSet<string>())
  /** 选中物品（详情面板） */
  let active = $state<PhxCatalogItem | null>(null)
  let amount = $state(1)
  let adding = $state(false)

  /** 分类 id → 子分类 */
  const kids = $derived.by(() => {
    const m = new SvelteMap<string | null, PhxCatalog['categories']>()
    for (const c of catalog?.categories ?? []) {
      const k = c.parent
      if (!m.has(k)) m.set(k, [])
      m.get(k)!.push(c)
    }
    return m
  })

  /** 分类 id → 自身 + 全部后代（筛选用） */
  const subtree = $derived.by(() => {
    const m = new SvelteMap<string, SvelteSet<string>>()
    const kidsOf = kids
    const walk = (id: string): Set<string> => {
      const hit = m.get(id)
      if (hit) return hit
      const set = new SvelteSet<string>([id])
      m.set(id, set)
      for (const c of kidsOf.get(id) ?? []) {
        for (const x of walk(c.id)) set.add(x)
      }
      return set
    }
    for (const c of catalog?.categories ?? []) walk(c.id)
    return m
  })

  const catById = $derived.by(() => {
    const m = new SvelteMap<string, PhxCatalog['categories'][number]>()
    for (const c of catalog?.categories ?? []) m.set(c.id, c)
    return m
  })

  /** 分类路径文案：组合物品 / 武器 / 突击步枪 */
  function pathOf(item: PhxCatalogItem): string {
    const names: string[] = []
    let cur = catById.get(item.leaf)?.parent ?? null
    const leaf = catById.get(item.leaf)
    if (leaf) names.unshift(leaf.name)
    let guard = 0
    while (cur && guard++ < 32) {
      const c = catById.get(cur)
      if (!c) break
      names.unshift(c.name)
      cur = c.parent
    }
    return names.join(' / ')
  }

  const rows = $derived.by(() => {
    const list = catalog?.items ?? []
    const q = query.trim().toLowerCase()
    const scope = picked ? subtree.get(picked) : null
    return list.filter((it) => {
      if (scope && !scope.has(it.leaf)) return false
      if (!q) return true
      return (
        it.name.toLowerCase().includes(q) ||
        it.short.toLowerCase().includes(q) ||
        it.tpl.toLowerCase().includes(q)
      )
    })
  })

  const total = $derived(catalog?.items.length ?? 0)

  // ——— 虚拟滚动：4553 行不能全渲染 ———
  const ROW_H = 62
  let viewport = $state<HTMLElement | null>(null)
  let scrollTop = $state(0)
  let viewH = $state(600)
  const startIdx = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - 6))
  const endIdx = $derived(Math.min(rows.length, startIdx + Math.ceil(viewH / ROW_H) + 12))
  const slice = $derived(rows.slice(startIdx, endIdx))

  function onScroll(e: Event) {
    scrollTop = (e.currentTarget as HTMLDivElement).scrollTop
  }

  function toggle(id: string) {
    const next = new SvelteSet(collapsed)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    collapsed = next
    playSound('ui')
  }

  function pick(id: string) {
    picked = picked === id ? '' : id
    scrollTop = 0
    if (viewport) viewport.scrollTop = 0
    playSound('ui')
  }

  function openDetail(it: PhxCatalogItem) {
    active = it
    amount = 1
    playSound('ui')
  }

  async function add() {
    if (!active || adding) return
    if (!currentId) {
      toast.warning(t('enc.noProfile'))
      return
    }
    adding = true
    try {
      const res = await phxStash.save(currentId, {
        ops: [{ op: 'addItem', tpl: active.tpl, count: Math.max(1, Math.min(100, amount)) }]
      })
      toast.success(t('enc.added', { n: Math.max(1, Math.min(100, amount)), name: active.name }))
      if (res.backupPath) toast.info(t('enc.backup'))
      playSound('success')
    } catch (e) {
      toast.error(t('enc.addFail'))
      console.error(e)
    } finally {
      adding = false
    }
  }

  onMount(() => {
    const el = viewport
    if (el) viewH = el.clientHeight || 600
    const onResize = () => {
      if (viewport) viewH = viewport.clientHeight || 600
    }
    window.addEventListener('resize', onResize)
    void (async () => {
      try {
        currentId = (await phxProfile.current()) ?? ''
        catalog = await phxEncyclopedia.catalog()
        if (!catalog) failed = t('enc.noServer')
      } catch (e) {
        failed = String(e)
      } finally {
        loading = false
      }
    })()
    return () => window.removeEventListener('resize', onResize)
  })

  const fmt = (n: number) => n.toLocaleString('zh-CN')
</script>

<div class="page">
  <header class="head">
    <div class="titles">
      <h1>{t('enc.title')}</h1>
      <p class="sub">
        {#if total}
          {t('enc.sub', { n: fmt(total) })}
        {:else}
          {t('enc.subEmpty')}
        {/if}
      </p>
    </div>
    <div class="search">
      <PhxIcon markup={ICON.search} size={16} />
      <input
        type="text"
        bind:value={query}
        placeholder={t('enc.searchPlaceholder')}
        aria-label={t('enc.searchPlaceholder')}
      />
      {#if query}
        <button class="clear" onclick={() => (query = '')} aria-label={t('enc.clear')}>
          <PhxIcon markup={ICON.close} size={13} />
        </button>
      {/if}
    </div>
  </header>

  {#if loading}
    <div class="state">{t('enc.loading')}</div>
  {:else if failed}
    <div class="state bad">{failed}</div>
  {:else}
    <div class="body">
      <aside class="tree">
        <button class="node top" class:on={!picked} onclick={() => pick('')}>
          <span class="nm">{t('enc.all')}</span>
          <span class="ct">{fmt(total)}</span>
        </button>
        {#each kids.get(null) ?? [] as c (c.id)}
          {@const open = !collapsed.has(c.id)}
          <button class="node" class:on={picked === c.id} onclick={() => pick(c.id)}>
            {#if (kids.get(c.id) ?? []).length > 0}
              <span
                class="tw"
                class:closed={!open}
                role="presentation"
                onclick={(e) => {
                  e.stopPropagation()
                  toggle(c.id)
                }}
              >
                <PhxIcon markup={ICON.caret} size={12} />
              </span>
            {:else}
              <span class="tw"></span>
            {/if}
            <span class="nm">{c.name}</span>
            <span class="ct">{fmt(c.count)}</span>
          </button>
          {#if open}
            {#each kids.get(c.id) ?? [] as s2 (s2.id)}
              <button class="node lv2" class:on={picked === s2.id} onclick={() => pick(s2.id)}>
                <span class="tw"></span>
                <span class="nm">{s2.name}</span>
                <span class="ct">{fmt(s2.count)}</span>
              </button>
            {/each}
          {/if}
        {/each}
      </aside>

      <section class="list" bind:this={viewport} onscroll={onScroll}>
        {#if rows.length === 0}
          <div class="state">{t('enc.empty')}</div>
        {:else}
          <div class="spacer" style:height="{rows.length * ROW_H}px">
            <div class="win" style:transform="translateY({startIdx * ROW_H}px)">
              {#each slice as it (it.tpl)}
                <button
                  class="row"
                  class:on={active?.tpl === it.tpl}
                  style:height="{ROW_H}px"
                  onclick={() => openDetail(it)}
                >
                  <span class="icon"><PhxIcon markup={ICON.box} size={16} /></span>
                  <span class="main">
                    <span class="nm">{it.name}</span>
                    <span class="meta">
                      {pathOf(it)}
                      {#if it.short}· {it.short}{/if}
                    </span>
                  </span>
                  <span class="dims">{it.w}×{it.h}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </section>

      <aside class="detail" class:empty={!active}>
        {#if active}
          <div class="dhead">
            <h2>{active.name}</h2>
            {#if active.short}<p class="dshort">{active.short}</p>{/if}
          </div>
          <dl class="kv">
            <dt>{t('enc.category')}</dt>
            <dd>{pathOf(active)}</dd>
            <dt>{t('enc.size')}</dt>
            <dd>{active.w} × {active.h}</dd>
            <dt>{t('enc.stack')}</dt>
            <dd>{fmt(active.stack)}</dd>
            <dt>{t('enc.price')}</dt>
            <dd>₽ {fmt(active.price)}</dd>
            {#if active.rarity}
              <dt>{t('enc.rarity')}</dt>
              <dd>{active.rarity}</dd>
            {/if}
            <dt>{t('enc.tpl')}</dt>
            <dd class="mono">{active.tpl}</dd>
          </dl>
          <div class="add">
            <label class="amt">
              <span>{t('enc.amount')}</span>
              <input
                type="number"
                min="1"
                max="100"
                bind:value={amount}
                aria-label={t('enc.amount')}
              />
            </label>
            <button class="primary" disabled={adding || !currentId} onclick={add}>
              <PhxIcon markup={ICON.plus} size={15} />
              {adding ? t('enc.adding') : t('enc.toStash')}
            </button>
            {#if !currentId}
              <p class="hint">{t('enc.noProfile')}</p>
            {:else}
              <p class="hint">{t('enc.hint')}</p>
            {/if}
          </div>
        {:else}
          <div class="state">{t('enc.pickHint')}</div>
        {/if}
      </aside>
    </div>
  {/if}
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    padding: 22px 26px 18px;
    gap: 16px;
  }
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 20px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0;
    font: 700 30px/1.15 var(--phx-font-brand);
    color: var(--phx-text);
  }
  .sub {
    margin: 6px 0 0;
    font: 400 13px/1.4 var(--phx-font-sans);
    color: var(--phx-text-3);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    height: 38px;
    min-width: 280px;
    border-radius: var(--phx-r-btn);
    background: var(--phx-surface-2);
    border: 1px solid var(--phx-sep);
    color: var(--phx-text-3);
  }
  .search input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 0;
    outline: none;
    color: var(--phx-text);
    font: 400 13px/1 var(--phx-font-sans);
  }
  .clear {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 50%;
    background: var(--phx-fill-tertiary);
    color: var(--phx-text-2);
    cursor: pointer;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 232px minmax(0, 1fr) 300px;
    gap: 14px;
  }
  .tree {
    overflow: auto;
    padding: 8px;
    border-radius: var(--phx-r-card);
    background: var(--phx-surface);
    border: 1px solid var(--phx-sep);
  }
  .node {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 7px 8px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--phx-text-2);
    font: 400 13px/1.2 var(--phx-font-sans);
    text-align: left;
    cursor: pointer;
  }
  .node:hover {
    background: var(--phx-fill-secondary);
    color: var(--phx-text);
  }
  .node.on {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
  }
  .node.top {
    font-weight: 600;
  }
  .node.lv2 {
    padding-left: 26px;
    font-size: 12.5px;
  }
  .tw {
    display: grid;
    place-items: center;
    width: 14px;
    height: 14px;
    color: var(--phx-text-3);
    transition: transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .tw.closed {
    transform: rotate(-0deg);
  }
  .tw:not(.closed) {
    transform: rotate(90deg);
  }
  .nm {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ct {
    font: 400 11px/1 var(--phx-font-mono);
    color: var(--phx-text-3);
  }
  .list {
    position: relative;
    overflow: auto;
    border-radius: var(--phx-r-card);
    background: var(--phx-surface);
    border: 1px solid var(--phx-sep);
  }
  .spacer {
    position: relative;
    width: 100%;
  }
  .win {
    position: absolute;
    inset: 0 0 auto 0;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 0 14px;
    border: 0;
    border-bottom: 1px solid var(--phx-sep);
    background: transparent;
    color: var(--phx-text);
    text-align: left;
    cursor: pointer;
  }
  .row:hover {
    background: var(--phx-fill-secondary);
  }
  .row.on {
    background: var(--phx-fill-tertiary);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    background: var(--phx-fill-tertiary);
    color: var(--phx-accent);
    flex: none;
  }
  .main {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    flex: 1;
  }
  .main .nm {
    font: 500 13.5px/1.2 var(--phx-font-sans);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    font: 400 11.5px/1.2 var(--phx-font-sans);
    color: var(--phx-text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dims {
    font: 400 11.5px/1 var(--phx-font-mono);
    color: var(--phx-text-3);
    flex: none;
  }
  .detail {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px;
    overflow: auto;
    border-radius: var(--phx-r-card);
    background: var(--phx-surface);
    border: 1px solid var(--phx-sep);
  }
  .detail.empty {
    justify-content: center;
  }
  .dhead h2 {
    margin: 0;
    font: 600 17px/1.3 var(--phx-font-sans);
    color: var(--phx-text);
  }
  .dshort {
    margin: 4px 0 0;
    font: 400 12px/1.3 var(--phx-font-mono);
    color: var(--phx-text-3);
  }
  .kv {
    display: grid;
    grid-template-columns: 84px minmax(0, 1fr);
    gap: 8px 10px;
    margin: 0;
    font: 400 12.5px/1.4 var(--phx-font-sans);
  }
  .kv dt {
    color: var(--phx-text-3);
  }
  .kv dd {
    margin: 0;
    color: var(--phx-text-2);
    word-break: break-word;
  }
  .kv .mono {
    font-family: var(--phx-font-mono);
    font-size: 11.5px;
  }
  .add {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .amt {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    font: 400 12.5px/1 var(--phx-font-sans);
    color: var(--phx-text-2);
  }
  .amt input {
    width: 88px;
    height: 32px;
    padding: 0 10px;
    border-radius: 8px;
    background: var(--phx-surface-2);
    border: 1px solid var(--phx-sep);
    color: var(--phx-text);
    font: 400 13px/1 var(--phx-font-mono);
    outline: none;
  }
  .primary {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 38px;
    border: 0;
    border-radius: var(--phx-r-btn);
    background: var(--phx-accent);
    color: var(--phx-win-bg);
    font: 600 13px/1 var(--phx-font-sans);
    cursor: pointer;
  }
  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .hint {
    margin: 0;
    font: 400 11.5px/1.4 var(--phx-font-sans);
    color: var(--phx-text-3);
  }
  .state {
    display: grid;
    place-items: center;
    flex: 1;
    padding: 40px;
    font: 400 13px/1.5 var(--phx-font-sans);
    color: var(--phx-text-3);
  }
  .state.bad {
    color: var(--phx-danger);
  }
</style>
