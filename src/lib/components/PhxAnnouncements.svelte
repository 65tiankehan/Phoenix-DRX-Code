<script lang="ts">
  import { t } from '$lib/i18n/i18n.svelte'
  import PhxLink from './PhxLink.svelte'
  import { announcements, closeAnnouncements } from '$lib/announcements.svelte'
</script>

<!--
  18 稿图生产实现：居中模态公告面板（z-modal 层，主窗专属）。
  关闭出口四合一：面板关闭钮 / 「知道了」/ Esc / 遮罩点击，均走 closeAnnouncements（含全量标记已读）。
  稿图偏离说明：公告列表未画「相对时间」——Rust AnnouncementItem 无时间字段（诚实原则，不放假数据）。
-->
<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape' && announcements.open) closeAnnouncements()
  }}
/>

{#if announcements.open}
  <!-- 遮罩点击关闭是弹层标准交互（role=presentation + target/currentTarget 对齐 PhxDialog 先例） -->
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) closeAnnouncements()
    }}
  >
    <div class="panel" role="dialog" aria-modal="true" aria-label={t('announce.panelTitle')}>
      <header class="panel-head">
        <div class="panel-title-wrap">
          <h2 class="panel-title">{t('announce.panelTitle')}</h2>
          {#if announcements.source === 'remote'}
            <span class="src-badge src-remote"><i></i>{t('announce.srcRemote')}</span>
          {:else if announcements.source === 'cache'}
            <span class="src-badge src-cache"><i></i>{t('announce.srcCache')}</span>
          {/if}
        </div>
        <button
          type="button"
          class="panel-close"
          aria-label={t('announce.closeBtn')}
          onclick={closeAnnouncements}
        >
          <svg viewBox="0 0 10 10">
            <path
              d="M0 0l10 10M10 0L0 10"
              stroke="currentColor"
              stroke-width="1.2"
              fill="none"
              stroke-linecap="round"
            />
          </svg>
        </button>
      </header>

      {#if announcements.list.length}
        <div class="panel-list">
          {#each announcements.list as a (a.id)}
            <article class="ann">
              <div class="ann-head">
                <span class="ann-unread" class:read={announcements.readIds.has(a.id)}></span>
                <h3 class="ann-title">{a.title}</h3>
              </div>
              <p class="ann-body">{a.body}</p>
              {#if a.actionUrl}
                <span class="ann-link">
                  <PhxLink href={a.actionUrl} external>
                    {a.actionLabel || t('announce.viewDetail')}
                    <svg viewBox="0 0 10 10" aria-hidden="true">
                      <path
                        d="M3 1l4 4-4 4"
                        stroke="currentColor"
                        stroke-width="1.2"
                        fill="none"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      />
                    </svg>
                  </PhxLink>
                </span>
              {/if}
            </article>
          {/each}
        </div>
      {:else}
        <div class="panel-list panel-empty">
          <svg
            viewBox="0 0 32 32"
            fill="none"
            stroke="currentColor"
            stroke-width="1.4"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path
              d="M16 4.5a8.5 8.5 0 0 0-8.5 8.5c0 6.5-2.5 9-2.5 9h22s-2.5-2.5-2.5-9A8.5 8.5 0 0 0 16 4.5Z"
            />
            <path d="M13.6 26a2.6 2.6 0 0 0 4.8 0" />
          </svg>
          <p class="empty-title">{t('announce.empty')}</p>
          <p class="empty-hint">{t('announce.emptyHint')}</p>
        </div>
      {/if}

      <footer class="panel-foot">
        <span class="foot-hint">{t('announce.closeHint')}</span>
        <button type="button" class="btn-primary" onclick={closeAnnouncements}>
          {announcements.list.length ? t('announce.knowBtn') : t('announce.closeBtn')}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  /* 18 稿图原值：遮罩 + 520px 模态卡片；入场动画照 phx-page-enter 惯例（挂载即播，both） */
  .overlay {
    position: absolute;
    inset: 0;
    z-index: var(--phx-z-modal);
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.5);
    animation: ann-overlay-in var(--phx-dur-norm) var(--phx-ease-out) both;
  }

  @keyframes ann-overlay-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .panel {
    width: 520px;
    max-height: 560px;
    display: flex;
    flex-direction: column;
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    box-shadow: var(--phx-shadow-win);
    border: 1px solid var(--phx-sep);
    overflow: hidden;
    animation: ann-panel-in var(--phx-dur-norm) var(--phx-ease-emphasized) both;
  }

  @keyframes ann-panel-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .panel-head {
    flex: none;
    height: 52px;
    display: flex;
    align-items: center;
    padding: 0 10px 0 20px;
    border-bottom: 1px solid var(--phx-sep);
  }

  .panel-title-wrap {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }

  .panel-title {
    font-size: 17px;
    font-weight: 600;
    color: var(--phx-text);
  }

  .src-badge {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    line-height: 1;
    padding: 4px 8px;
    border-radius: 999px;
  }

  .src-badge i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
  }

  .src-remote {
    background: var(--phx-green-dim);
    color: var(--phx-green);
  }

  .src-remote i {
    background: var(--phx-green);
  }

  .src-cache {
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }

  .src-cache i {
    background: var(--phx-accent);
  }

  .panel-close {
    margin-left: auto;
    width: 36px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: 0;
    border-radius: var(--phx-r-btn);
    color: var(--phx-text-2);
    cursor: default;
    font: inherit;
    transition:
      background 0.12s,
      color 0.12s;
  }

  .panel-close svg {
    width: 10px;
    height: 10px;
    display: block;
  }

  .panel-close:hover {
    background: var(--phx-fill-quaternary);
    color: var(--phx-text);
  }

  .panel-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 14px 16px;
    scrollbar-width: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .panel-list::-webkit-scrollbar {
    display: none;
  }

  .ann {
    flex: none;
    background: var(--phx-surface-2);
    border-radius: var(--phx-r-card);
    padding: 14px 16px 12px;
  }

  .ann-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .ann-unread {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--phx-accent);
  }

  .ann-unread.read {
    background: none;
  }

  .ann-title {
    font-size: 14.5px;
    font-weight: 600;
    color: var(--phx-text);
  }

  .ann-body {
    margin-top: 8px;
    font-size: 13px;
    line-height: 1.7;
    color: var(--phx-text-2);
  }

  /* 查看详情链接：PhxLink 的 .lnk 是行内文本，这里套 flex 排箭头 */
  .ann-link {
    margin-top: 10px;
    display: inline-flex;
  }

  .ann-link :global(.lnk) {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-weight: 500;
  }

  .ann-link :global(.lnk:hover) {
    text-underline-offset: 3px;
  }

  .ann-link :global(svg) {
    width: 9px;
    height: 9px;
    display: block;
  }

  .panel-foot {
    flex: none;
    height: 60px;
    display: flex;
    align-items: center;
    padding: 0 16px 0 20px;
    border-top: 1px solid var(--phx-sep);
  }

  .foot-hint {
    font-size: 12px;
    color: var(--phx-text-3);
  }

  .btn-primary {
    margin-left: auto;
    height: 32px;
    padding: 0 18px;
    background: var(--phx-accent);
    color: #fff;
    border: 0;
    border-radius: var(--phx-r-btn);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: default;
    transition:
      background 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
  }

  .btn-primary:hover {
    background: #24a1ff;
  }

  .btn-primary:active {
    transform: scale(0.97);
  }

  /* 空态：铃铛线稿 + 双行文案（覆盖 .panel-list 的纵向布局为居中） */
  .panel-empty {
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 28px 16px;
    color: var(--phx-text-3);
  }

  .panel-empty svg {
    width: 32px;
    height: 32px;
    color: var(--phx-text-3);
  }

  .empty-title {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--phx-text-2);
  }

  .empty-hint {
    font-size: 12px;
    color: var(--phx-text-3);
  }
</style>
