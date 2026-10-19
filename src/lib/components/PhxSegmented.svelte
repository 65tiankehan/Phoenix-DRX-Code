<script lang="ts">
  import { playSound } from '$lib/sound'

  let {
    options,
    value = $bindable(),
    ariaLabel = '',
    size = 'sm',
    role,
    onchange
  }: {
    /** 选项集 */
    options: { value: string; label: string }[]
    /** 当前选中值 */
    value?: string
    /** 无障碍名称 */
    ariaLabel?: string
    /** 尺寸档：sm=12px 紧凑档（8 页统一档）/ md=28px 高大档（character 主档，稿图原值） */
    size?: 'sm' | 'md'
    /** 容器语义角色（tablist / radiogroup） */
    role?: string
    /** 选中值变化回调（value 赋值后调用；仅 bind:value 不够时用，如切换需联动副作用） */
    onchange?: (v: string) => void
  } = $props()

  let wrap = $state<HTMLDivElement>()
  let btns = $state<HTMLButtonElement[]>([])
  /** 滑块几何：null = 未完成首测（不渲染，元素插入即终态，天然无从 0 滑入的跳变） */
  let ind = $state<{ left: number; width: number } | null>(null)

  function measure(): void {
    const el = btns[options.findIndex((o) => o.value === value)]
    if (!el) {
      ind = null
      return
    }
    ind = { left: el.offsetLeft, width: el.offsetWidth }
  }

  // 选中值 / 选项集变化时重测定位（measure 内同步读取自动追踪）
  $effect(() => {
    measure()
  })

  // 容器尺寸变化（窗口缩放等）时防止滑块漂移
  $effect(() => {
    if (!wrap) return
    const ro = new ResizeObserver(measure)
    ro.observe(wrap)
    return () => ro.disconnect()
  })

  function pick(v: string): void {
    if (v !== value) {
      playSound('ui')
      value = v
      onchange?.(v)
    }
  }

  /** 鼠标跟随光源：写入相对坐标供按钮 ::after 光斑定位 */
  function spotlight(e: MouseEvent): void {
    const t = e.currentTarget as HTMLElement
    const r = t.getBoundingClientRect()
    t.style.setProperty('--mx', `${e.clientX - r.left}px`)
    t.style.setProperty('--my', `${e.clientY - r.top}px`)
  }
</script>

<div class="seg {size}" {role} aria-label={ariaLabel || undefined} bind:this={wrap}>
  {#if ind}
    <div
      class="ind"
      style:transform={`translateX(${ind.left}px)`}
      style:width={`${ind.width}px`}
    ></div>
  {/if}
  {#each options as o, i (o.value)}
    <button
      type="button"
      class:on={o.value === value}
      bind:this={btns[i]}
      onclick={() => pick(o.value)}
      onmousemove={spotlight}
    >
      {o.label}
    </button>
  {/each}
</div>

<style>
  .seg {
    position: relative;
    display: inline-flex;
    padding: 2px;
    border-radius: 9px;
    background: var(--phx-fill-tertiary);
  }
  .seg.md {
    display: flex;
  }
  /* 液体滑块：{#if} 插入即终态；仅选中值变化时产生过渡（--phx-dur-norm + emphasized，感官层基线档位） */
  .ind {
    position: absolute;
    top: 2px;
    bottom: 2px;
    left: 0;
    border-radius: 7px;
    pointer-events: none;
    transition:
      transform var(--phx-dur-norm) var(--phx-ease-emphasized),
      width var(--phx-dur-norm) var(--phx-ease-emphasized);
  }
  /* 稿图原值：sm 档 rgba(255,255,255,.16) / md 档 rgba(255,255,255,.27)，无对应 token 沿用先例 */
  .seg.sm .ind {
    background: rgba(255, 255, 255, 0.16);
  }
  .seg.md .ind {
    background: rgba(255, 255, 255, 0.27);
  }

  .seg button {
    position: relative;
    overflow: hidden;
    z-index: 1;
    border: none;
    background: transparent;
    color: var(--phx-text-2);
    font-family: inherit;
    font-weight: 500;
    border-radius: 7px;
    cursor: pointer;
    transition: color var(--phx-dur-fast) var(--phx-ease-out);
  }
  .seg.sm button {
    font-size: 12px;
    padding: 5px 14px;
  }
  /* --seg-pad-x 出口：BEAR/USEC 分段稿图原值 22px（character 页覆盖），默认 18px */
  .seg.md button {
    height: 28px;
    padding: 0 var(--seg-pad-x, 18px);
    font-size: 13px;
  }
  .seg.sm button.on {
    color: var(--phx-text);
    font-weight: 600;
  }
  .seg.md button.on {
    color: #fff;
    font-weight: 600;
  }

  /* hover 光源（跟随鼠标的光斑，160px 半径白 8%）：--mx/--my 由 spotlight 写入 */
  .seg button::after {
    content: '';
    position: absolute;
    inset: 0;
    pointer-events: none;
    border-radius: inherit;
    background: radial-gradient(
      160px circle at var(--mx, 50%) var(--my, 50%),
      rgba(255, 255, 255, 0.08),
      transparent 60%
    );
    opacity: 0;
    transition: opacity var(--phx-dur-norm) var(--phx-ease-out);
  }
  .seg button:hover::after {
    opacity: 1;
  }
</style>
