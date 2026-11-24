/**
 * 主题三档：dark（默认）/ light / system。
 * 依据：docs/design/视觉基线.md §三「明暗主题」（2026-11-24 稿图获批后由「仅暗色」改为三档）。
 *
 * 分工：CSS 只认 `data-theme`（src/app.css 的 `:root[data-theme='light']` 覆盖块）；
 *      「跟随系统」在这里解析成具体值再写属性 —— 不在 CSS 里用 prefers-color-scheme，
 *      否则用户手动选暗色时会被系统浅色覆盖。
 */

export type ThemeMode = 'dark' | 'light' | 'system'
export type ResolvedTheme = 'dark' | 'light'

const THEME_KEY = 'phx.theme'

/** 纯函数，便于单测：system 档按系统偏好解析，其余原样返回 */
export function resolveTheme(mode: ThemeMode, prefersLight: boolean): ResolvedTheme {
  if (mode === 'light') return 'light'
  if (mode === 'dark') return 'dark'
  return prefersLight ? 'light' : 'dark'
}

/** 启动即读持久化档位（localStorage 同步读，避免首帧主题闪烁）；没有记录时按暗色，保住老用户观感 */
function initialMode(): ThemeMode {
  try {
    const saved = localStorage.getItem(THEME_KEY)
    if (saved === 'dark' || saved === 'light' || saved === 'system') return saved
  } catch {
    /* 存储不可用时走默认暗色 */
  }
  return 'dark'
}

function lightQuery(): MediaQueryList | null {
  try {
    return window.matchMedia('(prefers-color-scheme: light)')
  } catch {
    return null
  }
}

/** 当前档位与实际生效值（runes 全局状态；切换入口在设置页） */
export const theme = $state<{ mode: ThemeMode; resolved: ResolvedTheme }>({
  mode: initialMode(),
  resolved: 'dark'
})

/** 云同步监听（sync store 注册）：档位每次切换都会通知，回环由 sync store 的闸门拦截 */
type ThemeChangeFn = (mode: ThemeMode) => void
let themeChangeListener: ThemeChangeFn | null = null

export function onThemeChange(fn: ThemeChangeFn | null): void {
  themeChangeListener = fn
}

/** 把当前档位解析成实际值并写到根元素；返回实际生效值 */
export function applyTheme(): ResolvedTheme {
  const resolved = resolveTheme(theme.mode, lightQuery()?.matches ?? false)
  theme.resolved = resolved
  try {
    document.documentElement.dataset.theme = resolved
  } catch {
    /* 非浏览器环境（单测）忽略 */
  }
  return resolved
}

/** 切换档位、持久化并立即生效 */
export function setThemeMode(mode: ThemeMode): void {
  theme.mode = mode
  try {
    localStorage.setItem(THEME_KEY, mode)
  } catch {
    /* 存储不可用时仅内存生效 */
  }
  applyTheme()
  themeChangeListener?.(mode)
}

/** 客户端启动时调一次：应用当前档位，并在 system 档下跟随系统变化 */
export function initTheme(): void {
  applyTheme()
  lightQuery()?.addEventListener('change', () => {
    if (theme.mode === 'system') applyTheme()
  })
}
