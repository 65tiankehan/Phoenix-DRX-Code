<!--
  20 稿图生产实现：全局命令面板（Ctrl+K）。
  z-modal 层，主窗专属（薄窗不挂载，见 +layout.svelte）。
  稿图偏离说明：面板定位由稿图 absolute top:108px 改为 overlay flex + padding-top（动画 transform 不与定位冲突）；
  max-height 上限 560px，小窗自适应为视口余量。
-->
<script lang="ts">
  import { tick } from 'svelte'
  import { page } from '$app/state'
  import { resolve } from '$app/paths'
  import { goto } from '$app/navigation'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import { t } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxTray, phxUpdate } from '$lib/ipc'
  import { playSound } from '$lib/sound'
  import { CMDK_ENTRIES, CMDK_GROUPS, closeCmdk, cmdk, toggleCmdk } from '$lib/cmdk.svelte'
  import type { CmdkAction, CmdkEntry, CmdkGroup } from '$lib/cmdk.svelte'
  import type { MessageKey } from '$lib/i18n/i18n.svelte'

  // 与 about 页保持一致，改动须两处同步
  const HOME_URL = 'https://github.com/65tiankehan'
  const ISSUES_URL = 'https://github.com/65tiankehan/Phoenix-DRX-releases/issues'

  let query = $state('')
  let activeIdx = $state(0)
  let inputEl: HTMLInputElement | undefined = $state()
  let listEl: HTMLDivElement | undefined = $state()

  const q = $derived(query.trim().toLowerCase())

  /** 按名称 + 描述前缀过滤；空查询返回全量（显示顺序即索引数组顺序） */
  const filtered = $derived.by(() => {
    if (!q) return CMDK_ENTRIES
    return CMDK_ENTRIES.filter((e) => `${t(e.nm)} ${t(e.ds)}`.toLowerCase().includes(q))
  })

  /** 按 CMDK_GROUPS 固定组序切片，空组剔除 */
  const groups = $derived.by(() => {
    const out: Array<{ id: CmdkGroup; label: MessageKey; items: readonly CmdkEntry[] }> = []
    for (const g of CMDK_GROUPS) {
      const items = filtered.filter((e) => e.group === g.id)
      if (items.length > 0) out.push({ id: g.id, label: g.label, items })
    }
    return out
  })

  // 打开时重置输入与选中位，焦点落到输入框
  $effect(() => {
    if (cmdk.open) {
      query = ''
      activeIdx = 0
      void tick().then(() => inputEl?.focus())
    }
  })

  // 查询变化重置选中位到首行
  $effect(() => {
    void query
    activeIdx = 0
  })

  // 选中位变化后把当前行滚进可视区
  $effect(() => {
    void activeIdx
    void tick().then(() => {
      listEl?.querySelector('.cmdk-row[data-active]')?.scrollIntoView({ block: 'nearest' })
    })
  })

  /** 名称高亮三段切分（前段 / 命中 / 后段循环），只渲染名称 */
  function segments(text: string, needle: string): Array<{ t: string; hit: boolean }> {
    if (!needle) return [{ t: text, hit: false }]
    const out: Array<{ t: string; hit: boolean }> = []
    let rest = text
    let i = rest.toLowerCase().indexOf(needle)
    while (i !== -1) {
      if (i > 0) out.push({ t: rest.slice(0, i), hit: false })
      out.push({ t: rest.slice(i, i + needle.length), hit: true })
      rest = rest.slice(i + needle.length)
      i = rest.toLowerCase().indexOf(needle)
    }
    if (rest) out.push({ t: rest, hit: false })
    return out
  }

  function move(d: number): void {
    if (filtered.length === 0) return
    activeIdx = (activeIdx + d + filtered.length) % filtered.length
  }

  function run(entry: CmdkEntry): void {
    closeCmdk()
    if (entry.action) {
      runAction(entry.action)
      return
    }
    const target = resolve(entry.route!)
    playSound(page.url.pathname === target ? 'ui' : 'nav')
    // target 已是 resolve() 结果，仅拼了查询串；规则无法静态识别，故局部豁免
    // eslint-disable-next-line svelte/no-navigation-without-resolve
    void goto(target + (entry.anchor ? `?group=${entry.anchor}` : ''))
  }

  function runAction(action: CmdkAction): void {
    if (action === 'check-update') {
      // 与设置页 checkNow 一致：notAvailable / error 用 toast；available 时 Rust 侧已自动开薄窗
      void phxUpdate
        .check(true)
        .then((s) => {
          if (s.phase === 'notAvailable') toast.success(t('update.noUpdate'))
          else if (s.phase === 'error') toast.error(t('update.checkFail'))
        })
        .catch(() => toast.error(t('update.checkFail')))
    } else if (action === 'open-github') {
      void openUrl(HOME_URL)
    } else if (action === 'open-issues') {
      void openUrl(ISSUES_URL)
    } else if (action === 'open-profiles') {
      void phxTray.openProfilesDir()
    }
  }

  function onKeydown(e: KeyboardEvent): void {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
      e.preventDefault()
      toggleCmdk()
      return
    }
    if (!cmdk.open) return
    if (e.key === 'Escape') {
      e.preventDefault()
      closeCmdk()
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      move(1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      move(-1)
    } else if (e.key === 'Enter') {
      e.preventDefault()
      const entry = filtered[activeIdx]
      if (entry) run(entry)
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if cmdk.open}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) closeCmdk()
    }}
  >
    <div class="cmdk" role="dialog" aria-modal="true" aria-label={t('cmdk.search')}>
      <div class="cmdk-input">
        <svg
          viewBox="0 0 17 17"
          fill="none"
          stroke="currentColor"
          stroke-width="1.3"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <circle cx="7.4" cy="7.4" r="4.6" /><path d="M14.2 14.2 10.8 10.8" />
        </svg>
        <input
          bind:this={inputEl}
          bind:value={query}
          type="text"
          placeholder={t('cmdk.placeholder')}
          spellcheck="false"
        />
        <span class="esc">esc</span>
      </div>
      {#if filtered.length > 0}
        <div class="cmdk-list" bind:this={listEl}>
          {#each groups as g (g.id)}
            <div class="cmdk-grouph">{t(g.label)}</div>
            {#each g.items as item (item.id)}
              {@const idx = filtered.indexOf(item)}
              <button
                type="button"
                class="cmdk-row"
                data-active={idx === activeIdx ? '' : undefined}
                onmouseenter={() => (activeIdx = idx)}
                onclick={() => run(item)}
              >
                <svg
                  viewBox="0 0 17 17"
                  fill={item.fill ? 'currentColor' : 'none'}
                  stroke={item.fill ? 'none' : 'currentColor'}
                  stroke-width="1.3"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  aria-hidden="true"
                >
                  <!-- icon 来自 cmdk.svelte.ts 内置常量，非用户输入，无 XSS 面（同 PhxIcon） -->
                  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                  {@html item.icon}
                </svg>
                <span class="nm">
                  {#each segments(t(item.nm), q) as seg, si (si)}{#if seg.hit}<span class="hl"
                      >{seg.t}</span
                    >{:else}{seg.t}{/if}{/each}
                </span>
                <span class="ds">{t(item.ds)}</span>
                <span class="kr">↵</span>
              </button>
            {/each}
          {/each}
        </div>
      {:else}
        <div class="cmdk-empty">
          <svg
            viewBox="0 0 17 17"
            fill="none"
            stroke="currentColor"
            stroke-width="1.3"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <circle cx="7.4" cy="7.4" r="4.6" /><path d="M14.2 14.2 10.8 10.8" />
          </svg>
          <div class="t1">{t('cmdk.empty')}</div>
          <div class="t2">{t('cmdk.emptyHint')}</div>
        </div>
      {/if}
      <div class="cmdk-foot">
        {#if filtered.length > 0}
          <span><span class="key">↑↓</span>{t('cmdk.footer.nav')}</span>
          <span><span class="key">↵</span>{t('cmdk.footer.enter')}</span>
        {/if}
        <span><span class="key">esc</span>{t('cmdk.footer.esc')}</span>
        {#if !q}
          <span class="hint">{t('cmdk.footer.hint')}</span>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: absolute;
    inset: 0;
    z-index: var(--phx-z-modal);
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding-top: 108px;
    background: rgba(0, 0, 0, 0.5);
    animation: cmdk-overlay-in var(--phx-dur-norm) var(--phx-ease-out) both;
  }
  @keyframes cmdk-overlay-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  .cmdk {
    width: 560px;
    max-height: min(560px, calc(100% - 108px - 16px));
    display: flex;
    flex-direction: column;
    background: var(--phx-glass);
    backdrop-filter: blur(28px) saturate(1.4);
    -webkit-backdrop-filter: blur(28px) saturate(1.4);
    border-radius: 14px;
    box-shadow:
      0 24px 64px rgba(0, 0, 0, 0.55),
      0 0 0 0.5px var(--phx-sep-strong);
    overflow: hidden;
    animation: cmdk-panel-in var(--phx-dur-norm) var(--phx-ease-emphasized) both;
  }
  @keyframes cmdk-panel-in {
    from {
      transform: translateY(8px) scale(0.98);
    }
    to {
      transform: none;
    }
  }
  .cmdk-input {
    height: 54px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px 0 18px;
    border-bottom: 1px solid var(--phx-sep);
    color: var(--phx-text-2);
  }
  .cmdk-input svg {
    width: 17px;
    height: 17px;
    flex: none;
  }
  .cmdk-input input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: none;
    outline: none;
    color: var(--phx-text);
    font-size: 15px;
    font-family: inherit;
  }
  .cmdk-input input::placeholder {
    color: var(--phx-text-3);
  }
  .esc {
    height: 22px;
    flex: none;
    display: flex;
    align-items: center;
    padding: 0 8px;
    border-radius: 6px;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--phx-text-3);
    background: var(--phx-fill-tertiary);
    border: 1px solid var(--phx-sep);
  }
  .cmdk-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 8px;
    scrollbar-width: none;
  }
  .cmdk-list::-webkit-scrollbar {
    display: none;
  }
  .cmdk-grouph {
    padding: 10px 10px 6px 11px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    color: var(--phx-text-3);
  }
  .cmdk-row {
    height: 40px;
    width: 100%;
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 0 10px;
    border: none;
    border-radius: 9px;
    background: transparent;
    color: var(--phx-text);
    cursor: pointer;
    font-family: inherit;
    text-align: left;
  }
  .cmdk-row svg {
    width: 17px;
    height: 17px;
    flex: none;
    color: var(--phx-text-2);
  }
  .cmdk-row .nm {
    font-size: 13.5px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cmdk-row .ds {
    margin-left: auto;
    flex: none;
    font-size: 11.5px;
    color: var(--phx-text-3);
  }
  .cmdk-row .kr {
    display: none;
    height: 20px;
    margin-left: auto;
    padding: 0 6px;
    align-items: center;
    border-radius: 5px;
    font-size: 10.5px;
    font-weight: 600;
    color: var(--phx-text-3);
    background: var(--phx-fill-tertiary);
    border: 1px solid var(--phx-sep);
  }
  .cmdk-row[data-active] {
    background: var(--phx-accent-dim);
  }
  .cmdk-row[data-active] svg {
    color: var(--phx-accent);
  }
  .cmdk-row[data-active] .ds {
    display: none;
  }
  .cmdk-row[data-active] .kr {
    display: flex;
  }
  .cmdk-row .hl {
    color: var(--phx-accent);
    font-weight: 600;
  }
  .cmdk-empty {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 40px 20px 44px;
    color: var(--phx-text-3);
  }
  .cmdk-empty svg {
    width: 30px;
    height: 30px;
  }
  .cmdk-empty .t1 {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--phx-text-2);
  }
  .cmdk-empty .t2 {
    font-size: 12px;
  }
  .cmdk-foot {
    height: 38px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0 14px;
    border-top: 1px solid var(--phx-sep);
    font-size: 11px;
    color: var(--phx-text-3);
  }
  .cmdk-foot span {
    display: flex;
    align-items: center;
  }
  .cmdk-foot .key {
    height: 18px;
    margin-right: 5px;
    padding: 0 6px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 600;
    color: var(--phx-text-3);
    background: var(--phx-fill-tertiary);
    border: 1px solid var(--phx-sep);
  }
  .cmdk-foot .hint {
    margin-left: auto;
  }
</style>
