import { invoke } from '@tauri-apps/api/core'

/**
 * 与旧 Electron preload 同语义的 Tauri invoke 封装。
 * 命名约定：旧 IPC 通道 `server:pick` → Rust 命令 `server_pick`（t11 落地）。
 * 窗口操作不在此层：直接使用 @tauri-apps/api/window（见 PhxTitlebar）。
 *
 * 领域铁律：profile_save 在 Rust 侧必须先落备份再写档案。
 */

export type PhxServerCheck = 'ok' | 'exe' | 'profiles'

export interface PhxServerStatus {
  check: PhxServerCheck
  profileCount: number
  name: string
}

export interface PhxServerRow extends PhxServerStatus {
  id: string
  path: string
}

export interface PhxAddResult {
  id: string
  duplicate: boolean
  status: PhxServerStatus
}

export interface PhxProfileMeta {
  id: string
  nickname: string
  side: string
  level: number
}

export interface PhxProfileHero extends PhxProfileMeta {
  gameVersion: string
  rub: string
  usd: string
  eur: string
}

export interface PhxSaveResult {
  backupPath: string | null
}

export const phxServer = {
  pick: (): Promise<string | null> => invoke('server_pick'),
  add: (path: string): Promise<PhxAddResult> => invoke('server_add', { path }),
  list: (): Promise<PhxServerRow[]> => invoke('server_list'),
  remove: (id: string): Promise<void> => invoke('server_remove', { id }),
  use: (id: string): Promise<boolean> => invoke('server_use', { id }),
  current: (): Promise<PhxServerRow | null> => invoke('server_current')
}

export const phxProfile = {
  list: (): Promise<PhxProfileMeta[]> => invoke('profile_list'),
  load: (id: string): Promise<PhxProfileHero | null> => invoke('profile_load', { id }),
  save: (id: string): Promise<PhxSaveResult> => invoke('profile_save', { id }),
  current: (): Promise<string | null> => invoke('profile_current'),
  use: (id: string): Promise<void> => invoke('profile_use', { id })
}

/** 应用级标志（启动页/引导页首启编排） */
export interface PhxAppFlags {
  splashSeen: boolean
  onboarded: boolean
}

export const phxApp = {
  flags: (): Promise<PhxAppFlags> => invoke('app_flags'),
  markSplashSeen: (): Promise<void> => invoke('mark_splash_seen'),
  markOnboarded: (): Promise<void> => invoke('mark_onboarded')
}

/** 托盘面板（自绘弹窗，/tray 路由）；几何定位与收起均由 Rust 侧裁决 */
export const phxTray = {
  showMain: (): Promise<void> => invoke('app_show_main'),
  quit: (): Promise<void> => invoke('app_quit'),
  openProfilesDir: (): Promise<void> => invoke('tray_open_profiles_dir'),
  hide: (): Promise<void> => invoke('tray_hide'),
  reportHeight: (h: number): Promise<void> => invoke('tray_report_height', { h })
}

// —— 设置页（15-settings）——

/** 问题处理三态：ask 询问我如何处理 / fix 自动修复 / ignore 自动忽略（对齐参考枚举 IssuesAction） */
export type PhxIssueHandling = 'ask' | 'fix' | 'ignore'

export interface PhxSettings {
  /** 自动补全缺失的武器精通/专精等级（对齐 Rust Settings autoAddMasterings） */
  autoAddMasterings: boolean
  /** 自动补全缺失的 Scav 技能等级（对齐 Rust Settings autoAddScavSkills） */
  autoAddScavSkills: boolean
  issueHandling: PhxIssueHandling
}

export const phxSettings = {
  get: (): Promise<PhxSettings> => invoke('settings_get'),
  set: (settings: PhxSettings): Promise<void> => invoke('settings_set', { settings }),
  openDataDir: (): Promise<void> => invoke('settings_open_data_dir'),
  reset: (): Promise<void> => invoke('settings_reset'),
  restart: (): Promise<void> => invoke('settings_restart')
}

// —— 角色页（03-character）——

export interface PhxCharVital {
  current: number
  maximum: number
}

export interface PhxCharHealth {
  head: PhxCharVital
  chest: PhxCharVital
  stomach: PhxCharVital
  leftArm: PhxCharVital
  rightArm: PhxCharVital
  leftLeg: PhxCharVital
  rightLeg: PhxCharVital
  hydration: PhxCharVital
  energy: PhxCharVital
}

