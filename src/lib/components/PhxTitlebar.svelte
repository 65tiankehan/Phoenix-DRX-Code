<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { t } from '$lib/i18n/i18n.svelte'

  const appWindow = getCurrentWindow()
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
</script>

<header class="titlebar" data-tauri-drag-region>
  <span class="tb-label" data-tauri-drag-region>Phoenix</span>
  <div class="capbtns">
    <button type="button" aria-label={t('cap.minimize')} onclick={() => void appWindow.minimize()}>
      <svg viewBox="0 0 10 10"><path d="M0 5h10" stroke="currentColor" fill="none" /></svg>
    </button>
    <button
      type="button"
      aria-label={maximized ? t('cap.restore') : t('cap.maximize')}
      onclick={() => void appWindow.toggleMaximize()}
    >
      {#if maximized}
        <svg viewBox="0 0 10 10">
          <rect x=".5" y="2.5" width="7" height="7" stroke="currentColor" fill="none" />
          <path d="M2.5 2.5V.5h7v7" stroke="currentColor" fill="none" />
        </svg>
      {:else}
        <svg viewBox="0 0 10 10">
          <rect x=".5" y=".5" width="9" height="9" stroke="currentColor" fill="none" />
        </svg>
      {/if}
    </button>
    <button type="button" class="cap-close" aria-label={t('cap.close')} onclick={() => void appWindow.close()}>
      <svg viewBox="0 0 10 10">
        <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" fill="none" />
      </svg>
    </button>
  </div>
</header>

<style>
  .titlebar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 40px;
    z-index: var(--phx-z-header);
    display: flex;
    align-items: center;
    padding: 0 0 0 18px;
  }

  .tb-label {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--phx-text-2);
  }

  .capbtns {
    display: flex;
    align-self: stretch;
    margin-left: auto;
  }

  .capbtns button {
    width: 46px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--phx-text-2);
    background: none;
    border: 0;
    font: inherit;
    transition:
      background 0.12s,
      color 0.12s,
      transform var(--phx-dur-fast) var(--phx-ease-out);
    cursor: default;
  }

  .capbtns button svg {
    width: 10px;
    height: 10px;
    display: block;
  }

  .capbtns button:hover {
    background: rgba(255, 255, 255, 0.06);
    color: var(--phx-text);
  }

  .capbtns button.cap-close:hover {
    background: #c42b1c;
    color: #fff;
  }
</style>
