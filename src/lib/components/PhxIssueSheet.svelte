<script lang="ts">
  import { tick } from 'svelte'
  import { playSound } from '$lib/sound'
  import { t } from '$lib/i18n/i18n.svelte'
  import { issueText } from '$lib/issueText'
  import { phxIssues, phxSettings, type PhxIssueRow } from '$lib/ipc'
  import PhxCheckbox from './PhxCheckbox.svelte'
  import PhxTooltip from './PhxTooltip.svelte'

  let {
    open = $bindable(false),
    profileId,
    issues = [],
    /** 结束回调：save = 忽略/修复完可以继续保存；cancel = 放弃本次保存 */
    onresolve
  }: {
    open?: boolean
    profileId: string
    issues?: PhxIssueRow[]
    onresolve: (action: 'save' | 'cancel') => void
  } = $props()

  let rows = $state<PhxIssueRow[]>([])
  let remember = $state(false)
  let busy = $state(false)
  let ignoreBtn: HTMLButtonElement | undefined = $state()

  // 每次打开重置为传入快照（父级在 scan 后才置 open）
  $effect(() => {
    if (open) {
      rows = [...issues]
      remember = false
      busy = false
      playSound('pop')
      void tick().then(() => ignoreBtn?.focus())
    }
  })

  function finish(action: 'ignore' | 'fix'): void {
    if (remember) {
      // 「设置以上为默认选择」：把本次动作写回全局偏好，下次保存不再询问
      void phxSettings
        .get()
        .then((s) => phxSettings.set({ ...s, issueHandling: action }))
        .catch(() => {})
    }
    onresolve('save')
  }

  async function fixOne(row: PhxIssueRow): Promise<void> {
    if (busy) return
    busy = true
    try {
      const res = await phxIssues.fix(profileId, [row.id])
      rows = res.remaining
      if (rows.length === 0) finish('fix') // 重扫为空自动关窗并继续保存
    } catch {
      rows = [...rows] // 修复失败保留原行，弹窗不关
    } finally {
      busy = false
    }
  }

  async function fixAll(): Promise<void> {
    if (busy || rows.length === 0) return
    busy = true
    try {
      const res = await phxIssues.fix(
        profileId,
        rows.map((r) => r.id)
      )
      rows = res.remaining
      if (rows.length === 0) finish('fix')
    } catch {
      rows = [...rows]
    } finally {
      busy = false
    }
  }

  function cancel(): void {
    if (busy) return
    onresolve('cancel')
  }

  function onKeydown(e: KeyboardEvent): void {
    if (!open) return
    if (e.key === 'Escape') {
      e.preventDefault()
      cancel()
    }
  }

  /** 挂到 body 下，避免被父级 overflow/z-index 裁剪 */
  function portal(node: HTMLElement): void {
    document.body.appendChild(node)
  }
</script>

<svelte:window onkeydown={onKeydown} />

{#if open}
  <!-- 保存拦截弹窗（13 稿：620px sheet，z 层对齐 PhxDialog 同用 modal 层） -->
  <div
    use:portal
    class="ovl"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) cancel()
    }}
  >
    <div class="sheet" role="alertdialog" aria-modal="true" aria-label={t('issues.sheetTitle')}>
      <div class="shead">
        <span class="stitle">{t('issues.sheetTitle')}</span>
        <span class="ssub">
          {rows.length > 0 ? t('issues.count', { n: rows.length }) : t('issues.none')}
        </span>
        <PhxTooltip label={t('cap.close')}>
          <button type="button" class="sx" aria-label={t('cap.close')} onclick={cancel}>
            <svg viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" fill="none" stroke-width="1.4" /></svg>
          </button>
        </PhxTooltip>
      </div>
      <div class="sbody">
        {#each rows as row (row.id)}
          <div class="irow">
            <div class="itxt">{issueText(row)}</div>
            <button type="button" class="fix" disabled={busy} onclick={() => fixOne(row)}>
              {t('issues.fix')}
            </button>
          </div>
        {/each}
        <div class="ckline">
          <PhxCheckbox bind:checked={remember} label={t('issues.remember')} />
        </div>
      </div>
      <div class="sfoot">
        <button
          bind:this={ignoreBtn}
          type="button"
          class="fbtn ghost"
          disabled={busy}
          onclick={() => finish('ignore')}
        >
          {t('issues.ignoreAll')}
        </button>
        <button type="button" class="fbtn" disabled={busy || rows.length === 0} onclick={fixAll}>
          {t('issues.fixAll')}
        </button>
        <button type="button" class="fbtn ghost" disabled={busy} onclick={cancel}>
          {t('dlg.cancel')}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .ovl {
    position: fixed;
    inset: 0;
    z-index: var(--phx-z-modal);
    background: rgba(0, 0, 0, 0.55);
    backdrop-filter: blur(2px);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .sheet {
    width: 620px;
    max-height: 660px;
    display: flex;
    flex-direction: column;
    background: var(--phx-surface);
    border: 0.5px solid var(--phx-sep-strong);
    border-radius: 14px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.55);
  }
  .shead {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px 18px 12px;
  }
  .stitle {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.1px;
  }
  .ssub {
    font-size: 12px;
    color: var(--phx-text-3);
  }
  .sx {
    margin-left: auto;
    width: 28px;
    height: 28px;
    border: none;
    border-radius: 8px;
    background: var(--phx-fill-tertiary);
    color: var(--phx-text-2);
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .sx:hover {
    background: var(--phx-fill-quaternary);
  }
  .sx svg {
    width: 10px;
    height: 10px;
  }
  .sbody {
    flex: 1;
    overflow-y: auto;
    padding: 2px 18px 4px;
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
  .ckline {
    margin: 10px 0 12px;
  }
  .sfoot {
    display: flex;
    gap: 8px;
    padding: 12px 18px 16px;
    border-top: 0.5px solid var(--phx-sep);
  }
  .fbtn {
    height: 32px;
    padding: 0 16px;
    border: none;
    border-radius: var(--phx-r-btn);
    cursor: pointer;
    font-size: 12.5px;
    font-weight: 600;
    font-family: inherit;
    background: var(--phx-accent);
    color: #fff;
  }
  .fbtn:hover {
    filter: brightness(1.1);
  }
  .fbtn.ghost {
    background: var(--phx-fill-tertiary);
    color: var(--phx-text);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
  }
  .fbtn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