export interface PhxCharSide {
  side: string
  nickname: string
  level: number
  experience: number
  head: string
  voice: string
  health: PhxCharHealth
  pocketsTpl: string
}

export interface PhxCharData {
  side: string
  aid: string
  gameVersion: string
  pmc: PhxCharSide
  scav: PhxCharSide
}

export interface PhxCharOptionItem {
  id: string
  name: string
}

export interface PhxCharPocketItem {
  id: string
  name: string
  slots: number
  specialSlots: number
}

export interface PhxCharOptions {
  expTable: number[]
  voices: PhxCharOptionItem[]
  heads: PhxCharOptionItem[]
  pockets: PhxCharPocketItem[]
}

export interface PhxCharSaveData {
  pmc: PhxCharSide
  scav: PhxCharSide
}

/** 角色页：读全量 / 选项 / 写回（写回侧 Rust 先落备份） */
export const phxChar = {
  load: (id: string): Promise<PhxCharData | null> => invoke('char_load', { id }),
  options: (): Promise<PhxCharOptions> => invoke('char_options'),
  save: (id: string, data: PhxCharSaveData): Promise<PhxSaveResult> =>
    invoke('char_save', { id, data })
}

// —— 商人页（04-traders）——

/** base.json 单等级段下限（前端联动口径：升级补下限 / 手改反推等级） */
export interface PhxTraderLevel {
  minSalesSum: number
  minStanding: number
}

export interface PhxTraderRow {
  id: string
  /** 商人原名（base.json），缺失为空串 */
  nickname: string
  loyaltyLevel: number
  salesSum: number
  standing: number
  unlocked: boolean
  /** 等级上限（base.json 等级段数） */
  maxLevel: number
  /** 各等级段下限（base.json 缺失为空，联动退化为保持当前值） */
  levels: PhxTraderLevel[]
}

export interface PhxTradersData {
  traders: PhxTraderRow[]
  ragfair: { rating: number }
}

export interface PhxTraderSaveRow {
  id: string
  loyaltyLevel: number
  salesSum: number
  standing: number
  unlocked: boolean
}

export interface PhxTradersSaveData {
  traders: PhxTraderSaveRow[]
  ragfair: number
}

/** 商人页：读全量 / 写回（写回侧 Rust 先落备份，Fence 自动同步 Scav 好感） */
export const phxTraders = {
  load: (id: string): Promise<PhxTradersData | null> => invoke('traders_load', { id }),
  save: (id: string, data: PhxTradersSaveData): Promise<PhxSaveResult> =>
    invoke('traders_save', { id, data })
}

// —— 任务页（05-quests）——

export interface PhxQuestRow {
  qid: string
  type: string // Standart / Weekly / Daily / Daily_Savage
  trader: string // 查无 "unknown"
  status: string
  startTime: number
  statusTimers: Record<string, number>
  questName: string
  unlockHint?: string
}

export interface PhxQuestsData {
  rows: PhxQuestRow[]
  missing: PhxQuestRow[] // templates − 档案 − 事件任务，带判型与名字，供补齐缺失
}

export interface PhxQuestsSaveData {
  rows: { qid: string; status: string }[]
}

/** 任务页：读全量 / 写回（写回侧 Rust 先落备份，状态修正 + 联动配方增量在 Rust 侧完成） */
export const phxQuests = {
  load: (id: string): Promise<PhxQuestsData | null> => invoke('quests_load', { id }),
  save: (id: string, data: PhxQuestsSaveData): Promise<PhxSaveResult> =>
    invoke('quests_save', { id, data })
}

// —— 技能页（06-skills）——

export interface PhxSkillRow {
  id: string
  name: string // 本地化名；无映射回退 Id（BotReload/BotSound 即此形态）
  progress: number
  cap: number // 普通技能 5100；武器熟练 = globals Level2+Level3
  bot: boolean // bot 开头的机器人内部技能，「全部拉满」跳过
}

export interface PhxSkillSideData {
  common: PhxSkillRow[]
  mastering: PhxSkillRow[]
}

export interface PhxSkillsData {
  pmc: PhxSkillSideData
  scav: PhxSkillSideData
}

export interface PhxSkillsSaveRow {
  side: string // pmc / scav
  group: string // Common / Mastering
  id: string
  progress: number
}

export interface PhxSkillsSaveData {
  rows: PhxSkillsSaveRow[]
}

