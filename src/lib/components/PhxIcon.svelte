<script lang="ts">
  // 静态图标注入：markup 仅来自各屏内置 ICON 常量，非用户输入，无 XSS 面
  // size 可选：给了就输出宽高，不给则由外层 CSS 决定（既有页面依赖这一点）
  let { markup, size }: { markup: string; size?: number } = $props()
</script>

<svg
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  width={size}
  height={size}
  ><!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html markup}
</svg>
