export type ToastKind = 'success' | 'warning' | 'error' | 'info'

export interface ToastAction {
  label: string
  /** 外链（与 onClick 二选一；onClick 优先） */
  url: string
  /** 命令回调（如「立即重启」调 update_install），设置后忽略 url */
  onClick?: () => void
}

export interface ToastItem {
  id: number
  kind: ToastKind
  text: string
  action?: ToastAction
}

/** 模块级共享队列：全应用一个 toast 域，右上角可堆叠（视觉基线 §一 轻提示行） */
export const toastItems = $state<ToastItem[]>([])

let seq = 0

function push(kind: ToastKind, text: string, duration: number, action?: ToastAction): void {
  const id = ++seq
  toastItems.push(action ? { id, kind, text, action } : { id, kind, text })
  window.setTimeout(() => {
    const i = toastItems.findIndex((t) => t.id === id)
    if (i !== -1) toastItems.splice(i, 1)
  }, duration)
}

/** 轻提示：success/warning 默认 3.2s，error 默认 4s（对齐旧 useToast 语义） */
export const toast = {
  success: (text: string, duration = 3200): void => push('success', text, duration),
  warning: (text: string, duration = 3200): void => push('warning', text, duration),
  error: (text: string, duration = 4000): void => push('error', text, duration),
  /** info：中性信息（公告等），accent 点 */
  info: (text: string, duration = 3200): void => push('info', text, duration),
  /** 公告：长驻 12s + 可点击动作；点击动作后由 ToastHost 调 dismissToast 移除 */
  announce: (text: string, action?: ToastAction, duration = 12000): void =>
    push('info', text, duration, action)
}

/** 主动移除指定 toast（动作点击后即清） */
export function dismissToast(id: number): void {
  const i = toastItems.findIndex((t) => t.id === id)
  if (i !== -1) toastItems.splice(i, 1)
}
