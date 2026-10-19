<script lang="ts">
  // 13-issues 问题清单页：home 仪表台「问题检测」入口落点
  // 背板 = 警告条（实算计数）+ 说明卡（13 稿背板）+ 问题列表行内修复（13 稿注 4）
  import { onMount } from 'svelte'
  import { t } from '$lib/i18n/i18n.svelte'
  import { toast } from '$lib/toast.svelte'
  import { phxIssues, phxProfile, type PhxIssueRow } from '$lib/ipc'
  import { issueText } from '$lib/issueText'

  /** 骨架屏行序号生成 */
  const skRows = (n: number) => [...Array(n).keys()]

  let booting = $state(true)
  let loaded = $state(false)
  let profileId = $state('')
  let rows = $state<PhxIssueRow[]>([])
  let busy = $state(false)

  const count = $derived(rows.length)

  async function refresh(): Promise<void> {
    const id = await phxProfile.current()
    profileId = id ?? ''
    rows = profileId ? await phxIssues.scan(profileId) : []
  }

  onMount(() => {
    void refresh().finally(() => {
      loaded = true
      booting = false
    })
  })

  /** 行内修复：issues_fix 先备份后直接写盘，返回重扫结果替换列表；全部清空给轻提示 */
  async function fixOne(row: PhxIssueRow): Promise<void> {
    if (busy || !profileId) return
    busy = true
    try {
      const res = await phxIssues.fix(profileId, [row.id])
      rows = res.remaining
      if (rows.length === 0) toast.success(t('issues.toast.allFixed'))
    } catch {
      toast.error(t('issues.toast.failed'))
    } finally {
      busy = false
    }
  }
</script>

<main class="page">
  <header class="main-head">
    <div>
      <h1 class="large-title">{t('home.item.issues')}</h1>
      <p class="head-meta">{t('issues.meta')}</p>
    </div>
  </header>

  <div class="body">
    <div class="boot phx-boot" class:gone={!booting}>
      <div class="warn">
        <i class="phx-sk wic-sk"></i>
        <i class="phx-sk wtxt-sk"></i>
      </div>
      <div class="card">
        <i class="phx-sk ctitle-sk"></i>
        <i class="phx-sk cbody-sk"></i>
      </div>
      <div class="list">
        {#each skRows(6) as i (i)}
          <div class="irow">
            <i class="phx-sk itxt-sk"></i>
            <i class="phx-sk fix-sk"></i>
          </div>
        {/each}
      </div>
    </div>
    {#if loaded}
      <div class="scroll phx-fade-in">
        {#if count > 0}
        <div class="warn">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 2 20h20L12 3z" /><path d="M12 9v5" /><path d="M12 17.5v.5" /></svg>
          {t('issues.warnBar', { n: count })}
        </div>
      {/if}

      <div class="card">
        <div class="card-title">{t('issues.cardTitle')}</div>
        <div class="card-body">{t('issues.cardBody')}</div>
      </div>

      {#if count > 0}
        <div class="list">
          {#each rows as row (row.id)}
            <div class="irow">
              <div class="itxt">{issueText(row)}</div>
              <button type="button" class="fix" disabled={busy} onclick={() => fixOne(row)}>
                {t('issues.fix')}
              </button>
            </div>
          {/each}
        </div>
      {:else}
        <div class="empty">{t('issues.none')}</div>
      {/if}
    </div>
    {/if}
  </div>
</main>

<style>
  /* 容器三段式（对齐生产页惯例） */
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .main-head {
    padding: 24px 24px 0;
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
  .boot .wic-sk {
    width: 14px;
    height: 14px;
    border-radius: 4px;
    flex: none;
  }
  .boot .wtxt-sk {
    width: 180px;
    height: 12px;
    border-radius: 6px;
  }
  .boot .ctitle-sk {
    width: 120px;
    height: 13px;
    border-radius: 6px;
  }
  .boot .cbody-sk {
    width: 70%;
    height: 12px;
    border-radius: 6px;
    margin-top: 6px;
  }
  .boot .itxt-sk {
    flex: 1;
    height: 13px;
    border-radius: 6px;
  }
  .boot .fix-sk {
    width: 56px;
    height: 24px;
    border-radius: 7px;
    flex: none;
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
  /* 警告条（13 稿 .warn 原值） */
  .warn {
    margin: 16px 0 0;
    padding: 10px 14px;
    border-radius: 10px;
    background: var(--phx-warn-bg);
    border: 0.5px solid var(--phx-warn-bd);
    color: var(--phx-warn);
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .warn svg {
    width: 14px;
    height: 14px;
    flex: none;
  }
  /* 说明卡（13 稿背板原值） */
  .card {
    margin-top: 12px;
    background: var(--phx-surface-2);
    border-radius: 12px;
    box-shadow: inset 0 0 0 0.5px var(--phx-sep);
    padding: 14px 16px;
  }
  .card-title {
    font-size: 13px;
    font-weight: 600;
  }
  .card-body {
    font-size: 12px;
    color: var(--phx-text-2);
    margin-top: 6px;
    line-height: 1.7;
  }
  /* 问题列表：行样式与修复钮复用 13 稿 irow / fix 原值 */
  .list {
    margin-top: 16px;
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    padding: 4px 16px;
  }
  .irow {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 2px;
  }
  .irow + .irow {
    border-top: 0.5px solid var(--phx-sep);
  }
  .itxt {
    flex: 1;
    font-size: 12.5px;
    line-height: 1.5;
  }
  .fix {
    flex: none;
    height: 24px;
    padding: 0 12px;
    border: none;
    border-radius: 7px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    font-size: 11.5px;
    font-weight: 600;
    font-family: inherit;
    cursor: pointer;
  }
  .fix:hover {
    filter: brightness(1.2);
  }
  .fix:disabled {
    opacity: 0.5;
    cursor: default;
  }
  /* 无问题空态 */
  .empty {
    margin-top: 16px;
    padding: 22px 16px;
    text-align: center;
    font-size: 12.5px;
    color: var(--phx-text-3);
  }
</style>
