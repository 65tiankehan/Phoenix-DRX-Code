import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const css = readFileSync(resolve(import.meta.dirname, '../src/app.css'), 'utf-8')

/** 视觉基线（docs/design/视觉基线.md §三）+ 感官层基线（§二）声明的全部 token 名 */
const REQUIRED_TOKENS = [
  // 颜色 27
  '--phx-win-bg',
  '--phx-surface',
  '--phx-surface-2',
  '--phx-fill-tertiary',
  '--phx-fill-quaternary',
  '--phx-fill-secondary',
  '--phx-glass',
  '--phx-glass-float',
  '--phx-sep',
  '--phx-sep-strong',
  '--phx-text',
  '--phx-text-2',
  '--phx-text-3',
  '--phx-accent',
  '--phx-accent-dim',
  '--phx-warn',
  '--phx-warn-bg',
  '--phx-warn-bd',
  '--phx-danger',
  '--phx-danger-bg',
  '--phx-green',
  '--phx-green-dim',
  '--phx-red',
  '--phx-red-dim',
  '--phx-orange',
  '--phx-orange-dim',
  '--phx-yellow',
  // 形状 / 阴影 4
  '--phx-r-win',
  '--phx-r-card',
  '--phx-r-btn',
  '--phx-shadow-win',
  // 字体 3
  '--phx-font-sans',
  '--phx-font-mono',
  '--phx-font-brand',
  // 动效 6
  '--phx-dur-fast',
  '--phx-dur-norm',
  '--phx-dur-slow',
  '--phx-ease-out',
  '--phx-ease-in',
  '--phx-ease-emphasized',
  // 层级带 4
  '--phx-z-header',
  '--phx-z-overlay',
  '--phx-z-modal',
  '--phx-z-toast'
]

describe('视觉 token 单一来源（tokens.css）', () => {
  it('包含视觉基线声明的全部 token', () => {
    const missing = REQUIRED_TOKENS.filter((t) => !css.includes(`${t}:`))
    expect(missing).toEqual([])
  })

  it('每个 token 只定义一次（暗色基座内无重复，浅色块只做覆盖不引入新名字）', () => {
    // 2026-11-24 起有浅色主题：`:root` 是暗色基座，`:root[data-theme='light']` 是覆盖块。
    // 覆盖块允许重定义同名 token（那是它的职责），但**不许**引入暗色没有的 token —— 否则暗色下会缺值。
    const lightHeader = ":root[data-theme='light']"
    const lightIdx = css.indexOf(lightHeader)
    const darkPart = lightIdx >= 0 ? css.slice(0, lightIdx) : css
    const lightPart = lightIdx >= 0 ? css.slice(lightIdx) : ''

    const names = (part: string): string[] =>
      (part.match(/--phx-[\w-]+\s*:/g) ?? []).map((d) => d.replace(/\s*:$/, ''))

    const duplicatesIn = (part: string): [string, number][] => {
      const counts = new Map<string, number>()
      for (const name of names(part)) counts.set(name, (counts.get(name) ?? 0) + 1)
      return [...counts.entries()].filter(([, n]) => n > 1)
    }

    expect(duplicatesIn(darkPart)).toEqual([])
    expect(duplicatesIn(lightPart)).toEqual([])

    const darkNames = new Set(names(darkPart))
    const introducedByLight = [...new Set(names(lightPart))].filter((n) => !darkNames.has(n))
    expect(introducedByLight).toEqual([])
  })

  it('本文件内定义的变量全部带 --phx- 前缀（单一来源纪律）', () => {
    const defined = css.match(/--[\w-]+\s*:/g) ?? []
    const nonPhx = defined.filter((d) => !d.startsWith('--phx-'))
    expect(nonPhx).toEqual([])
  })
})