/** 技能页：读全量 / 写回（写回侧 Rust 先落备份，Progress 夹取 0..cap 在 Rust 侧完成） */
export const phxSkills = {
  load: (id: string): Promise<PhxSkillsData | null> => invoke('skills_load', { id }),
  save: (id: string, data: PhxSkillsSaveData): Promise<PhxSaveResult> =>
    invoke('skills_save', { id, data })
}

// —— 健康页（07-health）——

export interface PhxHealthVital {
  current: number
  maximum: number
}

export interface PhxHealthSide {
  head: PhxHealthVital
  chest: PhxHealthVital
  stomach: PhxHealthVital
  leftArm: PhxHealthVital
  rightArm: PhxHealthVital
  leftLeg: PhxHealthVital
  rightLeg: PhxHealthVital
  hydration: PhxHealthVital
  energy: PhxHealthVital
}

export interface PhxHealthData {
  pmc: PhxHealthSide
  scav: PhxHealthSide
}

export interface PhxHealthSaveData {
  pmc: PhxHealthSide
  scav: PhxHealthSide
}

/** 健康页：读全量 / 写回（写回侧 Rust 先落备份，当前 > 上限时上限抬高在 Rust 侧兜底） */
export const phxHealth = {
  load: (id: string): Promise<PhxHealthData | null> => invoke('health_load', { id }),
  save: (id: string, data: PhxHealthSaveData): Promise<PhxSaveResult> =>
    invoke('health_save', { id, data })
}

// —— 藏身处页（08-hideout，仅 PMC）——

export interface PhxHideoutAreaRow {
  type: number
  level: number
  maxLevel: number // areas.json stages 数 − 1
  name: string
  parentType: number | null // 父区域 type（父子联动用）
}

export interface PhxHideoutRecipeRow {
  id: string
  name: string // 最终产品本地化名；无映射回退模板 id
  productTpl: string
  area: string
  added: boolean // 已在存档解锁清单（UnlockedInfo.unlockedProductionRecipe）
}

export interface PhxHideoutCraftRow {
  id: string
  name: string
  productTpl: string
  area: string
  finished: boolean // progress ≥ productionTime（参考 IsFinished）
  progress: number // 单位小时（参考 SetFinished 折算口径）
  productionTime: number
  startTimestamp: number
}

export interface PhxHideoutData {
  areas: PhxHideoutAreaRow[]
  recipes: PhxHideoutRecipeRow[]
  crafts: PhxHideoutCraftRow[]
}

export interface PhxHideoutAreaSet {
  type: number
  level: number
}

export interface PhxHideoutCraftSet {
  recipeId: string
  progress: number
  productionTime: number
  startTimestamp: number
}

export interface PhxHideoutSaveData {
  areas: PhxHideoutAreaSet[]
  unlocked: string[]
  crafts: PhxHideoutCraftSet[]
}

/** 藏身处页：读全量 / 写回（写回侧 Rust 先落备份；区域父子联动与卡控在 Rust 侧兜底） */
export const phxHideout = {
  load: (id: string): Promise<PhxHideoutData | null> => invoke('hideout_load', { id }),
  save: (id: string, data: PhxHideoutSaveData): Promise<PhxSaveResult> =>
    invoke('hideout_save', { id, data })
}

// —— 仓库页（09-stash，仅 PMC）——

export interface PhxStashItemRow {
  id: string
  name: string
  tpl: string
  count: number // StackObjectsCount，缺省 1
  modded: boolean // tpl 不在服务器物品库
  isWeapon: boolean
  isContainer: boolean
}

export interface PhxEquipSlotRow {
  slot: string // 存档 slotId 字段名（前端走 i18n 槽名映射）
  item: PhxStashItemRow | null
}

export interface PhxAddCategoryRow {
  id: string
  name: string
  parentId: string | null
}

export interface PhxAddItemRow {
  tpl: string
  name: string
  desc: string
  categoryId: string
}

export interface PhxStashData {
  rubles: number
  dollars: number
  euros: number
  bonusRows: number
  hasModded: boolean
  items: PhxStashItemRow[]
  equipment: PhxEquipSlotRow[]
  pockets: PhxStashItemRow[]
  categories: PhxAddCategoryRow[]
  addable: PhxAddItemRow[]
}

export type PhxStashOp =
  | { op: 'addMoney'; tpl: string; amount: number }
  | { op: 'setBonusRows'; count: number }
  | { op: 'removeItems'; ids: string[] }
  | { op: 'addItem'; tpl: string }
  | { op: 'addBuild'; presetId: string }

