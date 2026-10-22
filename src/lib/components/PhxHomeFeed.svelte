<script lang="ts">
  /**
   * 首页动态条（25 稿）：hero 下 span4 窄行，轮播 公告 / 小贴士 / 赞助。
   * 赞助红线：同一时刻最多 1 条；不自动弹窗、不点亮铃铛未读；外链系统浏览器打开且必带「推广」角标；
   * 设置页「显示赞助推荐」关闭后槽位回退公告/小贴士；系统减动效时停止自动轮换仅手动切换。
   */
  import { onMount } from 'svelte'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import { t, type MessageKey } from '$lib/i18n/i18n.svelte'
  import { announcements, toggleAnnouncements } from '$lib/announcements.svelte'
  import { phxSettings } from '$lib/ipc'
  import { isReducedMotion, onMotionChange } from '$lib/motion'

  const INTERVAL_MS = 8000

  /** 赞助推荐展示开关（设置页「显示赞助推荐」，默认开；拉取失败按默认值回退） */
  let showSponsor = $state(true)
  let reduced = $state(isReducedMotion())
  let idx = $state(0)
  let timer: ReturnType<typeof setInterval> | null = null

  onMotionChange((v) => (reduced = v))

  /** 轮播序列：公告全部 → 小贴士全部 → 赞助最多 1 条（红线：同一时刻最多 1 条） */
  const items = $derived.by(() => {
    const list = announcements.list
    const seq = [...list.filter((a) => a.kind === 'announce'), ...list.filter((a) => a.kind === 'tip')]
    if (showSponsor) {
      const sp = list.find((a) => a.kind === 'sponsor')
      if (sp) seq.push(sp)
    }
    return seq
  })

  const cur = $derived(items[idx])

  // 序列变化（公告异步到达 / 开关切换）时夹住当前下标
  $effect(() => {
    if (idx >= items.length) idx = 0
  })

  // 自动轮换：减动效或单条时不自动，仅手动切换
  $effect(() => {
    if (reduced || items.length < 2) return
    timer = setInterval(() => (idx = (idx + 1) % items.length), INTERVAL_MS)
    return () => {
      if (timer) {
        clearInterval(timer)
        timer = null
      }
    }
  })

  onMount(() => {
    void phxSettings
      .get()
      .then((s) => (showSponsor = s.showSponsor))
      .catch(() => {})
  })

  const tagKey: MessageKey = $derived(
    cur?.kind === 'tip' ? 'announce.tipTag' : cur?.kind === 'sponsor' ? 'announce.sponsored' : 'announce.panelTitle'
  )

  /** 点击条目：有外链走系统浏览器，否则打开公告面板 */
  function onClick(): void {
    if (!cur) return
    if (cur.actionUrl) void openUrl(cur.actionUrl).catch(() => {})
    else toggleAnnouncements()
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      onClick()
    }
  }

  function next(e: MouseEvent): void {
    e.stopPropagation()
    if (items.length < 2) return
    idx = (idx + 1) % items.length
  }
</script>

{#if cur}
  <div class="feed" style="grid-column: span 4" role="button" tabindex="0" onclick={onClick} onkeydown={onKey}>
    <span class="feed-tag {cur.kind}">{t(tagKey)}</span>
    <span class="feed-text"><b>{cur.title}</b> — {cur.body}</span>
    {#if items.length > 1}
      <span class="feed-dots" aria-hidden="true">
        {#each items as it, i (it.id)}
          <i class:on={i === idx}></i>
        {/each}
      </span>
      <button type="button" class="feed-arr" aria-label={t('home.feed.next')} onclick={next}>›</button>
    {/if}
  </div>
{/if}

<style>
  .feed {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 12px;
    border-radius: 10px;
    background: var(--phx-surface);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep);
    cursor: pointer;
    user-select: none;
  }
  .feed:hover {
    background: var(--phx-surface-2);
  }
  .feed-tag {
    flex: none;
    height: 18px;
    padding: 0 7px;
    border-radius: 5px;
    font-size: 10px;
    font-weight: 700;
    display: flex;
    align-items: center;
  }
  .feed-tag.announce {
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }
  .feed-tag.tip {
    background: var(--phx-green-dim);
    color: var(--phx-green);
  }
  .feed-tag.sponsor {
    background: var(--phx-warn-bg);
    color: var(--phx-warn);
    box-shadow: inset 0 0 0 0.5px var(--phx-warn-bd);
  }
  .feed-text {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    color: var(--phx-text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .feed-text b {
    color: var(--phx-text);
    font-weight: 600;
  }
  .feed-dots {
    flex: none;
    display: flex;
    gap: 4px;
  }
  .feed-dots i {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--phx-fill-quaternary);
  }
  .feed-dots i.on {
    background: var(--phx-text-3);
  }
  .feed-arr {
    flex: none;
    color: var(--phx-text-3);
    font-size: 14px;
    line-height: 1;
  }
</style>
