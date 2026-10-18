export type ToastKind = 'success' | 'warning' | 'error'

export interface ToastItem {
  id: number
  kind: ToastKind
  text: string
}

/** 模块级共享队列：全应用一个 toast 域，右上角可堆叠（视觉基线 §一 轻提示行） */
export const toastItems = $state<ToastItem[]>([])

let seq = 0

function push(kind: ToastKind, text: string, duration: number): void {
  const id = ++seq
  toastItems.push({ id, kind, text })
  window.setTimeout(() => {
    const i = toastItems.findIndex((t) => t.id === id)
    if (i !== -1) toastItems.splice(i, 1)
  }, duration)
}

/** 轻提示：success/warning 默认 3.2s，error 默认 4s（对齐旧 useToast 语义） */
export const toast = {
  success: (text: string, duration = 3200): void => push('success', text, duration),
  warning: (text: string, duration = 3200): void => push('warning', text, duration),
  error: (text: string, duration = 4000): void => push('error', text, duration)
}
