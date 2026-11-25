import { readFileSync, readdirSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'
import { FALLBACK_LOCALE, LOCALES } from '../src/lib/i18n/locale'

type Dict = Record<string, string>

const DIR = resolve(import.meta.dirname, '../src/lib/i18n')

const load = (file: string): Dict => JSON.parse(readFileSync(resolve(DIR, file), 'utf-8'))

/** 基准全集：英文（FALLBACK_LOCALE 指向它，任何语种都必须是它的超集/等集） */
const base = load(`${FALLBACK_LOCALE}.json`)
const baseKeys = Object.keys(base)

const dicts = LOCALES.map((l) => [l, load(`${l}.json`)] as const)

const placeholders = (s: string): string[] => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort()

describe('i18n 多语强一致（硬门禁）', () => {
  it('目录里的词典文件与 LOCALES 注册表一一对应', () => {
    const files = readdirSync(DIR)
      .filter((f) => f.endsWith('.json'))
      .map((f) => f.replace(/\.json$/, ''))
      .sort()
    expect(files).toEqual([...LOCALES].sort())
  })

  it('每个语种的 key 集合与基准全集完全相等', () => {
    for (const [locale, dict] of dicts) {
      expect(Object.keys(dict).sort(), `${locale} 的 key 集合与 ${FALLBACK_LOCALE} 不一致`).toEqual(
        [...baseKeys].sort()
      )
    }
  })

  it('每个语种的 key 顺序与基准一致（保证逐行对照翻译时不会错位）', () => {
    for (const [locale, dict] of dicts) {
      expect(Object.keys(dict), `${locale} 的 key 顺序与 ${FALLBACK_LOCALE} 不一致`).toEqual(baseKeys)
    }
  })

  it('所有 key 与取值均为非空字符串', () => {
    for (const [locale, dict] of dicts) {
      for (const [key, value] of Object.entries(dict)) {
        expect(key.trim().length, `${locale} 有空 key`).toBeGreaterThan(0)
        expect(String(value).trim().length, `${locale} 的 ${key} 取值为空`).toBeGreaterThan(0)
      }
    }
  })

  it('占位符与基准逐条相同（漏译 {n} 会在运行时露出裸花括号）', () => {
    for (const [locale, dict] of dicts) {
      for (const key of baseKeys) {
        expect(placeholders(dict[key]), `${locale} 的 ${key} 占位符与基准不一致`).toEqual(
          placeholders(base[key])
        )
      }
    }
  })

  it('语种之间不串文字：非中文语种里不许出现汉字，中文与英文里不许出现西里尔字母', () => {
    const cjk = /[\u4e00-\u9fff]/
    const cyrillic = /[\u0400-\u04ff]/
    for (const [locale, dict] of dicts) {
      for (const [key, value] of Object.entries(dict)) {
        if (locale !== 'zh-CN') {
          expect(cjk.test(value), `${locale} 的 ${key} 混入了汉字：${value}`).toBe(false)
        } else {
          expect(cyrillic.test(value), `${locale} 的 ${key} 混入了西里尔字母：${value}`).toBe(false)
        }
      }
    }
  })
})