export interface PhxStashSaveData {
  ops: PhxStashOp[]
}

/** 仓库页：读全量 / 写回（写回侧 Rust 先落备份，op 列表逐条执行；放置引擎在 Rust 侧） */
export const phxStash = {
  load: (id: string): Promise<PhxStashData | null> => invoke('stash_load', { id }),
  save: (id: string, data: PhxStashSaveData): Promise<PhxSaveResult> =>
    invoke('stash_save', { id, data })
}

/** Scav 装备 / 口袋页数据（装备槽 + 口袋物品 + 口袋容器名；与 PMC 仓库装备同构） */
export interface PhxScavData {
  equipment: PhxEquipSlotRow[]
  pockets: PhxStashItemRow[]
  pocketsName: string
}

/** Scav 写回只支持删除（行内删除与清空装备共用） */
export type PhxScavOp = { op: 'removeItems'; ids: string[] }

export interface PhxScavSaveData {
  ops: PhxScavOp[]
}

/** Scav 装备 / 口袋页：读全量 / 写回（写回侧 Rust 先落备份） */
export const phxScav = {
  load: (id: string): Promise<PhxScavData | null> => invoke('scav_load', { id }),
  save: (id: string, data: PhxScavSaveData): Promise<PhxSaveResult> =>
    invoke('scav_save', { id, data })
}

export interface PhxBuildPartRow {
  id: string
  name: string
  tpl: string
  slotId: string
  modded: boolean
}

/** 武器改装视图：根武器基础值 + 在库零件累加（后坐力百分比公式在 Rust 侧） */
export interface PhxBuildView {
  id: string
  name: string
  rootTpl: string
  rootName: string
  ergonomics: number
  recoilUp: number
  recoilBack: number
  hasModded: boolean
  parts: PhxBuildPartRow[]
}

/** 检视 / 图鉴页：检视档案内武器 + 服务器全局武器预设（只读）+ 预设写回与导出 */
export const phxInspect = {
  inspect: (id: string, itemId: string): Promise<PhxBuildView | null> =>
    invoke('inspect_weapon', { id, itemId }),
  loadGlobalBuilds: (): Promise<PhxBuildView[]> => invoke('builds_global'),
  presetAdd: (id: string, itemId: string): Promise<void> => invoke('preset_add', { id, itemId }),
  exportPreset: (path: string, presetId: string): Promise<void> =>
    invoke('preset_export', { path, presetId }),
  exportWeapon: (id: string, itemId: string, path: string): Promise<void> =>
    invoke('weapon_export', { id, itemId, path })
}

/** 装备预设视图：13 槽（含口袋容器内直接子项）+ 是否含服务器外物品 */
export interface PhxEquipmentPresetView {
  id: string
  name: string
  slots: PhxEquipSlotRow[]
  pockets: PhxStashItemRow[]
  hasModded: boolean
}

export interface PhxPresetsData {
  weapons: PhxBuildView[]
  equipment: PhxEquipmentPresetView[]
}

export interface PhxImportResult {
  imported: number
  failed: string[]
  backupPath: string | null
}

/** 预设页：用户武器 / 装备预设的读取、增删、入仓、导入导出（写入侧 Rust 先落备份） */
export const phxPresets = {
  load: (id: string): Promise<PhxPresetsData | null> => invoke('presets_load', { id }),
  remove: (id: string, kind: 'weapon' | 'equipment', target: string): Promise<PhxSaveResult> =>
    invoke('build_remove', { id, kind, target }),
  removeAll: (id: string, kind: 'weapon' | 'equipment'): Promise<PhxSaveResult> =>
    invoke('builds_remove_all', { id, kind }),
  toStash: (id: string, kind: 'weapon' | 'equipment', target: string): Promise<PhxSaveResult> =>
    invoke('build_to_stash', { id, kind, target }),
  import: (id: string, kind: 'weapon' | 'equipment', paths: string[]): Promise<PhxImportResult> =>
    invoke('build_import', { id, kind, paths }),
  export: (id: string, kind: 'weapon' | 'equipment', target: string, path: string): Promise<void> =>
    invoke('build_export', { id, kind, target, path }),
  exportAll: (id: string, kind: 'weapon' | 'equipment', dir: string): Promise<number> =>
    invoke('builds_export_all', { id, kind, dir })
}
