<script lang="ts">
  // 01 服务器选择屏：bare 全窗独立屏（无 dock），按 docs/prototypes/01 稿图逐项落地
  import { goto } from '$app/navigation'
  import { resolve } from '$app/paths'
  import { t } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxServer, type PhxServerRow } from '$lib/ipc'
  import PhxDialog from '$lib/components/PhxDialog.svelte'

  let rows = $state<PhxServerRow[]>([])
  let version = $state('')
  let alertOpen = $state(false)
  let pendingRemove = $state<PhxServerRow | null>(null)

  async function load(): Promise<void> {
    rows = await phxServer.list()
  }

  $effect(() => {
    void (async () => {
      try {
        // 版本号对齐旧版：动态 import package.json（静态声明时 SvelteKit 会整包内联，此处无差异）
        const pkg = await import('../../../package.json')
        version = pkg.version
        await load()
      } catch {
        toast.error(t('ss.toast.failed'))
      }
    })()
  })

  /** 选目录 → 登记；重复目录仅提示 */
  async function add(): Promise<void> {
    try {
      const picked = await phxServer.pick()
      if (!picked) return
      const r = await phxServer.add(picked)
      await load()
      if (r.duplicate) toast.warning(t('ss.toast.exists'))
    } catch {
      toast.error(t('ss.toast.failed'))
    }
  }

  function askRemove(row: PhxServerRow): void {
    pendingRemove = row
    alertOpen = true
  }

  async function confirmRemove(): Promise<void> {
    const row = pendingRemove
    if (!row) return
    try {
      await phxServer.remove(row.id)
      await load()
    } catch {
      toast.error(t('ss.toast.failed'))
    }
  }

  /** 不可用的目录按钮已禁用，此处兜底忽略 */
  async function enter(row: PhxServerRow): Promise<void> {
    if (row.check !== 'ok') return
    try {
      const ok = await phxServer.use(row.id)
      if (ok) void goto(resolve('/home'))
      else toast.error(t('ss.toast.failed'))
    } catch {
      toast.error(t('ss.toast.failed'))
    }
  }

  function badgeText(row: PhxServerRow): string {
    return row.check === 'ok' ? t('ss.badge.ok') : t('ss.badge.bad')
  }

  function metaText(row: PhxServerRow): string {
    if (row.check === 'ok') {
      if (row.profileCount < 0) return t('ss.missing.profiles')
      return t('ss.meta.ok', { n: row.profileCount })
    }
    if (row.check === 'profiles') return t('ss.missing.profiles')
    return t('ss.missing.exe')
  }

  const removeText = $derived(t('ss.dlg.removeText', { name: pendingRemove?.name ?? '' }))
</script>

<div class="page phx-hide-scroll">
  <div class="brand">
    <span class="wordmark">Phoenix</span>
    <span class="brand-sub">{t('app.subtitle')}</span>
  </div>

  <h1 class="large-title">{t('ss.title')}</h1>
  <p class="lede">{t('ss.lede')}</p>

  {#if rows.length > 0}
    <div class="list-card">
      {#each rows as row (row.id)}
        <div class="srow">
          <div class="s-info">
            <div class="s-name">
              {row.name}
              <span class="badge" class:ok={row.check === 'ok'} class:bad={row.check !== 'ok'}>
                {badgeText(row)}
              </span>
            </div>
            <div class="s-path">{row.path}</div>
            <div class="s-meta" class:ok={row.check === 'ok'} class:bad={row.check !== 'ok'}>
              {metaText(row)}
            </div>
          </div>
          <div class="s-actions">
            <button
              type="button"
              class="btn btn-primary"
              disabled={row.check !== 'ok'}
              onclick={() => enter(row)}
            >
              {t('ss.enter')}
            </button>
            <button type="button" class="btn btn-ghost-red" onclick={() => askRemove(row)}>
              {t('ss.remove')}
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <button type="button" class="add-card" onclick={() => void add()}>
    <span class="plus">＋</span>{t('ss.add')}
  </button>

  <div class="footer">
    <span>Phoenix-DRX</span>
    <span class="sep">|</span>
    <span>{t('ss.footer.free')}</span>
    <span class="sep">|</span>
    <span>v{version}</span>
  </div>

  <PhxDialog
    bind:open={alertOpen}
    heading={t('ss.dlg.removeTitle')}
    text={removeText}
    confirmText={t('ss.remove')}
    danger
    onconfirm={() => void confirmRemove()}
  />
</div>

<style>
  /* 01 稿图原值逐项落地；色值一律 var(--phx-*)，无 token 场景（虚线边、分隔线宽）保留字面值 */
  .page {
    flex: 1;
    min-height: 0;
    width: 100%;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
  }
  .page > :global(*) {
    flex: none;
  }

  .brand {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin-bottom: 28px;
  }
  .wordmark {
    font-size: 30px;
    font-weight: 800;
    font-style: italic;
    letter-spacing: -0.5px;
    color: var(--phx-accent);
  }
  .brand-sub {
    font-size: 14px;
    font-weight: 600;
    color: var(--phx-text-2);
  }

  .large-title {
    font-size: 34px;
    font-weight: 700;
    letter-spacing: -0.4px;
    line-height: 1.15;
  }
  .lede {
    font-size: 15px;
    color: var(--phx-text-2);
    margin-top: 8px;
  }

  .list-card {
    width: 720px;
    margin-top: 30px;
    background: var(--phx-surface-2);
    border-radius: var(--phx-r-card);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
  }
  .srow {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 17px 16px;
  }
  .srow + .srow::before {
    content: '';
    position: absolute;
    top: 0;
    left: 16px;
    right: 0;
    height: 0.5px;
    background: var(--phx-sep-strong);
  }

  .s-name {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.3px;
  }
  .badge {
    font-size: 11px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: 999px;
  }
  .badge.ok {
    color: var(--phx-green);
    background: var(--phx-green-dim);
  }
  .badge.bad {
    color: var(--phx-red);
    background: var(--phx-red-dim);
  }
  .s-path {
    font-family: var(--phx-font-mono);
    font-size: 13px;
    color: var(--phx-text-2);
    margin-top: 5px;
  }
  .s-meta {
    font-size: 13px;
    margin-top: 7px;
  }
  .s-meta.ok {
    color: var(--phx-green);
  }
  .s-meta.bad {
    color: var(--phx-red);
  }

  .s-actions {
    display: flex;
    gap: 6px;
    flex: none;
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
  .btn-primary {
    background: var(--phx-accent);
    color: #fff;
  }
  .btn-primary:not(:disabled):hover {
    filter: brightness(1.12);
  }
  .btn-ghost-red {
    background: transparent;
    color: var(--phx-red);
  }
  .btn-ghost-red:hover {
    background: var(--phx-red-dim);
  }
  .btn:disabled {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text-3);
    cursor: default;
  }

  .add-card {
    width: 720px;
    height: 56px;
    margin-top: 12px;
    border: 1.5px dashed rgba(255, 255, 255, 0.28);
    border-radius: var(--phx-r-card);
    background: transparent;
    color: var(--phx-text-2);
    font-size: 15px;
    font-weight: 500;
    font-family: inherit;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    cursor: pointer;
    transition:
      border-color var(--phx-dur-fast) var(--phx-ease-out),
      color var(--phx-dur-fast) var(--phx-ease-out),
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }
  .add-card:hover {
    border-color: var(--phx-accent);
    color: var(--phx-accent);
  }

  .footer {
    margin-top: auto;
    padding: 12px 0;
    font-size: 12px;
    color: var(--phx-text-3);
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .footer .sep {
    opacity: 0.5;
  }
</style>
