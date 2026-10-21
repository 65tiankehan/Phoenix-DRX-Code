/**
 * 顶栏「常去」记录：按打分排序的导航页。
 *
 * 打分算法（页面价值 = 意愿强度 × 新鲜度）：
 *   score = (访问次数 n + 2 × 顶栏点入次数 c) × 0.5 ^ (距今天数 / 14)
 *   - 次数：从任何入口进出该页的次数，代表使用量
 *   - 点击：从顶栏「常去」组点进来的次数，意图更明确，权重 ×2
 *   - 时间：半衰减期 14 天，久不去的页面自然沉底，常去的浮上来
 * 同位次按最后访问时间降序；记录上限 20 条，超出按分数截断。
 *
 * 性能纪律：切页只更新内存状态；落盘走 400ms 防抖，不在路由切换的同一帧做磁盘写入。
 * 只记导航屏（服务器选择 / 托盘 / 更新薄窗等非导航屏不参与）。
 */
import { navIndex } from '$lib/nav'

const KEY = 'phx.recent'
const MAX = 20
const TOP = 3
const HALF_LIFE_DAYS = 14
const PERSIST_DELAY = 400

export interface RecentVisit {
  /** 路由 */
  path: string
  /** 访问次数（任意入口） */
  n: number
  /** 顶栏「常去」组点入次数（权重更高） */
  c: number
  /** 最后访问时间戳 */
  t: number
}

function load(): RecentVisit[] {
  try {
    const raw = localStorage.getItem(KEY)
    if (!raw) return []
    const arr: unknown = JSON.parse(raw)
    if (!Array.isArray(arr)) return []
    return arr
      .filter(
        (r): r is RecentVisit =>
          typeof r === 'object' &&
          r !== null &&
          typeof (r as RecentVisit).path === 'string' &&
          navIndex((r as RecentVisit).path) >= 0
      )
      .map((r) => ({
        path: r.path,
        n: Number(r.n) || 0,
        c: Number(r.c) || 0,
        t: Number(r.t) || 0
      }))
  } catch {
    /* 存储不可用或数据损坏：从空记录起步 */
    return []
  }
}

const state = $state<{ list: RecentVisit[] }>({ list: load() })

let flushTimer: ReturnType<typeof setTimeout> | null = null

/** 防抖落盘：连续切页只写一次，且不占用路由切换当帧 */
function schedulePersist(): void {
  if (flushTimer) return
  flushTimer = setTimeout(() => {
    flushTimer = null
    try {
      localStorage.setItem(KEY, JSON.stringify(state.list))
    } catch {
      /* 存储不可用仅内存生效 */
    }
  }, PERSIST_DELAY)
}

/** 顶栏 tab 点入标记：由 PhxTitlebar 在点击跳转前调用，供下一次 markVisit 识别来源 */
let tabIntent: string | null = null

export function markFromTab(path: string): void {
  tabIntent = path
}

/** 打分：意愿强度 × 时间衰减（半衰期 14 天） */
function score(r: RecentVisit, now: number): number {
  const ageDays = Math.max(0, (now - r.t) / 86_400_000)
  const freshness = Math.pow(0.5, ageDays / HALF_LIFE_DAYS)
  return (r.n + 2 * r.c) * freshness
}

/** 记录一次导航访问（非导航屏静默忽略） */
export function markVisit(pathname: string): void {
  if (navIndex(pathname) < 0) return
  const fromTab = tabIntent === pathname
  tabIntent = null

  const hit = state.list.find((r) => r.path === pathname)
  if (hit) {
    hit.n += 1
    if (fromTab) hit.c += 1
    hit.t = Date.now()
  } else {
    state.list.push({ path: pathname, n: 1, c: fromTab ? 1 : 0, t: Date.now() })
  }
  if (state.list.length > MAX) {
    const now = Date.now()
    state.list.sort((a, b) => score(b, now) - score(a, now) || b.t - a.t)
    state.list.splice(MAX)
  }
  schedulePersist()
}

/** 常去前 TOP 项（打分降序，同位次按最后访问降序）；不足 TOP 时按实际数量返回 */
export function oftenVisited(): RecentVisit[] {
  const now = Date.now()
  return [...state.list]
    .sort((a, b) => score(b, now) - score(a, now) || b.t - a.t)
    .slice(0, TOP)
}
