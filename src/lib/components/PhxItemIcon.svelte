<script lang="ts">
  /**
   * 物品图标位：有图显示图、无图显示首字色块。
   *
   * 为什么要有兜底：CDN 只覆盖真实物品（容器类如「仓库/Pockets」和新版本物品确实没图），
   * 且玩家可能离线。图标永远不阻塞渲染 —— 挂载即出色块，图到货再替换。
   */
  import { iconSrc, wantIcon } from '$lib/icons.svelte'

  let {
    tpl,
    name = '',
    size = 'md',
    square = false
  }: {
    /** 物品模板 id（24 位十六进制）；为空则直接走色块 */
    tpl?: string | null
    /** 物品名，取首字做兜底色块 */
    name?: string
    /** 尺寸档：sm=24 / md=34 / lg=44 */
    size?: 'sm' | 'md' | 'lg'
    /** 圆角方块（列表行）与圆形（商人头像式）之外统一圆角 */
    square?: boolean
  } = $props()

  const src = $derived(iconSrc(tpl))

  $effect(() => {
    if (tpl) wantIcon(tpl)
  })

  const letter = $derived((name || '?').trim().charAt(0).toUpperCase())
</script>

<span class="ico {size}" class:square>
  {#if src}
    <img src={src} alt="" loading="lazy" decoding="async" />
  {:else}
    <span class="ph">{letter}</span>
  {/if}
</span>

<style>
  .ico {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    border-radius: 8px;
    /* 图标本身是深底商品照，铺一层极浅内衬，浅色主题下不会显得「一块块黑」 */
    background: var(--phx-surface-3);
    overflow: hidden;
  }
  .ico.sm {
    width: 24px;
    height: 24px;
  }
  .ico.md {
    width: 34px;
    height: 34px;
  }
  .ico.lg {
    width: 44px;
    height: 44px;
  }
  .ico.square {
    border-radius: 6px;
  }
  .ico img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    /* CDN 图标是商品照抠图，枪械类本体偏黑；暗色主题下提亮一档才看得清轮廓 */
    filter: brightness(1.35) saturate(1.05);
  }
  .ph {
    font-size: 12px;
    font-weight: 700;
    color: var(--phx-text-3);
    line-height: 1;
  }
  .ico.lg .ph {
    font-size: 15px;
  }
</style>
