<script lang="ts">
  import '../app.css'
  import { onMount } from 'svelte'
  import type { Snippet } from 'svelte'
  import { page } from '$app/state'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { listen } from '@tauri-apps/api/event'
  import PhxTitlebar from '$lib/components/PhxTitlebar.svelte'
  import PhxDock from '$lib/components/PhxDock.svelte'
  import ToastHost from '$lib/components/ToastHost.svelte'
  import Splash from '$lib/components/Splash.svelte'
  import Onboarding from '$lib/components/Onboarding.svelte'
  import { phxApp, phxAnnouncement, phxUpdate, PHX_UPDATE_EVENT } from '$lib/ipc'
  import type { PhxUpdateStatus } from '$lib/ipc'
  import { applyMotionClass } from '$lib/motion'
  import { toast } from '$lib/toast.svelte'
  import { t } from '$lib/i18n/i18n.svelte'

  let { children }: { children: Snippet } = $props()

  const appWindow = getCurrentWindow()

  /** bare 屏（01 服务器选择）全窗独立：无 dock、留白收窄 */
  const bare = $derived(page.route.id === '/server-select')

  /** 托盘面板窗（/tray）：无窗口框架（无标题栏/dock/首启编排），仅渲染页面本体 */
  const isTray = $derived(page.route.id === '/tray')

  /** 更新薄窗（/update-window）：同托盘走极简渲染，无 dock/无 toast 域 */
  const isUpdateWin = $derived(page.route.id === '/update-window')

  /** 薄窗合并判定：托盘面板 + 更新薄窗共用（跳过首启编排与公告 toast） */
  const isThin = $derived(isTray || isUpdateWin)

  /* 最大化联动圆角：.shell.maximized 让 app.css 去圆角规则生效。
     （PhxTitlebar 内另有一份按钮态监听，职责不同，互不干扰） */
  let maximized = $state(false)

  $effect(() => {
    let unlisten: (() => void) | null = null
    let disposed = false

    const sync = (): void => {
      void appWindow.isMaximized().then((v) => {
        maximized = v
      })
    }
    sync()
    void appWindow.onResized(sync).then((fn) => {
      if (disposed) fn()
      else unlisten = fn
    })

    return () => {
      disposed = true
      unlisten?.()
    }
  })

  /* 首启编排：Splash（首启 full ~2.9s / 回访 short ~1.5s）→ 引导页（仅首次） */
  let splashMode = $state<'' | 'full' | 'short'>('')
  let onboardVisible = $state(false)

  /* 主窗更新状态监听（薄窗不需要；unlisten 由 cleanup 兜底） */
  let unlistenUpdate: (() => void) | null = null

  onMount(() => {
    applyMotionClass() // 感官层基线：启动即挂减少动效偏好（托盘窗也要）
    if (isThin) return // 薄窗（托盘面板/更新窗）：不参与首启编排
    void phxApp.flags().then((flags) => {
      splashMode = flags.splashSeen ? 'short' : 'full'
      onboardVisible = !flags.onboarded
    })
    // 公告：启动拉取（三镜像/缓存回退/急停均在 Rust 侧），逐条 toast 展示
    void phxAnnouncement.fetch().then((payload) => {
      for (const a of payload.announcements) {
        const action = a.actionUrl
          ? { label: a.actionLabel || t('announce.viewDetail'), url: a.actionUrl }
          : undefined
        toast.announce(`${a.title}：${a.body}`, action)
      }
    })
    // 更新下载完成：全局 toast 提供立即重启（装包在 Rust 侧，退出后跑安装器）。
    // available 态不在此处理——Rust 侧检查到即自动开薄窗。
    let prevPhase = ''
    void listen<PhxUpdateStatus>(PHX_UPDATE_EVENT, (e) => {
      const s = e.payload
      if (s.phase === 'downloaded' && prevPhase !== 'downloaded') {
        toast.announce(t('update.readyToast', { version: s.version }), {
          label: t('update.restartNow'),
          url: '',
          onClick: () => {
            void phxUpdate.install().catch(() => {})
          }
        })
      }
      prevPhase = s.phase
    }).then((fn) => {
      unlistenUpdate = fn
    })
    return () => {
      unlistenUpdate?.()
    }
  })

  async function splashDone(): Promise<void> {
    splashMode = ''
    await phxApp.markSplashSeen()
  }

  async function onboardDone(): Promise<void> {
    onboardVisible = false
    await phxApp.markOnboarded()
  }
</script>

{#if isThin}
  <!-- 薄窗（托盘面板/更新窗）：无窗口框架，仅渲染页面本体 -->
  {@render children()}
{:else}
  <div class="shell" class:maximized>
    <PhxTitlebar />
    {#key page.url.pathname}
      <main class="main" class:bare>
        {@render children()}
      </main>
    {/key}
    {#if !bare}
      <PhxDock />
    {/if}
    <ToastHost />

    {#if splashMode}
      <Splash short={splashMode === 'short'} onDone={() => void splashDone()} />
    {:else if onboardVisible}
      <Onboarding onDone={() => void onboardDone()} />
    {/if}
  </div>
{/if}

<style>
  .shell {
    position: relative;
    display: flex;
    height: 100vh;
    background: var(--phx-win-bg);
    color: var(--phx-text);
    /* 透明窗口的视觉圆角由前端提供（Rust 侧 DWM 为系统级补充） */
    border-radius: var(--phx-r-win);
    overflow: hidden;
  }

  /* 最大化时窗口铺满屏幕，圆角归零（.sp/.ob 的联动在 app.css） */
  .shell.maximized {
    border-radius: 0;
  }

  /* 底部 92px 为悬浮 dock 留位（稿图 .main 规格） */
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding: 40px 0 92px;
    /* 页面入场：随 {#key} 每次导航重放（感官层基线 §二） */
    animation: phx-page-enter var(--phx-dur-norm) var(--phx-ease-out) both;
  }

  /* 入场只动 opacity + translateY；reduced-motion 双通道降级为静态 */
  @keyframes phx-page-enter {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  /* bare 屏（01 稿图 .content padding 原值） */
  .main.bare {
    padding: 50px 0 26px;
  }
</style>
