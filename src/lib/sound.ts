/**
 * Phoenix-DRX 音效统一服务（单一来源）
 * 依据：docs/design/感官层基线.md §一（2026-10-18 裁决变更，参考 tengri-novel 方案）
 * 方案：WebAudio 程序化合成，零音频文件；主音量 0.6 克制。
 * 纪律：组件/页面禁止直连 AudioContext，一律走 playSound(kind)；
 *       新增语义先补基线映射表（§一）再实现。
 */

export type SoundKind = 'nav' | 'ui' | 'toggle' | 'pop' | 'gain' | 'success' | 'alert'

const STORAGE_KEY = 'phx.sound'

let ctx: AudioContext | null = null
let master: GainNode | null = null
let enabled = readEnabled()

function readEnabled(): boolean {
  if (typeof localStorage === 'undefined') return true
  try {
    return localStorage.getItem(STORAGE_KEY) !== '0'
  } catch {
    return true
  }
}

export function isSoundEnabled(): boolean {
  return enabled
}

/** 云同步监听（sync store 注册）：偏好每次落盘都会通知，回环由 sync store 的闸门拦截 */
type SoundChangeFn = (v: boolean) => void
let soundChangeListener: SoundChangeFn | null = null

export function onSoundChange(fn: SoundChangeFn | null): void {
  soundChangeListener = fn
}

export function setSoundEnabled(v: boolean): void {
  enabled = v
  try {
    localStorage.setItem(STORAGE_KEY, v ? '1' : '0')
  } catch {
    /* 持久化失败不影响本次会话开关 */
  }
  soundChangeListener?.(v)
}

function getCtx(): AudioContext | null {
  if (typeof window === 'undefined') return null
  if (!ctx) {
    const AC =
      window.AudioContext ??
      (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext
    if (!AC) return null
    ctx = new AC()
  }
  if (ctx.state === 'suspended') void ctx.resume()
  return ctx
}

function masterGain(c: AudioContext): GainNode {
  if (!master || master.context !== c) {
    master = c.createGain()
    master.gain.value = 0.6
    master.connect(c.destination)
  }
  return master
}

/** 单音：振荡器 + lowpass(3000Hz) + 指数包络（攻击 0.015s、衰减到 0.0001） */
function tone(
  freq: number,
  dur: number,
  vol: number,
  delay = 0,
  type: OscillatorType = 'triangle'
): void {
  const c = getCtx()
  if (!c) return
  const t0 = c.currentTime + delay
  const osc = c.createOscillator()
  osc.type = type
  osc.frequency.value = freq
  const filter = c.createBiquadFilter()
  filter.type = 'lowpass'
  filter.frequency.value = 3000
  const gain = c.createGain()
  gain.gain.setValueAtTime(0.0001, t0)
  gain.gain.exponentialRampToValueAtTime(vol, t0 + 0.015)
  gain.gain.exponentialRampToValueAtTime(0.0001, t0 + dur)
  osc.connect(filter).connect(gain).connect(masterGain(c))
  osc.start(t0)
  osc.stop(t0 + dur + 0.05)
}

/** 暖音色：失谐双正弦振子（freq × 1.005，副振子 0.55 峰值） */
function warmTone(freq: number, dur: number, vol: number, delay = 0): void {
  const c = getCtx()
  if (!c) return
  const t0 = c.currentTime + delay
  const filter = c.createBiquadFilter()
  filter.type = 'lowpass'
  filter.frequency.value = 3000
  const gain = c.createGain()
  gain.gain.setValueAtTime(0.0001, t0)
  gain.gain.exponentialRampToValueAtTime(vol, t0 + 0.015)
  gain.gain.exponentialRampToValueAtTime(0.0001, t0 + dur)
  const osc = c.createOscillator()
  osc.type = 'sine'
  osc.frequency.value = freq
  const osc2 = c.createOscillator()
  osc2.type = 'sine'
  osc2.frequency.value = freq * 1.005
  const g2 = c.createGain()
  g2.gain.value = 0.55
  osc.connect(filter)
  osc2.connect(g2).connect(filter)
  filter.connect(gain).connect(masterGain(c))
  osc.start(t0)
  osc.stop(t0 + dur + 0.05)
  osc2.start(t0)
  osc2.stop(t0 + dur + 0.05)
}

/** 泛音尾：高频 sine 双音衰减（success 收尾用） */
function sparkle(delay = 0): void {
  tone(2093, 0.18, 0.12, delay, 'sine')
  tone(3135.96, 0.22, 0.08, delay + 0.04, 'sine')
}

/** 播放映射表语义音效（感官层基线 §一 交互→音效映射表） */
export function playSound(kind: SoundKind): void {
  if (!enabled) return
  const c = getCtx()
  if (!c) return
  switch (kind) {
    case 'ui':
      tone(880, 0.06, 0.2)
      break
    case 'nav':
      warmTone(523.25, 0.12, 0.35)
      break
    case 'toggle':
      tone(659.25, 0.08, 0.25)
      tone(987.77, 0.1, 0.2, 0.06)
      break
    case 'pop':
      tone(784, 0.06, 0.3)
      tone(1174.7, 0.08, 0.2, 0.05)
      break
    case 'gain':
      tone(523.25, 0.1, 0.3)
      tone(783.99, 0.14, 0.25, 0.08)
      break
    case 'success':
      warmTone(523.25, 0.12, 0.3)
      warmTone(659.25, 0.12, 0.3, 0.09)
      warmTone(783.99, 0.16, 0.3, 0.18)
      tone(1046.5, 0.2, 0.15, 0.27)
      sparkle(0.32)
      break
    case 'alert':
      tone(587.33, 0.15, 0.3)
      tone(392, 0.2, 0.3, 0.12)
      break
  }
}
