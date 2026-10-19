<script lang="ts">
  // 14-backup 备份页：两列表格（日期 / 执行）+ 自绘确认弹窗 + 空态。
  // 备份来源唯一（只在「保存修改」时自动生成），页头不设任何按钮（14 稿注 2）。
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxBackup, phxProfile, type PhxBackupRow } from '$lib/ipc'
  import PhxDialog from '$lib/components/PhxDialog.svelte'

  /** 骨架屏行序号生成 */
  const skRows = (n: number) => [...Array(n).keys()]

  let booting = $state(true)
  let loaded = $state(false)
  let profileId = $state('')
  let rows = $state<PhxBackupRow[]>([])
  let busy = $state(false)

  // 确认弹窗：回档（danger=false 主题色）/ 删除（danger=true 红）共用一个 PhxDialog
  let dlgOpen = $state(false)
  let dlgKind = $state<'restore' | 'delete'>('restore')
  let dlgTarget = $state<PhxBackupRow | null>(null)

  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    profileId = id ?? ''
    rows = profileId ? await phxBackup.list(profileId) : []
  }
  onMount(() => {
    void refresh().finally(() => {
      loaded = true
      booting = false
    })
  })

  /** dd.MM.yyyy HH:mm:ss（14 稿注 3：参考 FormatedDate 口径） */
  function fmtDate(ms: number): string {
    const d = new Date(ms)
    const p = (n: number) => String(n).padStart(2, '0')
    return `${p(d.getDate())}.${p(d.getMonth() + 1)}.${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
  }

  function ask(row: PhxBackupRow, kind: 'restore' | 'delete'): void {
    if (busy) return
    dlgKind = kind
    dlgTarget = row
    dlgOpen = true
  }

  async function confirmDlg(): Promise<void> {
    const target = dlgTarget
    if (busy || !target || !profileId) return
    busy = true
    try {
      if (dlgKind === 'restore') {
        await phxBackup.restore(profileId, target.id)
        toast.success(t('backup.toast.restored'))
      } else {
        await phxBackup.delete(profileId, target.id)
        toast.success(t('backup.toast.deleted'))
      }
      await refresh()
    } catch {
      toast.error(t('backup.toast.failed'))
    } finally {
      busy = false
    }
  }
</script>

<main class="page">
  <header class="main-head">
    <div>
      <h1 class="large-title">{t('nav.backup')}</h1>
      <p class="head-meta">{t('backup.meta')}</p>
    </div>
  </header>
  <div class="body">
    <div class="boot phx-boot" class:gone={!booting}>
      <div class="qcard">
        <div class="bhead">
          <i class="phx-sk bh1-sk"></i>
          <i class="phx-sk bh2-sk"></i>
        </div>
        {#each skRows(6) as i (i)}
          <div class="brow">
            <i class="phx-sk bdate-sk"></i>
            <div class="acts">
              <i class="phx-sk bbtn-sk"></i>
              <i class="phx-sk bbtn-sk"></i>
            </div>
          </div>
        {/each}
      </div>
    </div>
    {#if loaded}
      <div class="scroll phx-fade-in">
        <div class="qcard">
          <div class="bhead"><span>{t('backup.colDate')}</span><span>{t('backup.colActions')}</span></div>
        {#each rows as row (row.id)}
          <div class="brow">
            <div class="bdate">{fmtDate(row.timeMs)}</div>
            <div class="acts">
              <button type="button" class="bbtn" disabled={busy} onclick={() => ask(row, 'restore')}>
                {t('backup.restore')}
              </button>
              <button type="button" class="bbtn danger" disabled={busy} onclick={() => ask(row, 'delete')}>
                {t('backup.delete')}
              </button>
            </div>
          </div>
        {/each}
        {#if rows.length === 0}
          <div class="empty">
            {t('backup.empty')}<br />{t('backup.emptyHint')}
          </div>
        {/if}
      </div>
    </div>
  {/if}
  </div>
</main>

<PhxDialog
  bind:open={dlgOpen}
  heading={dlgKind === 'restore' ? t('backup.restoreTitle') : t('backup.deleteTitle')}
  text={dlgKind === 'restore' ? t('backup.restoreText') : t('backup.deleteText')}
  confirmText={dlgKind === 'restore' ? t('backup.restore') : t('backup.delete')}
  danger={dlgKind === 'delete'}
  onconfirm={() => void confirmDlg()}
/>

<style>
  /* 外壳三段式（同 settings / issues 页惯例） */
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .main-head {
    padding: 24px 24px 0;
    flex: none;
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
    font-size: 13px;
    color: var(--phx-text-2);
    margin-top: 6px;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 24px 104px;
  }
  /* 加载态包装层：boot 覆盖层 + 内容同层交叉淡化 */
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .boot {
    padding: 0 24px 104px;
  }
  .boot .bh1-sk {
    width: 48px;
    height: 12px;
    border-radius: 6px;
  }
  .boot .bh2-sk {
    width: 24px;
    height: 12px;
    border-radius: 6px;
  }
  .boot .bdate-sk {
    width: 64%;
    height: 13px;
    border-radius: 6px;
  }
  .boot .bbtn-sk {
    width: 64px;
    height: 26px;
    border-radius: 7px;
    flex: none;
  }

  /* 14 稿原值：备份列表两列表格 */
  .qcard {
    margin-top: 16px;
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow: hidden;
  }
  .bhead {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
    padding: 10px 16px;
    border-bottom: 0.5px solid var(--phx-sep);
    font-size: 11.5px;
    font-weight: 600;
    color: var(--phx-text-3);
  }
  .brow {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
    align-items: center;
    padding: 12px 16px;
  }
  .brow + .brow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .bdate {
    font-size: 13px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 6px;
    justify-content: flex-end;
  }
  .bbtn {
    height: 26px;
    padding: 0 12px;
    border: none;
    border-radius: var(--phx-r-btn);
    cursor: pointer;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .bbtn.danger {
    background: transparent;
    color: var(--phx-text-3);
  }
  .bbtn.danger:hover {
    background: var(--phx-danger-bg);
    color: var(--phx-danger);
  }
  .empty {
    padding: 44px 0;
    text-align: center;
    color: var(--phx-text-3);
    font-size: 13px;
    line-height: 1.7;
  }
</style>
