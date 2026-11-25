import zhCN from './zh-CN.json'
import enUS from './en-US.json'
import ru from './ru.json'
import de from './de.json'
import fr from './fr.json'
import es from './es.json'
import pl from './pl.json'
import ptBR from './pt-BR.json'
import tr from './tr.json'
import { FALLBACK_LOCALE, LOCALES, localeFromSystem, type Locale } from './locale'

export type { Locale }
export type MessageKey = keyof typeof zhCN

/**
 * 各语种词条。静态导入：t() 必须保持同步（首帧就要渲染导航与标题），
 * Tauri 本地包不存在下载成本；语种数涨到 T2/T3（9～20 语）再改惰性装载 + 首帧等待。
 * 新增语种 = locale.ts 的 LOCALES/LOCALE_LABELS 各加一项 + 这里加一行 + 补 JSON 全集。
 */
const dicts: Record<Locale, Record<string, string>> = {
  'zh-CN': zhCN as Record<string, string>,
  'en-US': enUS as Record<string, string>,
  ru: ru as Record<string, string>,
  de: de as Record<string, string>,
  fr: fr as Record<string, string>,
  es: es as Record<string, string>,
  pl: pl as Record<string, string>,
  'pt-BR': ptBR as Record<string, string>,
  tr: tr as Record<string, string>
}

const LOCALE_KEY = 'phx.locale'

/** 启动即读持久化语言（localStorage 同步读，避免首帧语言闪烁）；没有记录时跟随系统 */
function initialLocale(): Locale {
  try {
    const saved = localStorage.getItem(LOCALE_KEY)
    if (saved && (LOCALES as readonly string[]).includes(saved)) return saved as Locale
  } catch {
    /* 存储不可用时走系统判定 */
  }
  try {
    return localeFromSystem(navigator.languages)
  } catch {
    return 'zh-CN'
  }
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
 * 取文案。全语种 key 集合相等由 tests/i18n.test.ts 卡死；
 * 缺 key 时按「当前语种 → 兜底英文 → 中文 → key 本身」逐级回退，开发期即暴露。
 *
 * 数量文案的复数处理：俄语不接 Intl.PluralRules，改用不可变缩写（`шт.` / `уров.` 等），
 * 这样「5 шт.」「1 шт.」都成立；到 T2 语种变多再统一引入 tn() + 复数后缀键。
 */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  const raw = dicts[i18n.locale][key] ?? dicts[FALLBACK_LOCALE][key] ?? dicts['zh-CN'][key] ?? key
  if (!params) return raw
  return raw.replace(/\{(\w+)\}/g, (_, p: string) => (p in params ? String(params[p]) : `{${p}}`))
}
