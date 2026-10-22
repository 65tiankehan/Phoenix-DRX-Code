/**
 * 顶栏「常去」记录：按打分排序的导航页。
 *
 * 打分算法（指数遗忘的「近期访问强度」，2026-10-22 改版）：
 *   score = n + 2 × c，其中 n / c 是按半衰期 14 天指数折旧后的次数
 *   - 每次导航（markVisit）先把全表次数按各自距今时长折旧，再给当前页 +1
 *   - 效果：排序反映「最近常去」而非「历史累计」——旧口径的 lifetime 累计量
 *     会长期压制新常去的页面（实测：常点仓库/藏身处，顶栏数周不换血），
 *     改后常点的新页面数天内即浮顶，久不去的自然沉底
 *   - 点击：从顶栏「常去」组点进来的次数，意图更明确，权重 ×2
 * 同位次按最后访问时间降序；记录上限 20 条，超出按分数截断。
 *
 * 性能纪律：切页只更新内存状态；落盘走 400ms 防抖，不在路由切换的同一帧做磁盘写入。
 * 只记导航屏（服务器选择 / 托盘 / 更新薄窗等非导航屏不参与）。
 */
import { navIndex } from '$lib/nav'

const KEY = 'phx.recent2'
const MAX = 20
const TOP = 3
const HALF_LIFE_DAYS = 14
const PERSIST_DELAY = 400

export interface RecentVisit {
  /** 路由 */
  path: string
  /** 访问强度（任意入口；已按半衰期折旧，非整数累计） */
  n: number
  /** 顶栏「常去」组点入强度（权重更高，同样折旧） */
  c: number
  /** 最后访问时间戳（兼作折旧锚点与同位次排序） */
  t: number
}

function load(): RecentVisit[] {
  try {
    /* v1（phx.recent）为历史累计量口径，存量会长期压制新排序：作废换 v2 全新起步 */
    localStorage.removeItem('phx.recent')
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

/** 打分：意愿强度（n / c 已在 markVisit 里按半衰期折旧为近期强度） */
function score(r: RecentVisit): number {
  return r.n + 2 * r.c
}

/** 记录一次导航访问（非导航屏静默忽略） */
export function markVisit(pathname: string): void {
  if (navIndex(pathname) < 0) return
  const fromTab = tabIntent === pathname
  tabIntent = null
  const now = Date.now()

  /* 全表折旧（指数遗忘）：每条按自身距今时长衰减后再记账，让排序跟随最近的
     访问强度而不是历史累计——老页面的存量不再压制新常去的页面 */
  for (const r of state.list) {
    const f = Math.pow(0.5, Math.max(0, now - r.t) / 86_400_000 / HALF_LIFE_DAYS)
    if (f < 1) {
      r.n *= f
      r.c *= f
    }
  }

  const hit = state.list.find((r) => r.path === pathname)
  if (hit) {
    hit.n += 1
    if (fromTab) hit.c += 1
    hit.t = now
  } else {
    state.list.push({ path: pathname, n: 1, c: fromTab ? 1 : 0, t: now })
  }
  if (state.list.length > MAX) {
    state.list.sort((a, b) => score(b) - score(a) || b.t - a.t)
    state.list.splice(MAX)
  }
  schedulePersist()
}

/** 常去前 TOP 项（强度降序，同位次按最后访问降序）；不足 TOP 时按实际数量返回 */
export function oftenVisited(): RecentVisit[] {
  return [...state.list]
    .sort((a, b) => score(b) - score(a) || b.t - a.t)
    .slice(0, TOP)
}
