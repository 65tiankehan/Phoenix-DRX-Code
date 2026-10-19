<script lang="ts">
  import { onMount } from 'svelte'
  import { getVersion } from '@tauri-apps/api/app'
  import { openUrl } from '@tauri-apps/plugin-opener'
  import { t } from '$lib/i18n/i18n.svelte'
  import PhxLink from '$lib/components/PhxLink.svelte'

  // 真实外链：开发者主页 / 发布页 / 项目仓库 / Issues
  const HOME_URL = 'https://github.com/65tiankehan'
  const RELEASES_URL = 'https://github.com/65tiankehan/Phoenix-DRX-releases/releases'
  const REPO_URL = 'https://github.com/65tiankehan/Phoenix-DRX-Code'
  const SPT_URL = 'https://sp-tarkov.com'
  const SKITLES_URL = 'https://github.com/SkiTles55/SPT-AKI-Profile-Editor'

  // 版本号从应用信息读取，不硬编码
  let version = $state('')

  onMount(() => {
    void getVersion().then((v) => (version = v))
  })
</script>

<main class="page">
  <header class="main-head">
    <div>
      <h1 class="large-title">{t('nav.about')}</h1>
      <p class="head-meta">{t('about.headMeta')}</p>
    </div>
  </header>

  <div class="scroll">
    <div class="sgrid">
      <div class="scol">
        <!-- 卡 1 · 应用 -->
        <section class="sgroup">
          <div class="appcard">
            <div class="app-logo">P</div>
            <div>
              <div class="app-name">Phoenix</div>
              <div class="app-tag">{t('about.appTag')}</div>
            </div>
          </div>
          <p class="app-desc">{t('about.appTagline')}</p>
          <div class="srow">
            <div class="stext">
              <div class="slabel">{t('about.developer')}</div>
            </div>
            <PhxLink href={HOME_URL} external>{t('about.devLink')}</PhxLink>
          </div>
        </section>

        <!-- 卡 2 · 版本 -->
        <section class="sgroup">
          <div class="sgroup-h">{t('about.card.version')}</div>
          <div class="sgroup-sub">{t('about.version.sub')}</div>
          <div class="srow">
            <div class="stext">
              <div class="slabel">{t('about.currentVersion')}</div>
            </div>
            <span class="ver">{version || '—'}</span>
          </div>
          <div class="srow">
            <div class="stext">
              <div class="slabel">{t('about.checkLatest')}</div>
              <div class="sdesc">{t('about.checkLatest.desc')}</div>
            </div>
            <button
              class="mbtn"
              type="button"
              onclick={() => void openUrl(RELEASES_URL).catch(() => {})}
            >
              {t('about.btnRelease')}
            </button>
          </div>
        </section>
      </div>

      <div class="scol">
        <!-- 卡 3 · 开源许可 -->
        <section class="sgroup">
          <div class="sgroup-h">{t('about.card.license')}</div>
          <div class="sgroup-sub">{t('about.license.desc')}</div>
          <div class="srow">
            <div class="stext">
              <div class="slabel">{t('about.license.label')}</div>
            </div>
            <span class="lic-badge">MIT</span>
          </div>
          <div class="srow">
            <div class="stext">
              <div class="slabel">{t('about.license.source')}</div>
            </div>
            <PhxLink href={REPO_URL} external>{t('about.repo')}</PhxLink>
          </div>
        </section>

        <!-- 卡 4 · 问题反馈 -->
        <section class="sgroup">
          <div class="sgroup-h">{t('about.card.issues')}</div>
          <div class="sgroup-sub">{t('about.issues.sub')}</div>
          <div class="srow">
            <div class="stext">
              <div class="slabel">{t('about.card.issues')}</div>
              <div class="sdesc">{t('about.issues.desc')}</div>
            </div>
            <PhxLink href={`${REPO_URL}/issues`} external>GitHub Issues</PhxLink>
          </div>
        </section>

        <!-- 卡 5 · 致谢（两条真实外链，系统浏览器打开） -->
        <section class="sgroup">
          <div class="sgroup-h">{t('about.card.credits')}</div>
          <div class="sgroup-sub">{t('about.credits.sub')}</div>
          <div class="srow">
            <div class="stext">
              <div class="slabel">{t('about.credit.spt')}</div>
              <div class="sdesc">{t('about.credit.spt.desc')}</div>
            </div>
            <PhxLink href={SPT_URL} external>{t('about.credit.spt.link')}</PhxLink>
          </div>
          <div class="srow">
            <div class="stext">
              <div class="slabel">{t('about.credit.skiTles')}</div>
              <div class="sdesc">{t('about.credit.skiTles.desc')}</div>
            </div>
            <PhxLink href={SKITLES_URL} external>GitHub</PhxLink>
          </div>
        </section>
      </div>
    </div>

    <!-- 卡 6 · 免责声明（全宽警示卡置底） -->
    <section class="sgroup disc">
      <div class="sgroup-h">{t('about.card.disclaimer')}</div>
      <div class="disc-body">
        <p>{t('about.disc.1')}</p>
        <p>{t('about.disc.2')}</p>
        <p>{t('about.disc.3')}</p>
      </div>
    </section>
  </div>
