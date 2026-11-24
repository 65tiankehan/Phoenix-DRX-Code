import { describe, expect, it } from 'vitest'
import { resolveTheme } from '../src/lib/theme.svelte'

describe('主题档位解析', () => {
  it('显式暗色不受系统影响', () => {
    expect(resolveTheme('dark', false)).toBe('dark')
    expect(resolveTheme('dark', true)).toBe('dark')
  })

  it('显式浅色不受系统影响', () => {
    expect(resolveTheme('light', false)).toBe('light')
    expect(resolveTheme('light', true)).toBe('light')
  })

  it('跟随系统时按系统偏好取实际值', () => {
    expect(resolveTheme('system', true)).toBe('light')
    expect(resolveTheme('system', false)).toBe('dark')
  })
})
