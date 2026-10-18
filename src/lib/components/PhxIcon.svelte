<script lang="ts">
  // 静态图标注入：markup 仅来自各屏内置 ICON 常量，非用户输入，无 XSS 面
  let { markup }: { markup: string } = $props()
</script>

<svg
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  ><!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html markup}
</svg>
