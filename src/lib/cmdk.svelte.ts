/**
 * 全局命令面板（Ctrl+K）状态与目的地索引。
 *
 * 索引一期只收主干：dock 10 页 + 设置页 5 分组 + 4 项操作
 * （稿图 docs/prototypes/20-cmdk.html 场景 1/2；二期完整搜索页仅存稿图）。
 * 名称与描述全部存 i18n key，渲染时 t() 求值，语言切换即时生效。
 */
import { playSound } from '$lib/sound'
import type { RouteId } from '$app/types'
import type { MessageKey } from '$lib/i18n/i18n.svelte'

/** 条目分组（渲染顺序固定：pages → settings → actions） */
export type CmdkGroup = 'pages' | 'settings' | 'actions'

/** route 之外的可执行动作 */
export type CmdkAction = 'check-update' | 'open-github' | 'open-issues' | 'open-profiles'

export interface CmdkEntry {
  id: string
  group: CmdkGroup
  /** 名称 i18n key */
  nm: MessageKey
  /** 描述 i18n key */
  ds: MessageKey
  /** 17box 图标 svg 内部标记（stroke 风格；fill 图标置 fill=true），面板 {@html} 渲染 */
  icon: string
  fill?: boolean
  /** 应用内路由（不含 base） */
  route?: RouteId
  /** 设置页分组锚点（route 为 /settings 时生效，落 ?group=<anchor>） */
  anchor?: string
  action?: CmdkAction
}

/** 组渲染顺序与组头文案 key */
export const CMDK_GROUPS: ReadonlyArray<{ id: CmdkGroup; label: MessageKey }> = [
  { id: 'pages', label: 'cmdk.group.pages' },
  { id: 'settings', label: 'cmdk.group.settings' },
  { id: 'actions', label: 'cmdk.group.actions' },
]

const IC = {
  home: '<path d="M2.5 8.5 8.5 2.8l6 5.7"/><path d="M4 7.6V14h9V7.6"/>',
  character:
    '<circle cx="8.5" cy="5.6" r="2.9"/><path d="M2.9 14.4c.8-2.7 3-4.1 5.6-4.1s4.8 1.4 5.6 4.1"/>',
  quests: '<circle cx="8.5" cy="8.5" r="6"/><path d="M8.5 5.2v3.3l2.3 1.5"/>',
  traders:
    '<circle cx="6.2" cy="6.2" r="2.6"/><path d="M1.8 14c.6-2.2 2.3-3.4 4.4-3.4s3.8 1.2 4.4 3.4"/><path d="M11.5 4.2h4M11.5 7.4h4M11.5 10.6h4"/>',
  hideout:
    '<path d="M2.2 14.5V6.2L8.5 2.5l6.3 3.7v8.3z"/><path d="M6.6 14.5v-4.4h3.8v4.4"/>',
  stash:
    '<path d="M2.5 4.5h12v9.2h-12z"/><path d="M2.5 4.5l1.8-2.3h8.4l1.8 2.3M8.5 6.8v4.6M6.2 9.1h4.6"/>',
  presets:
    '<path d="M4.2 3.2h2.6v2.6H4.2zM10.2 3.2h2.6v2.6h-2.6zM4.2 11.2h2.6v2.6H4.2zM10.2 11.2h2.6v2.6h-2.6z"/><path d="M5.5 5.8v5.4M11.5 5.8v5.4"/>',
  backup:
    '<path d="M8.5 2.5a6 6 0 1 1-6 6"/><path d="M8.5 5.4v3.1l2.2 1.4"/><path d="M2.5 3.2v3h3"/>',
  gear: '<circle cx="8.5" cy="8.5" r="2.2"/><path d="M13.9 10.6a2 2 0 0 0 .4-2.1l-.1-.2.1-.2a2 2 0 0 0-.4-2.1l-.6-.2-.2-.6a2 2 0 0 0-2.1-.4l-.2.1-.2-.1a2 2 0 0 0-2.1.4l-.2.6-.6.2a2 2 0 0 0-.4 2.1l.1.2-.1.2a2 2 0 0 0 .4 2.1l.6.2.2.6a2 2 0 0 0 2.1.4l.2-.1.2.1a2 2 0 0 0 2.1-.4z" transform="translate(-2.5 -2.5) scale(1.18)"/>',
  about:
    '<circle cx="8.5" cy="8.5" r="6.2"/><path d="M8.5 11.9h.01M6.7 6.5c.2-1 1-1.6 1.8-1.6s1.6.7 1.6 1.5c0 1.1-1.6 1.3-1.6 2.4"/>',
  sync: '<path d="M3 6.2h7.2M3 10.8h5.4"/><path d="M12.4 4.4v7.6M12.4 12l1.6-1.8M12.4 12l-1.6-1.8"/>',
  github:
    '<path d="M8.5 1.6a6.9 6.9 0 0 0-2.2 13.4c.3.1.5-.1.5-.3v-1.2c-1.9.4-2.3-.8-2.3-.8-.3-.8-.8-1-.8-1-.6-.4.1-.4.1-.4.7.1 1.1.7 1.1.7.6 1.1 1.6.8 2 .6.1-.5.3-.8.5-1-1.5-.2-3.2-.8-3.2-3.5 0-.8.3-1.4.7-1.9-.1-.2-.3-.9.1-1.9 0 0 .6-.2 2 .7a6.8 6.8 0 0 1 3.6 0c1.4-.9 2-.7 2-.7.4 1 .2 1.7.1 1.9.5.5.7 1.1.7 1.9 0 2.7-1.7 3.3-3.2 3.5.3.2.5.7.5 1.3v2c0 .2.2.4.5.3A6.9 6.9 0 0 0 8.5 1.6z"/>',
  server:
    '<rect x="2.5" y="3.2" width="12" height="4.4" rx="1.2"/><rect x="2.5" y="9.4" width="12" height="4.4" rx="1.2"/><path d="M5.2 5.4h.01M5.2 11.6h.01"/>',
  sliders:
    '<circle cx="10.4" cy="5.6" r="1.9"/><circle cx="6.2" cy="11.4" r="1.9"/><path d="M2.5 5.6h6M12.3 5.6h2.2M2.5 11.4h1.8M8.1 11.4h6.4"/>',
  wrench:
    '<path d="M10.4 4.5a.7.7 0 0 0 0 1l1.1 1.1a.7.7 0 0 0 1 0l2.7-2.7a4.25 4.25 0 0 1-5.6 5.6l-4.9 4.9a1.5 1.5 0 0 1-2.1-2.1l4.9-4.9a4.25 4.25 0 0 1 5.6-5.6l-2.7 2.7z"/>',
  refresh:
    '<path d="M14.9 8.5a6.4 6.4 0 1 1-6.4-6.4c1.8 0 3.5.7 4.8 1.9L14.9 5.7"/><path d="M14.9 2.1v3.5h-3.5"/>',
  issue:
    '<circle cx="8.5" cy="8.5" r="6.2"/><path d="M8.5 5v4.2M8.5 12h.01"/>',
  folder: '<path d="M2.5 13.5V3.5h4.3l1.7 1.9h6v8.1z"/>',
} as const

