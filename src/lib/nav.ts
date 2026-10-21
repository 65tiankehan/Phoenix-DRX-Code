/**
 * 导航单一来源：底部 dock 与顶栏「常去」tab 共用同一份路由 / 文案 / 图标定义。
 * 新增导航项只改这里，两处同步生效（图标统一为 path d 字符串数组，模板单一 {#each} 渲染）。
 */
import type { MessageKey } from '$lib/i18n/i18n.svelte'

export interface NavItem {
  path: NavPath
  key: MessageKey
  icon: string[]
}

/** 导航路由字面量联合：保证 resolve() 的强类型路由校验（NavItem 只允许这 10 条） */
export type NavPath =
  | '/home'
  | '/character'
  | '/quests'
  | '/traders'
  | '/hideout'
  | '/stash'
  | '/presets'
  | '/backup'
  | '/settings'
  | '/about'

/** 角色域子页：dock「角色」与顶栏 tab 按分区归并高亮 */
const CHARACTER_SECTION = ['/character', '/skills', '/health']

/** 仓库域子页：Scav 装备与检视屏同归仓库域 */
const STASH_SECTION = ['/stash', '/inspect', '/scav']

export const NAV: NavItem[] = [
  {
    path: '/home',
    key: 'nav.home',
    icon: ['M4 11.5 12 4l8 7.5', 'M6.5 10.5V20h11v-9.5']
  },
  {
    path: '/character',
    key: 'nav.character',
    icon: ['M12 4.4a3.6 3.6 0 1 0 0 7.2 3.6 3.6 0 1 0 0-7.2z', 'M5 20c1.4-3.6 4-5 7-5s5.6 1.4 7 5']
  },
  {
    path: '/quests',
    key: 'nav.quests',
    icon: [
      'M4 6l1.4 1.4L8 4.8',
      'M4 12l1.4 1.4L8 10.8',
      'M4 18l1.4 1.4L8 16.8',
      'M11.5 6H20',
      'M11.5 12H20',
      'M11.5 18H20'
    ]
  },
  {
    path: '/traders',
    key: 'nav.traders',
    icon: ['M6 8h12l-1 12H7L6 8Z', 'M9 8V6.5a3 3 0 0 1 6 0V8']
  },
  {
    path: '/hideout',
    key: 'nav.hideout',
    icon: ['M3.5 11 12 3.5 20.5 11', 'M5.5 9.5V20h13V9.5', 'M9.5 20v-5.5h5V20']
  },
  {
    path: '/stash',
    key: 'nav.stash',
    icon: ['M3.5 7.5h17v12.5h-17z', 'M3.5 11.5h17', 'M9 7.5V4h6v3.5']
  },
  {
    path: '/presets',
    key: 'nav.presets',
    icon: ['M12 3.6l2.5 5.2 5.7.8-4.1 4 1 5.7-5.1-2.7-5.1 2.7 1-5.7-4.1-4 5.7-.8z']
  },
  {
    path: '/backup',
    key: 'nav.backup',
    icon: ['M3 3v5h5', 'M3.05 13A9 9 0 1 0 6 5.3L3 8', 'M12 7v5l4 2']
  },
  {
    path: '/settings',
    key: 'nav.settings',
    icon: [
      'M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z',
      'M12 9a3 3 0 1 0 0 6 3 3 0 1 0 0-6z'
    ]
  },
  {
    path: '/about',
    key: 'nav.about',
    icon: ['M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20z', 'M12 16v-4', 'M12 8h.01']
  }
]

/**
 * 当前路径归并到导航项序号（角色域 / 仓库域子页归并到父项）；-1 = 未命中任何导航项。
 * dock 滑块定位与顶栏 tab 高亮都走这一个口径，避免两处亮法打架。
 */
export function navIndex(pathname: string): number {
  if (CHARACTER_SECTION.includes(pathname)) return NAV.findIndex((n) => n.path === '/character')
  if (STASH_SECTION.includes(pathname)) return NAV.findIndex((n) => n.path === '/stash')
  return NAV.findIndex((n) => n.path === pathname)
}
