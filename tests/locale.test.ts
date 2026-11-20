import { describe, expect, it } from 'vitest'
import { localeFromSystem } from '../src/lib/i18n/locale'

describe('首启语言跟随系统', () => {
  it('中文各分支都给简中', () => {
    expect(localeFromSystem(['zh-CN'])).toBe('zh-CN')
    expect(localeFromSystem(['zh-TW'])).toBe('zh-CN')
    expect(localeFromSystem(['zh-HK', 'zh-Hant'])).toBe('zh-CN')
    expect(localeFromSystem(['ZH-cn'])).toBe('zh-CN')
  })

  it('非中文一律英文', () => {
    expect(localeFromSystem(['en-US'])).toBe('en-US')
    expect(localeFromSystem(['en-GB'])).toBe('en-US')
    expect(localeFromSystem(['ru-RU'])).toBe('en-US')
    expect(localeFromSystem(['de'])).toBe('en-US')
  })

  it('取首个有主语言子标签的项，忽略空串', () => {
    expect(localeFromSystem(['', 'ru-RU', 'zh-CN'])).toBe('en-US')
    expect(localeFromSystem(['  ', 'zh-CN'])).toBe('zh-CN')
  })

  it('拿不到系统语言时回退简中（与 0.2.4 行为一致）', () => {
    expect(localeFromSystem(undefined)).toBe('zh-CN')
    expect(localeFromSystem([])).toBe('zh-CN')
    expect(localeFromSystem([''])).toBe('zh-CN')
  })
})
