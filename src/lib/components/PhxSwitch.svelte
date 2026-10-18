<script lang="ts">
  import { playSound } from '$lib/sound'

  let {
    checked = $bindable(false),
    label = '',
    disabled = false,
    size = 'md'
  }: {
    /** 开关状态 */
    checked?: boolean
    /** 无障碍名称 */
    label?: string
    disabled?: boolean
    /** 尺寸变体：md=40×24（默认）/ lg=51×31（04 商人屏稿图规格） */
    size?: 'md' | 'lg'
  } = $props()

  function toggle(): void {
    if (!disabled) {
      playSound('toggle')
      checked = !checked
    }
  }

  function onKeydown(e: KeyboardEvent): void {
    if (disabled) return
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      toggle()
    }
  }
</script>

<div
  class="sw"
  class:on={checked}
  class:disabled
  class:lg={size === 'lg'}
  role="switch"
  aria-checked={checked}
  aria-label={label || undefined}
  aria-disabled={disabled || undefined}
  tabindex={disabled ? -1 : 0}
  onclick={toggle}
  onkeydown={onKeydown}
></div>

<style>
  /* 15 屏稿图原值：40×24 轨道 + 18px 滑块 */
  .sw {
    width: 40px;
    height: 24px;
    border-radius: 12px;
    flex: none;
    cursor: pointer;
    background: var(--phx-fill-quaternary);
    box-shadow: inset 0 0 0 0.5px var(--phx-sep-strong);
    position: relative;
    transition: background var(--phx-dur-fast);
  }
  .sw::after {
    content: '';
    position: absolute;
    top: 3px;
    left: 3px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.92);
    transition: transform var(--phx-dur-fast);
  }
  .sw.on {
    background: var(--phx-accent);
    box-shadow: none;
  }
  .sw.on::after {
    transform: translateX(16px);
  }
  /* 三态（视觉基线 §四）：hover 提亮 / active 压暗 / disabled 置灰 */
  .sw:not(.disabled):hover {
    filter: brightness(1.15);
  }
  .sw:not(.disabled):active {
    filter: brightness(0.92);
  }
  .sw.disabled {
    opacity: 0.45;
    cursor: default;
    filter: none;
  }
  /* 04 商人屏稿图原值：51×31 轨道 + 27px 白球（带投影），off 底 fill-tertiary */
  .sw.lg {
    width: 51px;
    height: 31px;
    border-radius: 16px;
    background: var(--phx-fill-tertiary);
    transition: background 0.2s;
  }
  .sw.lg::after {
    top: 2px;
    left: 2px;
    width: 27px;
    height: 27px;
    background: #fff;
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.35);
    transition: transform 0.2s;
  }
  .sw.lg.on::after {
    transform: translateX(20px);
  }
</style>
