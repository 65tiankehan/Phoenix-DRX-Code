/**
 * GitHub 账号与云同步全局状态（19 稿图生产实现；主窗消费，薄窗不参与）。
 *
 * 职责（方案 §5/§6）：会话状态、配对码登录流、合并引擎、防回环闸门、防抖节流冷却。
 * Rust 侧只做 Gist IO（ipc.ts phxAuth/phxSync）；合并决策与触发时机全在前端。
 *
 * 同步语义（方案 §5.2）：
 * - soundEnabled / motionEnabled / locale：last-write-wins（本地脏标记判新旧）
 * - readAnnouncements：并集（只增不减，pull 永远可应用）
 * 触发时机（§6.2）：登录成功/应用启动各 pull 一次；变更防抖 1.5s + 距上次推送 ≥15s；
 * 失败后 60s 冷却（自动路径）；手动同步/重试绕过冷却；登出不推只清场。
 */
import { listen } from '@tauri-apps/api/event'
import { openUrl } from '@tauri-apps/plugin-opener'
import { SvelteSet } from 'svelte/reactivity'
import {
  phxAuth,
  phxSync,
  PHX_AUTH_DEVICE_SUCCESS,
  PHX_AUTH_DEVICE_ERROR,
  PHX_AUTH_SESSION_CHANGED
} from './ipc'
import type { PhxAuthStatus, PhxSyncSettings } from './ipc'
import { isSoundEnabled, onSoundChange, setSoundEnabled } from './sound'
import { isReducedMotion, onMotionChange, setReducedMotion } from './motion'
import { i18n, onLocaleChange, setLocale, t } from './i18n/i18n.svelte'
import type { Locale, MessageKey } from './i18n/i18n.svelte'
import { announcements, importReadIds, onReadIdsChange } from './announcements.svelte'
import { toast } from './toast.svelte'

// —— 节奏常量（方案 §6.2）——
const DEBOUNCE_MS = 1500 // 本地变更防抖
const THROTTLE_MS = 15000 // 距上次推送的最小间隔（超出部分挂 trailing 补推）
const COOLDOWN_MS = 60000 // 失败后自动路径冷却

type SyncKey = 'soundEnabled' | 'motionEnabled' | 'locale' | 'readAnnouncements'

/** 本地待推送键集合（push 成功清空；无 UI 消费，仅引擎内部读写） */
const DIRTY = new SvelteSet<SyncKey>()

/** 镜像：last-write-wins 三键的当前值，用于去重（$effect 回填触发 setter 时同值忽略） */
const mirror: { soundEnabled: boolean; motionEnabled: boolean; locale: Locale } = {
  soundEnabled: true,
  motionEnabled: false,
  locale: 'zh-CN'
}

/** 闸门：远端数据落本地期间为 true，本轮回调不上推（防回环，方案 §4.5） */
let remoteApplying = false

let debounceTimer: ReturnType<typeof setTimeout> | null = null
let trailingTimer: ReturnType<typeof setTimeout> | null = null
let countdownTimer: ReturnType<typeof setInterval> | null = null
let lastPushMs = 0
let cooldownUntil = 0
let pullInFlight = false
let pushInFlight = false
let initialized = false

/** 云同步 UI 状态（设置页 GitHub 账号分区渲染依据） */
export const sync = $state({
  /** 会话首帧是否已拉到（false 时不渲染分区，避免闪烁） */
  sessionLoaded: false,
  configured: false,
  login: null as string | null,
  avatarUrl: null as string | null,
  /** 配对码登录流进行中（null = 无流程；remainingSec 每秒递减，归零自清） */
  flow: null as { userCode: string; verificationUri: string; remainingSec: number } | null,
  /** 最近一次成功同步（pull/pull 均算）本地时间；null = 从未同步 */
  lastSyncMs: null as number | null,
  /** 最近一次同步是否失败（badge err 态） */
  lastError: false,
  /** 手动同步进行中（立即同步/立即重试按钮 loading） */
  syncing: false
})

// —— 合并引擎（方案 §5.2）——

/** 当前本地全量设置，push 载荷用 */
function collectSettings(): PhxSyncSettings {
  return {
    soundEnabled: mirror.soundEnabled,
    motionEnabled: mirror.motionEnabled,
    locale: mirror.locale,
    readAnnouncements: [...announcements.readIds]
  }
}