/** 面板目的地索引（19 项，显示顺序即数组顺序） */
export const CMDK_ENTRIES: readonly CmdkEntry[] = [
  // ── 页面（dock 10 页）──
  { id: 'page-home', group: 'pages', nm: 'nav.home', ds: 'cmdk.desc.home', icon: IC.home, route: '/home' },
  { id: 'page-character', group: 'pages', nm: 'nav.character', ds: 'cmdk.desc.character', icon: IC.character, route: '/character' },
  { id: 'page-quests', group: 'pages', nm: 'nav.quests', ds: 'cmdk.desc.quests', icon: IC.quests, route: '/quests' },
  { id: 'page-traders', group: 'pages', nm: 'nav.traders', ds: 'cmdk.desc.traders', icon: IC.traders, route: '/traders' },
  { id: 'page-hideout', group: 'pages', nm: 'nav.hideout', ds: 'cmdk.desc.hideout', icon: IC.hideout, route: '/hideout' },
  { id: 'page-stash', group: 'pages', nm: 'nav.stash', ds: 'cmdk.desc.stash', icon: IC.stash, route: '/stash' },
  { id: 'page-presets', group: 'pages', nm: 'nav.presets', ds: 'cmdk.desc.presets', icon: IC.presets, route: '/presets' },
  { id: 'page-backup', group: 'pages', nm: 'nav.backup', ds: 'cmdk.desc.backup', icon: IC.backup, route: '/backup' },
  { id: 'page-settings', group: 'pages', nm: 'nav.settings', ds: 'cmdk.desc.settings', icon: IC.gear, route: '/settings' },
  { id: 'page-about', group: 'pages', nm: 'nav.about', ds: 'cmdk.desc.about', icon: IC.about, route: '/about' },
  // ── 设置（分组级定位）──
  { id: 'set-general', group: 'settings', nm: 'settings.group.general', ds: 'settings.group.general.sub', icon: IC.gear, route: '/settings', anchor: 'general' },
  { id: 'set-server', group: 'settings', nm: 'settings.group.server', ds: 'settings.group.server.sub', icon: IC.server, route: '/settings', anchor: 'server' },
  { id: 'set-behavior', group: 'settings', nm: 'settings.group.behavior', ds: 'settings.group.behavior.sub', icon: IC.sliders, route: '/settings', anchor: 'behavior' },
  { id: 'set-maintenance', group: 'settings', nm: 'settings.group.maintenance', ds: 'settings.group.maintenance.sub', icon: IC.wrench, route: '/settings', anchor: 'maintenance' },
  { id: 'set-sync', group: 'settings', nm: 'sync.title', ds: 'sync.subA', icon: IC.sync, route: '/settings', anchor: 'sync' },
  // ── 操作 ──
  { id: 'act-check-update', group: 'actions', nm: 'cmdk.act.checkUpdate', ds: 'settings.checkNow.desc', icon: IC.refresh, action: 'check-update' },
  { id: 'act-open-github', group: 'actions', nm: 'cmdk.act.github', ds: 'cmdk.act.browser', icon: IC.github, fill: true, action: 'open-github' },
  { id: 'act-open-issues', group: 'actions', nm: 'about.card.issues', ds: 'about.issues.desc', icon: IC.issue, action: 'open-issues' },
  { id: 'act-open-profiles', group: 'actions', nm: 'tray.openDir', ds: 'cmdk.act.files', icon: IC.folder, action: 'open-profiles' },
]

/** 面板开关状态（PhxTitlebar 按钮 / Ctrl+K 快捷键 / PhxCommandPalette 共享） */
export const cmdk = $state({ open: false })

/** 开：响 pop；关：不响（感官层基线：开启类动效映射） */
export function toggleCmdk(): void {
  if (cmdk.open) {
    cmdk.open = false
  } else {
    cmdk.open = true
    playSound('pop')
  }
}

export function closeCmdk(): void {
  cmdk.open = false
}
