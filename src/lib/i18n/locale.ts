/** 界面语言清单（与 src/lib/i18n/<locale>.json 一一对应；新增语种只改这里 + 补 JSON） */
export const LOCALES = ['zh-CN', 'en-US', 'ru', 'de', 'fr', 'es', 'pl', 'pt-BR', 'tr'] as const

export type Locale = (typeof LOCALES)[number]

/** 兜底语言：任一语种缺键都回落到它（英文是全集的基准，见 tests/i18n.test.ts） */
export const FALLBACK_LOCALE: Locale = 'en-US'

/** 设置页下拉用本族语名，不随界面语言翻译 */
export const LOCALE_LABELS: Record<Locale, string> = {
  'zh-CN': '简体中文',
  'en-US': 'English',
  ru: 'Русский',
  de: 'Deutsch',
  fr: 'Français',
  es: 'Español',
  pl: 'Polski',
  'pt-BR': 'Português (BR)',
  tr: 'Türkçe'
}

/**
 * 主语言子标签 → 界面语言。
 * 只登记「有对应翻译」的语种；未登记的语种一律回落兜底语言，不做模糊猜测
 * （乌克兰语不并到俄语：SPT 自己把 uk 单列一种，玩家该有独立选项）。
 */
const PRIMARY_MAP: Record<string, Locale> = {
  zh: 'zh-CN',
  en: 'en-US',
  ru: 'ru',
  de: 'de',
  fr: 'fr',
  es: 'es',
  pl: 'pl',
  pt: 'pt-BR',
  tr: 'tr'
}

/**
 * 首次启动跟随系统语言：按 BCP 47 主语言子标签匹配，中文家族（zh / zh-TW / zh-HK …）给简中；
 * 尚未翻译的语种（uk / tr …）给兜底英文，不猜相似语种。
 * 放在独立文件里是因为 `i18n.svelte.ts` 带 runes，单测不该为它拉 svelte 运行时。
 */
export function localeFromSystem(languages: readonly string[] | undefined): Locale {
  for (const tag of languages ?? []) {
    const primary = tag.trim().toLowerCase().split('-')[0]
    if (!primary) continue
    return PRIMARY_MAP[primary] ?? FALLBACK_LOCALE
  }
  // 拿不到系统语言：沿用 0.2.4 行为给简中
  return 'zh-CN'
}


