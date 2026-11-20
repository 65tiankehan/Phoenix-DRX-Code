/** 界面语言（与 src/lib/i18n/*.json 一一对应） */
export type Locale = 'zh-CN' | 'en-US'

/**
 * 首次启动跟随系统语言：中文家族（zh / zh-TW / zh-HK …）给简中，其余一律英文。
 * 只看主语言子标签，避免 `zh-Hant` / `en-GB` 这类分支落到默认值。
 * 放在独立文件里是因为 `i18n.svelte.ts` 带 runes，单测不该为它拉 svelte 运行时。
 */
export function localeFromSystem(languages: readonly string[] | undefined): Locale {
  for (const tag of languages ?? []) {
    const primary = tag.trim().toLowerCase().split('-')[0]
    if (primary === 'zh') return 'zh-CN'
    if (primary) return 'en-US'
  }
  return 'zh-CN'
}