/** 远端设置落本地（闸门内执行；三键 last-write-wins 且脏键让位本地，readAnnouncements 只增不减） */
function applyRemote(s: PhxSyncSettings): void {
  remoteApplying = true
  try {
    if (
      s.soundEnabled !== undefined &&
      s.soundEnabled !== mirror.soundEnabled &&
      !DIRTY.has('soundEnabled')
    ) {
      mirror.soundEnabled = s.soundEnabled
      setSoundEnabled(s.soundEnabled)
    }
    if (
      s.motionEnabled !== undefined &&
      s.motionEnabled !== mirror.motionEnabled &&
      !DIRTY.has('motionEnabled')
    ) {
      mirror.motionEnabled = s.motionEnabled
      setReducedMotion(s.motionEnabled)
    }
    if (
      (s.locale === 'zh-CN' || s.locale === 'en-US') &&
      s.locale !== mirror.locale &&
      !DIRTY.has('locale')
    ) {
      mirror.locale = s.locale
      setLocale(s.locale)
    }
    // 并集有新增 → 回推远端补齐（只增不减）
    if (s.readAnnouncements && importReadIds(s.readAnnouncements)) {
      DIRTY.add('readAnnouncements')
    }
  } finally {
    remoteApplying = false
  }
}

// —— 推送编排（方案 §6.2）——

/** 本地变更后调：防抖 → 节流（不足间隔挂 trailing 补推）→ 自动推送 */
function schedulePush(): void {
  if (!sync.configured) return
  if (debounceTimer) clearTimeout(debounceTimer)
  debounceTimer = setTimeout(() => {
    debounceTimer = null
    const since = Date.now() - lastPushMs
    if (since < THROTTLE_MS) {
      if (!trailingTimer) {
        trailingTimer = setTimeout(() => {
          trailingTimer = null
          void doPush(true)
        }, THROTTLE_MS - since)
      }
      return
    }
    void doPush(true)
  }, DEBOUNCE_MS)
}

/** 推送；auto=false（手动路径）绕过冷却。push 成功才清 DIRTY（pull 不清）。 */
async function doPush(auto: boolean): Promise<void> {
  if (pushInFlight || DIRTY.size === 0) return
  if (auto && Date.now() < cooldownUntil) return
  pushInFlight = true
  try {
    await phxSync.push(collectSettings())
    DIRTY.clear()
    lastPushMs = Date.now()
    sync.lastSyncMs = lastPushMs
    sync.lastError = false
    cooldownUntil = 0
  } catch {
    cooldownUntil = Date.now() + COOLDOWN_MS
    sync.lastError = true
  } finally {
    pushInFlight = false
  }
}

/** 拉取远端并合并；成功也更新最近同步时间并解除冷却 */
async function doPull(): Promise<void> {
  if (pullInFlight) return
  pullInFlight = true
  try {
    const r = await phxSync.pull()
    if (r.settings) applyRemote(r.settings)
    sync.lastSyncMs = Date.now()
    sync.lastError = false
    cooldownUntil = 0
  } catch {
    sync.lastError = true
  } finally {
    pullInFlight = false
  }
}

/** 手动「立即同步 / 立即重试」：先拉后推（有脏键才推），绕过冷却 */
export async function confirmSync(): Promise<void> {
  if (sync.syncing || !sync.configured) return
  sync.syncing = true
  try {
    await doPull()
    await doPush(false)
  } finally {
    sync.syncing = false
  }
}

// —— 配对码登录流（方案 §4，19 稿图 C 态）——

function stopCountdown(): void {
  if (countdownTimer) {
    clearInterval(countdownTimer)
    countdownTimer = null
  }
}

