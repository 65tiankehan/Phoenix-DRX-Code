/**
 * 减少动态效果偏好（感官层基线：设置页开关 → html 根类 phx-reduced-motion）。
 * 系统级 prefers-reduced-motion 由 app.css 媒体查询独立生效，双通道互不替代。
 * 存储约定照音效偏好（sound.ts）：'phx.motion'，'1' = 减少，缺省 = 正常动效。
 */
const STORAGE_KEY = 'phx.motion'

export function isReducedMotion(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === '1'
  } catch {
    return false
  }
}

export function setReducedMotion(v: boolean): void {
  try {
    if (v) localStorage.setItem(STORAGE_KEY, '1')
    else localStorage.removeItem(STORAGE_KEY)
  } catch {
    /* 隐私模式等存储不可用场景静默失败 */
  }
  applyMotionClass()
}

/** 应用启动时调用：把持久化偏好挂到 html 根类 */
export function applyMotionClass(): void {
  document.documentElement.classList.toggle('phx-reduced-motion', isReducedMotion())
}
