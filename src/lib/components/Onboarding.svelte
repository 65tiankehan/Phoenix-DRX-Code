<script lang="ts">
  // 引导页：3 页沉浸舞台——欢迎 / 安全三条件 / 就绪。可跳过。
  // 每一页讲的都已实现（诚实原则）：自动备份 / 多服务器多存档 / 17 入口仪表台。
  // 样式承载在 app.css 的 .ob-* 段（样板同模式，组件不带 style 块）。
  import { t } from '$lib/i18n/i18n.svelte'
  import { getCurrentWindow } from '@tauri-apps/api/window'

  let { onDone }: { onDone: () => void } = $props()

  // 无边框窗口整面拖动：按住非按钮区域即拖动窗口（按钮交给正常点击）
  function drag(e: MouseEvent) {
    if (e.button !== 0) return
    if ((e.target as HTMLElement | null)?.closest('button')) return
    void getCurrentWindow().startDragging()
  }

  let slide = $state(1)
  const total = 3

  function next() {
    if (slide < total) slide += 1
    else onDone()
  }
  function prev() {
    if (slide > 1) slide -= 1
  }
  function go(i: number) {
    slide = i
  }
</script>

<svelte:window onmousedown={drag} />

<div class="ob">
  <div class="ob-atmo"></div>
  <div class="ob-brand"><i></i>{t('ob.brand')}</div>
  <button class="ob-skip" onclick={onDone}>{t('ob.skip')}</button>
  <div class="ob-pagenum">{t('ob.pagenum', { n: slide, total })}</div>

  {#if slide === 1}
    <div class="ob-slide">
      <div class="ob-hero">
        <div class="ob-tile">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <rect x="4" y="4.5" width="16" height="15" rx="2" />
            <path d="M8 9.5h8M8 13h5" />
          </svg>
        </div>
        <div class="ob-badge ob-badge-a">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 3.5l2.2 4.6 5 .7-3.6 3.5.9 5-4.5-2.4-4.5 2.4.9-5L4.8 8.8l5-.7L12 3.5z" />
          </svg>
        </div>
        <div class="ob-badge ob-badge-b">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3.5 7a1.5 1.5 0 0 1 1.5-1.5h4l2 2h8A1.5 1.5 0 0 1 20.5 9v8A1.5 1.5 0 0 1 19 18.5H5A1.5 1.5 0 0 1 3.5 17V7z" />
          </svg>
        </div>
      </div>
      <h1 class="ob-title">{t('ob.s1.title')}</h1>
      <div class="ob-kicker">{t('ob.s1.kicker')}</div>
      <p class="ob-lead">{t('ob.s1.lead')}</p>
    </div>
  {:else if slide === 2}
    <div class="ob-slide">
      <div class="ob-hero">
        <div class="ob-tile">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 3l7 3v5c0 4.6-3 8.1-7 10-4-1.9-7-5.4-7-10V6l7-3z" />
          </svg>
        </div>
        <div class="ob-badge ob-badge-a">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <rect x="4" y="4" width="16" height="4" rx="1" />
            <path d="M6 8v9a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V8M10 12h4" />
          </svg>
        </div>
        <div class="ob-badge ob-badge-b">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <rect x="4" y="4.5" width="16" height="6" rx="1.5" />
            <rect x="4" y="13.5" width="16" height="6" rx="1.5" />
            <path d="M7.5 7.5h.01M7.5 16.5h.01" />
          </svg>
        </div>
      </div>
      <h1 class="ob-title">{t('ob.s2.title')}</h1>
      <div class="ob-kicker">{t('ob.s2.kicker')}</div>
      <ul class="ob-caps">
        <li class="ob-cap">
          <i></i>
          <div><b>{t('ob.s2.cap1.t')}</b><small>{t('ob.s2.cap1.d')}</small></div>
        </li>
        <li class="ob-cap">
          <i></i>
          <div><b>{t('ob.s2.cap2.t')}</b><small>{t('ob.s2.cap2.d')}</small></div>
        </li>
        <li class="ob-cap">
          <i></i>
          <div><b>{t('ob.s2.cap3.t')}</b><small>{t('ob.s2.cap3.d')}</small></div>
        </li>
      </ul>
    </div>
  {:else}
    <div class="ob-slide">
      <div class="ob-hero">
        <div class="ob-tile">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 12.5l4.5 4.5L19 7.5" />
          </svg>
        </div>
        <div class="ob-badge ob-badge-a">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <rect x="4" y="4" width="7" height="7" rx="1.5" />
            <rect x="13" y="4" width="7" height="7" rx="1.5" />
            <rect x="4" y="13" width="7" height="7" rx="1.5" />
            <rect x="13" y="13" width="7" height="7" rx="1.5" />
          </svg>
        </div>
        <div class="ob-badge ob-badge-b">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3.5 7a1.5 1.5 0 0 1 1.5-1.5h4l2 2h8A1.5 1.5 0 0 1 20.5 9v8A1.5 1.5 0 0 1 19 18.5H5A1.5 1.5 0 0 1 3.5 17V7z" />
          </svg>
        </div>
      </div>
      <h1 class="ob-title">{t('ob.s3.title')}</h1>
      <div class="ob-kicker">{t('ob.s3.kicker')}</div>
      <p class="ob-lead">{t('ob.s3.lead')}</p>
    </div>
  {/if}

  <div class="ob-foot">
    <div class="ob-dots">
      {#each [...Array(total).keys()] as i (i)}
        <button
          class="ob-dot"
          class:on={slide === i + 1}
          aria-label={String(i + 1)}
          onclick={() => go(i + 1)}
        ></button>
      {/each}
    </div>
    <div class="ob-actions">
      {#if slide > 1}
        <button class="ob-ghost" onclick={prev}>{t('ob.prev')}</button>
      {/if}
      <button class="ob-primary" onclick={next}>
        {slide === total ? t('ob.finish') : t('ob.next')}
      </button>
    </div>
  </div>
</div>
