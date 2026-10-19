/**
 * 公告面板全局状态（18 稿图生产实现；主窗消费，薄窗不渲染不拉取）。
 * 拉取在 +layout onMount（薄窗直接 return），结果经 seedAnnouncements 写入。
 * 未读判定按公告 id，持久化 localStorage 'phx.ann.read'（照 phx.sound / phx.motion 模式）。
 */
import type { PhxAnnouncement } from './ipc'

const READ_KEY = 'phx.ann.read'

function loadReadIds(): Set<string> {
  if (typeof localStorage === 'undefined') return new Set()
  try {
    const raw = localStorage.getItem(READ_KEY)
    const ids: unknown = raw ? JSON.parse(raw) : []
    return Array.isArray(ids)
      ? new Set(ids.filter((x): x is string => typeof x === 'string'))
      : new Set()
  } catch {
    return new Set()
  }
}

export const announcements = $state({
  /** 公告列表（空数组 = 无公告，面板展示空态） */
  list: [] as PhxAnnouncement[],
  /** 数据来源（remote / cache / none），仅驱动来源徽章，不做业务分支 */
  source: '' as string,
  /** 面板开关 */
  open: false,
  /** 已读 id 集合（$state 集合代理：add / delete / has 均响应式） */
  readIds: loadReadIds()
})

/** 启动拉取结果写入；含未读公告时自动弹出面板一次（已读则静默，不扰人） */
export function seedAnnouncements(list: PhxAnnouncement[], source: string): void {
  announcements.list = list
  announcements.source = source
  if (list.some((a) => !announcements.readIds.has(a.id))) announcements.open = true
}

export function toggleAnnouncements(): void {
  announcements.open = !announcements.open
}

/** 关闭面板并全量标记已读（关闭钮 / 「知道了」/ Esc / 遮罩点击共用出口） */
export function closeAnnouncements(): void {
  announcements.open = false
  for (const a of announcements.list) announcements.readIds.add(a.id)
  persistReadIds()
}

function persistReadIds(): void {
  if (typeof localStorage === 'undefined') return
  try {
    localStorage.setItem(READ_KEY, JSON.stringify([...announcements.readIds]))
  } catch {
    /* 存储不可用：未读点退化为会话内记忆 */
  }
}