</main>

<style>
  .page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .main-head {
    padding: 24px 24px 0;
  }

  .large-title {
    font-size: 34px;
    font-weight: 700;
    letter-spacing: -0.4px;
    line-height: 1.15;
    margin: 0;
  }

  .head-meta {
    margin: 6px 0 0;
    font-size: 12.5px;
    color: var(--phx-text-3);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 0 24px 104px;
  }

  .sgrid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0 16px;
    align-items: start;
  }

  .sgroup {
    background: var(--phx-surface);
    border-radius: var(--phx-r-card);
    overflow: hidden;
    margin-top: 16px;
  }

  .sgroup-h {
    padding: 14px 18px 2px;
    font-size: 14.5px;
    font-weight: 700;
  }

  .sgroup-sub {
    padding: 0 18px 12px;
    font-size: 11.5px;
    color: var(--phx-text-3);
  }

  .srow {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 13px 18px;
  }

  .srow + .srow,
  .sgroup-sub + .srow {
    border-top: 0.5px solid var(--phx-sep);
  }

  .stext {
    flex: 1;
    min-width: 0;
  }

  .slabel {
    font-size: 13px;
    font-weight: 600;
  }

  .sdesc {
    font-size: 12px;
    color: var(--phx-text-2);
    margin-top: 3px;
    line-height: 1.5;
  }

  .mbtn {
    height: 26px;
    padding: 0 12px;
    border-radius: var(--phx-r-btn);
    border: none;
    font-size: 12px;
    font-weight: 600;
    font-family: inherit;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    transition: filter 0.15s;
  }

  .mbtn:hover {
    filter: brightness(1.15);
  }

  .lic-badge {
    height: 22px;
    padding: 0 10px;
    border-radius: 6px;
    flex: none;
    display: inline-flex;
    align-items: center;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.02em;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }

  .ver {
    font-family: ui-monospace, Consolas, monospace;
    font-size: 12.5px;
    color: var(--phx-text);
    flex: none;
  }

  .appcard {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 18px 12px;
  }

  .app-logo {
    width: 52px;
    height: 52px;
    border-radius: 14px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: fantasy;
    font-style: italic;
    font-weight: 700;
    font-size: 28px;
    background: var(--phx-accent-dim);
    color: var(--phx-accent);
  }

  .app-name {
    font-size: 19px;
    font-weight: 700;
    letter-spacing: -0.2px;
  }

  .app-tag {
    font-size: 12px;
    color: var(--phx-text-2);
    margin-top: 3px;
  }

  .app-desc {
    padding: 0 18px 14px;
    font-size: 12.5px;
    color: var(--phx-text-2);
    line-height: 1.65;
  }

  .app-desc + .srow {
    border-top: 0.5px solid var(--phx-sep);
  }

  .disc {
    background: var(--phx-warn-bg);
    box-shadow: inset 0 0 0 0.5px var(--phx-warn-bd);
  }

  .disc .sgroup-h {
    color: var(--phx-warn);
  }

  .disc-body {
    padding: 10px 18px 16px;
    font-size: 12.5px;
    color: var(--phx-text-2);
    line-height: 1.8;
  }

  .disc-body p + p {
    margin-top: 6px;
  }
</style>
