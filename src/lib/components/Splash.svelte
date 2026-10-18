<script lang="ts">
  // 启动页：凤凰环书写 + 对勾落定 + 信号线；点击/回车/超时退场。
  // 双时长：full ~2.9s（首启）/ short ~1.5s（回访），由父组件传 short 决定。
  // 样式承载在 app.css 的 .sp-* 段（样板同模式，组件不带 style 块）。
  import { t } from '$lib/i18n/i18n.svelte'

  let { short = false, onDone }: { short?: boolean; onDone: () => void } = $props()

  let exiting = $state(false)
  let gone = $state(false)

  $effect(() => {
    const totalMs = short ? 1500 : 2900
    const timer = setTimeout(startExit, totalMs)
    return () => clearTimeout(timer)
  })

  function startExit() {
    if (exiting) return
    exiting = true
    setTimeout(() => {
      gone = true
      onDone()
    }, 560)
  }
</script>

{#if !gone}
  <div
    class="sp"
    class:short
    class:exiting
    role="button"
    tabindex="-1"
    aria-label={t('sp.enter')}
    onclick={startExit}
    onkeydown={(e) => (e.key === 'Enter' || e.key === 'Escape') && startExit()}
  >
    <div class="sp-grid"></div>
    <div class="sp-vignette"></div>
    <div class="sp-noise"></div>
    <div class="sp-content">
      <svg class="sp-logo" viewBox="0 0 120 120">
        <defs>
          <linearGradient id="phx-spg" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" class="sp-stop-a" />
            <stop offset="1" class="sp-stop-b" />
          </linearGradient>
        </defs>
        <circle
          class="sp-stroke"
          cx="60"
          cy="60"
          r="44"
          fill="none"
          stroke="url(#phx-spg)"
          stroke-width="5"
          stroke-linecap="round"
          pathLength="100"
          transform="rotate(-90 60 60)"
        />
        <path
          class="sp-stroke late"
          d="M42 62 L55 75 L80 46"
          fill="none"
          stroke="var(--phx-text)"
          stroke-width="6"
          stroke-linecap="round"
          stroke-linejoin="round"
          pathLength="100"
        />
      </svg>
      <h1 class="sp-name">{t('app.title')}</h1>
      <div class="sp-signal"><i class="sp-signal-dot"></i></div>
      <p class="sp-sub">{t('sp.sub')}</p>
    </div>
    <div class="sp-enter">{t('sp.enter')}</div>
  </div>
{/if}
