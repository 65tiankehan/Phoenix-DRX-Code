/**
 * 物品图标：内存缓存 + 批量补图（Rust 侧落盘在 `%APPDATA%\com.phoenix.drx\icons`）。
 *
 * 调用约定：
 *   - 组件只读 `iconSrc(tpl)`，命中就显示图、没命中就显示首字色块（永不阻塞）；
 *   - 组件挂载时 `wantIcon(tpl)` 登记，模块攒 120ms 再批量请求，避免一屏几十次 IPC；
 *   - 离线 / CDN 抽风时 `icons_ensure` 返回空，界面保持色块，不报错、不转圈。
 */
import { SvelteSet } from 'svelte/reactivity'

import { phxIcons } from './ipc'

const cache = $state<Record<string, string>>({})
const requested = new SvelteSet<string>()
const queue: string[] = []
let timer: ReturnType<typeof setTimeout> | undefined
let enabled = $state(true)

/** 是否启用物品图标（设置页开关；关闭后一律用首字色块） */
export function setIconsEnabled(on: boolean): void {
  enabled = on
}

export function iconsEnabled(): boolean {
  return enabled
}

export function iconSrc(tpl: string | undefined | null): string | undefined {
  if (!enabled || !tpl) return undefined
  return cache[tpl]
}

/** 登记一个想看的图标（幂等；同一 tpl 只请求一次） */
export function wantIcon(tpl: string | undefined | null): void {
  if (!enabled || !tpl || requested.has(tpl) || cache[tpl]) return
  requested.add(tpl)
  queue.push(tpl)
  if (queue.length >= 48) {
    void flush()
    return
  }
  if (timer) clearTimeout(timer)
  timer = setTimeout(() => void flush(), 120)
}

async function flush(): Promise<void> {
  if (timer) {
    clearTimeout(timer)
    timer = undefined
  }
  const batch = queue.splice(0, 48)
  if (batch.length === 0) return
  try {
    // 先问缓存（不联网），缺的再补图；两步都返回 data URL 映射
    const hit = await phxIcons.cached(batch)
    Object.assign(cache, hit)
    const miss = batch.filter((t) => !hit[t])
    if (miss.length > 0) {
      const got = await phxIcons.ensure(miss)
      Object.assign(cache, got)
      // 拿不到的（容器类 / 新物品 / 离线）记下来，不再重复请求
      for (const t of miss) if (!got[t]) requested.add(t)
    }
  } catch {
    // 失败就保持色块兜底；清掉标记，下次进页面还能再试
    for (const t of batch) requested.delete(t)
  }
  if (queue.length > 0) void flush()
}
