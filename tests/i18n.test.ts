import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

type Dict = Record<string, string>

const load = (file: string): Dict =>
  JSON.parse(readFileSync(resolve(import.meta.dirname, '../src/lib/i18n', file), 'utf-8'))

const zhCN = load('zh-CN.json')
const enUS = load('en-US.json')

describe('i18n 多语强一致（硬门禁）', () => {
  it('zh-CN 与 en-US 的 key 集合完全相等', () => {
    const zhKeys = Object.keys(zhCN).sort()
    const enKeys = Object.keys(enUS).sort()
    expect(enKeys).toEqual(zhKeys)
  })

  it('所有 key 与取值均为非空字符串', () => {
    for (const [file, dict] of [
      ['zh-CN.json', zhCN],
      ['en-US.json', enUS]
    ] as const) {
      for (const [key, value] of Object.entries(dict)) {
        expect(key.trim().length, `${file} 有空 key`).toBeGreaterThan(0)
        expect(String(value).trim().length, `${file} 的 ${key} 取值为空`).toBeGreaterThan(0)
      }
    }
  })
})