/** 开始 GitHub 登录：取配对码 → 起倒计时 → 开浏览器 → 剪贴板复制码 */
export async function startLogin(): Promise<void> {
  if (sync.flow || sync.configured) return
  try {
    const d = await phxAuth.deviceStart()
    sync.flow = { userCode: d.userCode, verificationUri: d.verificationUri, remainingSec: d.expiresIn }
    countdownTimer = setInterval(() => {
      if (!sync.flow) {
        stopCountdown()
        return
      }
      sync.flow.remainingSec -= 1
      if (sync.flow.remainingSec <= 0) {
        stopCountdown()
        sync.flow = null
      }
    }, 1000)
    await openUrl(d.verificationUri).catch(() => {})
    try {
      await navigator.clipboard.writeText(d.userCode)
      toast.success(t('sync.codeCopied'))
    } catch {
      /* 剪贴板不可用不阻塞登录 */
    }
  } catch {
    toast.error(t('sync.errGeneric'))
  }
}

/** 取消登录：通知 Rust 停轮询（失败也先收 UI） */
export async function cancelLogin(): Promise<void> {
  if (!sync.flow) return
  try {
    await phxAuth.deviceCancel()
  } catch {
    /* 取消失败也先收 UI，Rust 轮询自会结束 */
  }
  stopCountdown()
  sync.flow = null
}

/** 退出登录（Rust 清凭据 + 广播清场事件） */
export async function logout(): Promise<void> {
  try {
    await phxAuth.logout()
  } catch {
    toast.error(t('sync.logoutFailed'))
  }
}

/** device-error 码 → i18n 键（Rust message 是技术串，前端按码映射） */
function deviceErrorKey(code: string): MessageKey {
  switch (code) {
    case 'expired':
      return 'sync.errExpired'
    case 'denied':
      return 'sync.errDenied'
    case 'network':
      return 'sync.errNetwork'
    case 'profile':
      return 'sync.errProfile'
    case 'store':
      return 'sync.errStore'
    default:
      return 'sync.errGeneric'
  }
}

// —— 初始化与事件接线（方案 §6.1）——

/** 注册四个模块钩子：本地变更 → 标脏 + 上推（闸门内回调忽略，镜像同值忽略） */
function registerHooks(): void {
  onSoundChange((v) => {
    if (remoteApplying || v === mirror.soundEnabled) return
    mirror.soundEnabled = v
    DIRTY.add('soundEnabled')
    schedulePush()
  })
  onMotionChange((v) => {
    if (remoteApplying || v === mirror.motionEnabled) return
    mirror.motionEnabled = v
    DIRTY.add('motionEnabled')
    schedulePush()
  })
  onLocaleChange((l) => {
    if (remoteApplying || l === mirror.locale) return
    mirror.locale = l
    DIRTY.add('locale')
    schedulePush()
  })
  onReadIdsChange(() => {
    if (remoteApplying) return
    DIRTY.add('readAnnouncements')
    schedulePush()
  })
}

/** 主窗初始化（+layout onMount 非薄窗分支调用）：镜像初始化 → 钩子 → 事件 → 会话首拉 */
export async function initSync(): Promise<void> {
  if (initialized) return
  initialized = true
  mirror.soundEnabled = isSoundEnabled()
  mirror.motionEnabled = isReducedMotion()
  mirror.locale = i18n.locale
  registerHooks()

  void listen<PhxAuthStatus>(PHX_AUTH_DEVICE_SUCCESS, (e) => {
    const s = e.payload
    sync.configured = true
    sync.login = s.login
    sync.avatarUrl = s.avatarUrl
    stopCountdown()
    sync.flow = null
    void doPull()
  })
  void listen<{ code: string }>(PHX_AUTH_DEVICE_ERROR, (e) => {
    stopCountdown()
    sync.flow = null
    toast.error(t(deviceErrorKey(e.payload.code)))
  })
  void listen<{ configured: boolean }>(PHX_AUTH_SESSION_CHANGED, (e) => {
    if (e.payload.configured) return
    // 登出清场：状态归零、待推数据丢弃（登出不推只清，方案 §6.2）
    sync.configured = false
    sync.login = null
    sync.avatarUrl = null
    sync.flow = null
    sync.lastSyncMs = null
    sync.lastError = false
    DIRTY.clear()
  })

  try {
    const s = await phxAuth.status()
    sync.configured = s.configured
    sync.login = s.login
    sync.avatarUrl = s.avatarUrl
  } catch {
    /* 读不到按未登录 */
  }
  sync.sessionLoaded = true
  if (sync.configured) void doPull()
}
