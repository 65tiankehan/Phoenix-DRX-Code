import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { describe, expect, it } from 'vitest'

const css = readFileSync(resolve(import.meta.dirname, '../src/app.css'), 'utf-8')

/** 视觉基线（docs/design/视觉基线.md §三）+ 感官层基线（§二）声明的全部 token 名 */
const REQUIRED_TOKENS = [
  // 颜色 26
  '--phx-win-bg',
  '--phx-surface',
  '--phx-surface-2',
  '--phx-fill-tertiary',
  '--phx-fill-quaternary',
  '--phx-fill-secondary',
  '--phx-glass',
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
  // 形状 / 阴影 / 布局 5
  '--phx-r-win',
  '--phx-r-card',
  '--phx-r-btn',
  '--phx-shadow-win',
  '--phx-scrollbar-w',
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

  it('每个 token 只定义一次（无重复定义）', () => {
    const defined = css.match(/--phx-[\w-]+\s*:/g) ?? []
    const counts = new Map<string, number>()
    for (const d of defined) {
      const name = d.replace(/\s*:$/, '')
      counts.set(name, (counts.get(name) ?? 0) + 1)
    }
    const duplicated = [...counts.entries()].filter(([, n]) => n > 1)
    expect(duplicated).toEqual([])
  })

  it('本文件内定义的变量全部带 --phx- 前缀（单一来源纪律）', () => {
    const defined = css.match(/--[\w-]+\s*:/g) ?? []
    const nonPhx = defined.filter((d) => !d.startsWith('--phx-'))
    expect(nonPhx).toEqual([])
  })
})
