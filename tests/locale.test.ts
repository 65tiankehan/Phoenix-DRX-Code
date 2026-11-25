import { describe, expect, it } from 'vitest'
import { FALLBACK_LOCALE, LOCALE_LABELS, LOCALES, localeFromSystem } from '../src/lib/i18n/locale'

describe('首启语言跟随系统', () => {
  it('中文各分支都给简中', () => {
    expect(localeFromSystem(['zh-CN'])).toBe('zh-CN')
    expect(localeFromSystem(['zh-TW'])).toBe('zh-CN')
    expect(localeFromSystem(['zh-HK', 'zh-Hant'])).toBe('zh-CN')
    expect(localeFromSystem(['ZH-cn'])).toBe('zh-CN')
  })

  it('俄语各分支都给俄语', () => {
    expect(localeFromSystem(['ru'])).toBe('ru')
    expect(localeFromSystem(['ru-RU'])).toBe('ru')
    expect(localeFromSystem(['RU-ru'])).toBe('ru')
  })

  it('英文各分支都给英文', () => {
    expect(localeFromSystem(['en-US'])).toBe('en-US')
    expect(localeFromSystem(['en-GB'])).toBe('en-US')
  })

  it('德语各分支都给德语', () => {
    expect(localeFromSystem(['de'])).toBe('de')
    expect(localeFromSystem(['de-DE'])).toBe('de')
    expect(localeFromSystem(['DE-at'])).toBe('de')
  })

  it('法语各分支都给法语', () => {
    expect(localeFromSystem(['fr'])).toBe('fr')
    expect(localeFromSystem(['fr-FR'])).toBe('fr')
    expect(localeFromSystem(['FR-ca'])).toBe('fr')
  })

  it('西班牙语各分支都给西班牙语', () => {
    expect(localeFromSystem(['es'])).toBe('es')
    expect(localeFromSystem(['es-ES'])).toBe('es')
    expect(localeFromSystem(['ES-mx'])).toBe('es')
  })

  it('波兰语各分支都给波兰语', () => {
    expect(localeFromSystem(['pl'])).toBe('pl')
    expect(localeFromSystem(['pl-PL'])).toBe('pl')
    expect(localeFromSystem(['PL-pl'])).toBe('pl')
  })

  it('巴西葡萄牙语各分支都给巴西葡萄牙语', () => {
    expect(localeFromSystem(['pt'])).toBe('pt-BR')
    expect(localeFromSystem(['pt-BR'])).toBe('pt-BR')
    expect(localeFromSystem(['PT-br'])).toBe('pt-BR')
  })

  it('土耳其语各分支都给土耳其语', () => {
    expect(localeFromSystem(['tr'])).toBe('tr')
    expect(localeFromSystem(['tr-TR'])).toBe('tr')
    expect(localeFromSystem(['TR-tr'])).toBe('tr')
  })

  it('未支持的语种一律给兜底英文（不猜相似语种：uk 不并到 ru，cs 不并到 pl）', () => {
    expect(localeFromSystem(['uk-UA'])).toBe(FALLBACK_LOCALE)
    expect(localeFromSystem(['cs-CZ'])).toBe(FALLBACK_LOCALE)
    expect(localeFromSystem(['ja-JP'])).toBe(FALLBACK_LOCALE)
    expect(localeFromSystem(['ko-KR'])).toBe(FALLBACK_LOCALE)
  })

  it('取首个有主语言子标签的项，忽略空串', () => {
    expect(localeFromSystem(['', 'ru-RU', 'zh-CN'])).toBe('ru')
    expect(localeFromSystem(['  ', 'zh-CN'])).toBe('zh-CN')
  })

  it('拿不到系统语言时回退简中（与 0.2.4 行为一致）', () => {
    expect(localeFromSystem(undefined)).toBe('zh-CN')
    expect(localeFromSystem([])).toBe('zh-CN')
    expect(localeFromSystem([''])).toBe('zh-CN')
  })
})

describe('语种注册表', () => {
  it('每个语种都有本族语标签且不重复', () => {
    for (const locale of LOCALES) {
      expect(LOCALE_LABELS[locale]?.trim().length ?? 0, `${locale} 缺标签`).toBeGreaterThan(0)
    }
    expect(new Set(Object.values(LOCALE_LABELS)).size).toBe(LOCALES.length)
  })

  it('兜底语言本身必须在注册表里', () => {
    expect(LOCALES as readonly string[]).toContain(FALLBACK_LOCALE)
  })
})
