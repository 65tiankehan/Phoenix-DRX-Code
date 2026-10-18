<script lang="ts">
  import type { Snippet } from 'svelte'
  import { resolve } from '$app/paths'
  import { openUrl } from '@tauri-apps/plugin-opener'

  let {
    href,
    external = false,
    onpending,
    children
  }: {
    /** 链接地址：内链须传合法路由（调用方保证与路由表匹配）；外链传任意 URL；未定呈现占位态 */
    href?: string
    /** 新窗口打开外链 */
    external?: boolean
    onpending?: () => void
    children: Snippet
  } = $props()
</script>

<!--
  三分支拆分是 no-navigation-without-resolve 规则的唯一合规形态：
  条件表达式（consequent ? resolve() : fallback）中 fallback 的 string 类型不被认可；
  内链分支字面 resolve() 调用 + 外链分支经 opener 插件开系统浏览器 + 占位分支无 href。
  内链分支的 as any 断言：resolve 按各路由字面联合强类型（元组重载，泛型推断收窄），
  运行时字符串无法静态证明；契约由调用方保证传合法路由。
  外链分支的 target="_blank" rel="noopener external" 是本规则的豁免标记；实际打开不走新窗口——
  左键已被 preventDefault 拦下交 opener 插件（WebView2 会拦截新窗口），href 仅保留右键复制语义。
-->
{#if href && !external}
  <!-- resolve 的强类型只认路由字面量，运行时字符串动态路由场景无法规避 any，已由上方注释说明契约 -->
  <!-- eslint-disable-next-line @typescript-eslint/no-explicit-any -->
  <a class="lnk" href={resolve(href as any)}>{@render children()}</a>
{:else if href}
  {@const url = href}
  <a
    class="lnk"
    href={href}
    target="_blank"
    rel="noopener external"
    onclick={(e) => {
      e.preventDefault()
      void openUrl(url).catch(() => {})
    }}>{@render children()}</a
  >
{:else}
  <button type="button" class="lnk" onclick={() => onpending?.()}>{@render children()}</button>
{/if}

<style>
  /* 16 屏稿图原值：accent 色链接钮 */
  .lnk {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--phx-accent);
    text-decoration: none;
    flex: none;
    cursor: pointer;
  }
  .lnk:hover {
    text-decoration: underline;
  }
  /* 占位态是 button：重置原生外观与 .lnk 对齐 */
  button.lnk {
    background: none;
    border: none;
    padding: 0;
    font-family: inherit;
  }
</style>
