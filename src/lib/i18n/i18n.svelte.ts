import zhCN from './zh-CN.json'
import enUS from './en-US.json'

export type Locale = 'zh-CN' | 'en-US'
export type MessageKey = keyof typeof zhCN

const dicts: Record<Locale, Record<string, string>> = {
  'zh-CN': zhCN as Record<string, string>,
  'en-US': enUS as Record<string, string>
}

const LOCALE_KEY = 'phx.locale'

/** 启动即读持久化语言（localStorage 同步读，避免首帧语言闪烁） */
function initialLocale(): Locale {
  try {
    const saved = localStorage.getItem(LOCALE_KEY)
    if (saved === 'zh-CN' || saved === 'en-US') return saved
  } catch {
    /* 存储不可用时回落默认 */
  }
  return 'zh-CN'
}

/** 当前语言（runes 全局状态；切换入口在设置页，偏好持久化在 localStorage） */
export const i18n = $state({ locale: initialLocale() })

/** 云同步监听（sync store 注册）：语言每次切换落盘都会通知，回环由 sync store 的闸门拦截 */
type LocaleChangeFn = (l: Locale) => void
let localeChangeListener: LocaleChangeFn | null = null

export function onLocaleChange(fn: LocaleChangeFn | null): void {
  localeChangeListener = fn
}

/** 切换语言并持久化 */
export function setLocale(locale: Locale): void {
  i18n.locale = locale
  try {
    localStorage.setItem(LOCALE_KEY, locale)
  } catch {
    /* 存储不可用时仅内存生效 */
  }
  localeChangeListener?.(locale)
}

/**
 * 取文案。双语 key 强一致由 tests/i18n.test.ts 卡死；
 * 缺 key 时回退中文再回退 key 本身，开发期即暴露。
 */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  const raw = dicts[i18n.locale][key] ?? dicts['zh-CN'][key] ?? key
  if (!params) return raw
  return raw.replace(/\{(\w+)\}/g, (_, p: string) => (p in params ? String(params[p]) : `{${p}}`))
}
